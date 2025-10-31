//! Interplanetary cruise phase: integrates the heliocentric transfer leg using the selected propulsion model.

use solar_config::PlanetConfig;
use solar_ephem_spice::{self as ephemeris, StateVector};
use solar_impulsive::{lambert, transfers::hohmann};
use solar_orbits::norm3;
use solar_propulsion::{PropulsionMode, Vehicle};

const MU_SUN: f64 = 1.327_124_400_18e11; // km^3 / s^2
const SECONDS_PER_DAY: f64 = 86_400.0;

mod continuous;

/// Configuration for the interplanetary leg.
#[derive(Debug, Clone)]
pub struct InterplanetaryConfig {
    pub departure_body: String,
    pub destination_body: String,
    pub departure_epoch: String,
    pub arrival_epoch: Option<String>,
    pub propulsion_mode: PropulsionMode,
}

/// Result from planning the cruise leg.
#[derive(Debug, Clone)]
pub struct InterplanetaryPlan {
    pub time_of_flight_days: f64,
    pub propellant_used_kg: Option<f64>,
    pub departure_state: StateVector,
    pub arrival_state: StateVector,
    pub peak_speed_km_s: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum InterplanetaryError {
    #[error("ephemeris lookup failed: {0}")]
    Ephemeris(#[from] ephemeris::EphemerisError),
    #[error("propulsion mode not implemented yet")]
    UnsupportedPropulsion,
    #[error("continuous-thrust solver requires positive acceleration data")]
    InvalidAcceleration,
    #[error("continuous-thrust solver requires positive specific impulse")]
    InvalidSpecificImpulse,
}

/// Propagates the interplanetary leg between the origin and destination bodies.
///
/// Continuous-thrust missions are integrated with a steering-aware solver that considers
/// solar gravity, thrust limits, and propellant consumption. Impulsive missions currently
/// return analytic placeholders until the impulsive solver is implemented.
pub fn plan_interplanetary(
    vehicle: &Vehicle,
    config: &InterplanetaryConfig,
    origin: &PlanetConfig,
    destination: &PlanetConfig,
) -> Result<InterplanetaryPlan, InterplanetaryError> {
    let dep_target = ephemeris::normalize_heliocentric_target_name(&config.departure_body);
    let arr_target = ephemeris::normalize_heliocentric_target_name(&config.destination_body);
    let departure_et = ephemeris::epoch_seconds(&config.departure_epoch)?;

    let departure_state =
        ephemeris::state_vector_et(&dep_target, "SUN", "ECLIPJ2000", "NONE", departure_et)?;
    let destination_state_at_departure =
        ephemeris::state_vector_et(&arr_target, "SUN", "ECLIPJ2000", "NONE", departure_et)?;

    match &config.propulsion_mode {
        PropulsionMode::Continuous { .. } => {
            let arrival_et = if let Some(epoch) = &config.arrival_epoch {
                ephemeris::epoch_seconds(epoch)?
            } else {
                departure_et
            };
            let arrival_state =
                ephemeris::state_vector_et(&arr_target, "SUN", "ECLIPJ2000", "NONE", arrival_et)?;

            continuous::solve(
                vehicle,
                config,
                origin,
                destination,
                departure_state,
                arrival_state,
            )
        }
        PropulsionMode::Impulsive { .. } | PropulsionMode::Hybrid => {
            let (_arrival_et, tof_seconds, arrival_state) = if let Some(epoch) =
                &config.arrival_epoch
            {
                let arrival_et = ephemeris::epoch_seconds(epoch)?;
                let arrival_state = ephemeris::state_vector_et(
                    &arr_target,
                    "SUN",
                    "ECLIPJ2000",
                    "NONE",
                    arrival_et,
                )?;
                (arrival_et, (arrival_et - departure_et).abs(), arrival_state)
            } else {
                let r1 = norm3(&departure_state.position_km);
                let r2 = norm3(&destination_state_at_departure.position_km);
                let hohmann = hohmann(r1, r2, MU_SUN);
                let baseline_tof = if hohmann.tof_seconds.is_finite() && hohmann.tof_seconds > 0.0 {
                    hohmann.tof_seconds
                } else {
                    200.0 * 86_400.0
                };

                match optimize_impulsive_arrival(
                    departure_et,
                    &departure_state,
                    &arr_target,
                    baseline_tof,
                )? {
                    Some((best_arrival_et, best_arrival_state, best_tof)) => {
                        (best_arrival_et, best_tof, best_arrival_state)
                    }
                    None => {
                        let fallback_et = departure_et + baseline_tof;
                        let fallback_state = ephemeris::state_vector_et(
                            &arr_target,
                            "SUN",
                            "ECLIPJ2000",
                            "NONE",
                            fallback_et,
                        )?;
                        (fallback_et, baseline_tof, fallback_state)
                    }
                }
            };

            let tof_days = tof_seconds / 86_400.0;

            let depart_speed = norm3(&departure_state.velocity_km_s);
            let arrival_speed = norm3(&arrival_state.velocity_km_s);
            let peak_speed = depart_speed.max(arrival_speed);

            Ok(InterplanetaryPlan {
                time_of_flight_days: tof_days,
                propellant_used_kg: None,
                departure_state,
                arrival_state,
                peak_speed_km_s: Some(peak_speed),
            })
        }
    }
}

fn lambert_vinf_score(
    departure_state: &StateVector,
    arrival_state: &StateVector,
    tof_seconds: f64,
    short: bool,
) -> Option<f64> {
    let (v1, v2) = lambert::solve(
        departure_state.position_km,
        arrival_state.position_km,
        tof_seconds,
        MU_SUN,
        short,
    )
    .ok()?;

    let vinf_depart = [
        v1[0] - departure_state.velocity_km_s[0],
        v1[1] - departure_state.velocity_km_s[1],
        v1[2] - departure_state.velocity_km_s[2],
    ];
    let vinf_arrive = [
        v2[0] - arrival_state.velocity_km_s[0],
        v2[1] - arrival_state.velocity_km_s[1],
        v2[2] - arrival_state.velocity_km_s[2],
    ];

    let score = norm3(&vinf_depart) + norm3(&vinf_arrive);
    Some(score)
}

fn optimize_impulsive_arrival(
    departure_et: f64,
    departure_state: &StateVector,
    arrival_target: &str,
    baseline_tof_seconds: f64,
) -> Result<Option<(f64, StateVector, f64)>, ephemeris::EphemerisError> {
    let mut baseline_days = (baseline_tof_seconds / SECONDS_PER_DAY).abs();
    if !baseline_days.is_finite() || baseline_days < 1.0 {
        baseline_days = 200.0;
    }

    let mut min_days = (baseline_days * 0.5).max(30.0);
    let mut max_days = (baseline_days * 1.8).max(min_days + 30.0).min(1_500.0);
    let mut step_days = ((max_days - min_days) / 120.0).max(2.0);

    let mut best: Option<(f64, StateVector, f64, f64)> = None;

    for _ in 0..3 {
        let mut tof_days = min_days;
        while tof_days <= max_days + 1e-6 {
            let tof_seconds = tof_days * SECONDS_PER_DAY;
            let arrival_et = departure_et + tof_seconds;
            let arrival_state = match ephemeris::state_vector_et(
                arrival_target,
                "SUN",
                "ECLIPJ2000",
                "NONE",
                arrival_et,
            ) {
                Ok(state) => state,
                Err(_) => {
                    tof_days += step_days;
                    continue;
                }
            };

            let mut best_score_for_candidate: Option<f64> = None;
            if let Some(score) =
                lambert_vinf_score(departure_state, &arrival_state, tof_seconds, true)
            {
                best_score_for_candidate = Some(score);
            }
            if let Some(score) =
                lambert_vinf_score(departure_state, &arrival_state, tof_seconds, false)
            {
                if best_score_for_candidate.map_or(true, |current| score < current) {
                    best_score_for_candidate = Some(score);
                }
            }

            if let Some(score) = best_score_for_candidate {
                match &mut best {
                    Some(existing) => {
                        if score < existing.3 {
                            *existing = (arrival_et, arrival_state.clone(), tof_seconds, score);
                        }
                    }
                    None => {
                        best = Some((arrival_et, arrival_state.clone(), tof_seconds, score));
                    }
                }
            }

            tof_days += step_days;
        }

        if let Some((_, _, best_tof_seconds, _)) = best {
            let best_days = best_tof_seconds / SECONDS_PER_DAY;
            min_days = (best_days - step_days * 4.0).max(30.0);
            max_days = (best_days + step_days * 4.0)
                .min((baseline_days * 2.2).max(best_days + 20.0).min(1_500.0));
            if max_days <= min_days + 1.0 {
                max_days = min_days + 1.0;
            }
            step_days = (step_days / 2.0).max(0.25);
        } else {
            break;
        }
    }

    Ok(best.map(|(et, state, tof, _)| (et, state, tof)))
}
