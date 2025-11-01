//! Arrival phase: capture into destination parking orbit, optionally with aerobraking support.

use solar_config::PlanetConfig;
use solar_ephem_spice::{self as ephemeris, EphemerisError};

use super::interplanetary::{InterplanetaryConfig, InterplanetaryPlan};
use solar_aerobrake::{
    AerobrakeRequest as AeroRequest, PlanetEntryContext as AeroPlanet,
    VehicleEntryContext as AeroVehicle, simulate_ballistic_pass,
};
use solar_impulsive::lambert;
use solar_orbits::{capture_delta_v, norm3};
use solar_propulsion::{PropulsionMode, Vehicle};

const MU_SUN: f64 = 1.327_124_400_18e11;
const MAX_AEROBRAKE_DYNAMIC_PRESSURE_PA: f64 = 80_000.0;
const MAX_AEROBRAKE_ACCEL_M_S2: f64 = 39.24; // ≈ 4 g

/// Aerobraking option describing whether atmospheric drag can reduce capture delta-v.
#[derive(Debug, Clone, Copy)]
pub enum AerobrakingOption {
    /// No aerobraking; perform a purely propulsive capture burn.
    Disabled,
    /// Attempt partial aerobraking followed by a trim burn. Value is the desired periapsis altitude (km).
    Partial { periapsis_altitude_km: f64 },
    /// Attempt full aerocapture with target periapsis altitude (km). Fall back to propulsive if constraints fail.
    Full { periapsis_altitude_km: f64 },
}

/// Arrival configuration.
#[derive(Debug, Clone)]
pub struct ArrivalConfig {
    pub destination_body: String,
    pub target_parking_altitude_km: f64,
    pub encounter_epoch: String,
    pub propulsion_mode: PropulsionMode,
    pub aerobraking: Option<AerobrakingOption>,
}

/// Result of the arrival planning phase.
#[derive(Debug, Clone)]
pub struct ArrivalPlan {
    pub delta_v_required: f64,
    pub burn_duration_s: Option<f64>,
    pub aerobraking: Option<AerobrakingOption>,
    pub aerobrake_report: Option<AerobrakeReport>,
}

/// Diagnostic data describing an aerobraking pass.
#[derive(Debug, Clone)]
pub struct AerobrakeReport {
    pub delta_v_drag_km_s: f64,
    pub final_vinf_km_s: f64,
    pub peak_dynamic_pressure_pa: f64,
    pub peak_deceleration_m_s2: f64,
    pub periapsis_altitude_m: f64,
    pub integration_steps: usize,
}

fn optimize_periapsis_altitude(
    base_planet: &AeroPlanet,
    vehicle: &AeroVehicle,
    vinf_m_s: f64,
    lower_alt_m: f64,
    upper_alt_m: f64,
) -> Option<solar_aerobrake::AerobrakeResult> {
    if upper_alt_m <= lower_alt_m {
        return None;
    }
    let mut lower = lower_alt_m.max(20_000.0);
    let mut upper = upper_alt_m;
    if upper - lower < 1_000.0 {
        upper = lower + 1_000.0;
    }

    let mut best: Option<solar_aerobrake::AerobrakeResult> = None;
    let mut best_score = f64::INFINITY;

    for _ in 0..3 {
        let samples = 24;
        let step = (upper - lower) / samples as f64;
        for i in 0..=samples {
            let alt = lower + step * i as f64;
            let mut planet = base_planet.clone();
            planet.target_periapsis_altitude_m = alt;
            let request = AeroRequest {
                planet,
                vehicle: vehicle.clone(),
                initial_vinf_m_s: vinf_m_s,
            };
            if let Ok(result) = simulate_ballistic_pass(&request) {
                if result.peak_dynamic_pressure_pa > MAX_AEROBRAKE_DYNAMIC_PRESSURE_PA
                    || result.peak_deceleration_m_s2 > MAX_AEROBRAKE_ACCEL_M_S2
                {
                    continue;
                }
                if result.final_vinf_m_s < best_score {
                    best_score = result.final_vinf_m_s;
                    best = Some(result);
                }
            }
        }

        if let Some(best_result) = &best {
            if best_score <= 50.0 {
                break;
            }
            let best_alt = best_result.periapsis_altitude_m;
            let window = (upper - lower) / 6.0;
            lower = (best_alt - window).max(lower_alt_m);
            upper = (best_alt + window).min(upper_alt_m.max(best_alt + 1_000.0));
            if upper - lower < 1_000.0 {
                break;
            }
        } else {
            break;
        }
    }

    best
}

#[derive(Debug, thiserror::Error)]
pub enum ArrivalError {
    #[error("ephemeris lookup failed: {0}")]
    Ephemeris(#[from] EphemerisError),
    #[error("propulsion constraints not yet implemented")]
    UnsupportedPropulsion,
    #[error("lambert solver failed: {0}")]
    Lambert(String),
}

/// Compute the capture manoeuvre required at the destination body, optionally modelling aerobraking.
///
/// The solver compares the Lambert-arrival hyperbolic excess with the destination parking orbit and
/// estimates propulsive capture delta-v. Aerobraking options scale the required burn to reflect atmospheric
/// assistance; detailed heating limits will be layered on later.
pub fn plan_arrival(
    vehicle: &Vehicle,
    config: &ArrivalConfig,
    destination: &PlanetConfig,
    cruise_config: &InterplanetaryConfig,
    aerobraking: Option<AerobrakingOption>,
    cruise: &InterplanetaryPlan,
) -> Result<ArrivalPlan, ArrivalError> {
    let parking_radius = destination.radius_km + config.target_parking_altitude_km;
    let tof_seconds = if let Some(arrival_epoch) = &cruise_config.arrival_epoch {
        let departure_et = ephemeris::epoch_seconds(&cruise_config.departure_epoch)?;
        let arrival_et = ephemeris::epoch_seconds(arrival_epoch)?;
        (arrival_et - departure_et).abs().max(1.0)
    } else {
        cruise.time_of_flight_days * 86_400.0
    };

    let planet_velocity = cruise.arrival_state.velocity_km_s;
    let mut best_v_infinity: Option<f64> = None;

    let arrival_positions = [
        cruise.arrival_state.position_km,
        [
            cruise.arrival_state.position_km[0] + 1.0,
            cruise.arrival_state.position_km[1] + 1.0,
            cruise.arrival_state.position_km[2],
        ],
    ];

    for arrival_position in &arrival_positions {
        for &short in &[true, false] {
            if let Ok((_, lambert_v2)) = lambert::solve(
                cruise.departure_state.position_km,
                *arrival_position,
                tof_seconds,
                MU_SUN,
                short,
            ) {
                let v_infinity_vec = [
                    lambert_v2[0] - planet_velocity[0],
                    lambert_v2[1] - planet_velocity[1],
                    lambert_v2[2] - planet_velocity[2],
                ];
                let vinf_mag = norm3(&v_infinity_vec);
                if best_v_infinity.map_or(true, |current| vinf_mag < current) {
                    best_v_infinity = Some(vinf_mag);
                }
            }
        }
    }

    let v_infinity = best_v_infinity.unwrap_or(0.0);

    let mut effective_v_infinity = v_infinity;
    let mut aerobrake_report = None;

    if let Some(option) = aerobraking {
        if let (Some(atmosphere), Some(vehicle_aero)) =
            (destination.atmosphere.as_ref(), vehicle.aero.as_ref())
        {
            if atmosphere.exists {
                if let Some(beta) =
                    vehicle_aero.ballistic_coefficient(vehicle.reference_entry_mass_kg())
                {
                    let desired_periapsis_m = match option {
                        AerobrakingOption::Partial {
                            periapsis_altitude_km,
                        }
                        | AerobrakingOption::Full {
                            periapsis_altitude_km,
                        } => periapsis_altitude_km * 1_000.0,
                        AerobrakingOption::Disabled => 0.0,
                    };

                    let entry_target = destination.entry_target.as_ref();
                    let default_periapsis_m = entry_target
                        .map(|t| t.target_periapsis_altitude_m)
                        .unwrap_or(100_000.0);
                    let target_periapsis_m = if desired_periapsis_m > 0.0 {
                        desired_periapsis_m
                    } else {
                        default_periapsis_m
                    };
                    let exit_altitude_m = entry_target
                        .map(|t| t.atm_exit_altitude_m)
                        .unwrap_or(target_periapsis_m + atmosphere.scale_height_km * 1_000.0 * 6.0);

                    let base_planet = AeroPlanet {
                        mu_m3_s2: destination.mu_km3_s2 * 1.0e9,
                        radius_m: destination.radius_km * 1_000.0,
                        surface_density_kg_m3: atmosphere.surface_density_kg_m3,
                        scale_height_m: atmosphere.scale_height_km * 1_000.0,
                        target_periapsis_altitude_m: target_periapsis_m,
                        exit_altitude_m,
                    };

                    let vehicle_ctx = AeroVehicle {
                        ballistic_coefficient_kg_m2: beta,
                        lift_to_drag: vehicle_aero.lift_to_drag,
                    };

                    let vinf_m_s = v_infinity * 1_000.0;

                    let result_opt = match option {
                        AerobrakingOption::Partial { .. } => {
                            let request = AeroRequest {
                                planet: base_planet.clone(),
                                vehicle: vehicle_ctx.clone(),
                                initial_vinf_m_s: vinf_m_s,
                            };
                            simulate_ballistic_pass(&request).ok()
                        }
                        AerobrakingOption::Full { .. } => {
                            let lower = (target_periapsis_m * 0.4).max(20_000.0);
                            let upper = (target_periapsis_m * 1.6)
                                .min(exit_altitude_m.max(target_periapsis_m + 20_000.0));
                            optimize_periapsis_altitude(
                                &base_planet,
                                &vehicle_ctx,
                                vinf_m_s,
                                lower,
                                upper,
                            )
                        }
                        AerobrakingOption::Disabled => None,
                    };

                    if let Some(result) = result_opt {
                        if result.peak_dynamic_pressure_pa > MAX_AEROBRAKE_DYNAMIC_PRESSURE_PA
                            || result.peak_deceleration_m_s2 > MAX_AEROBRAKE_ACCEL_M_S2
                        {
                            // Aerobrake would exceed structural limits; ignore.
                        } else {
                            let delta_v_drag_km_s = result.delta_v_drag_m_s / 1_000.0;
                            let final_vinf_km_s = (result.final_vinf_m_s / 1_000.0).max(0.0);
                            effective_v_infinity = final_vinf_km_s;
                            aerobrake_report = Some(AerobrakeReport {
                                delta_v_drag_km_s,
                                final_vinf_km_s,
                                peak_dynamic_pressure_pa: result.peak_dynamic_pressure_pa,
                                peak_deceleration_m_s2: result.peak_deceleration_m_s2,
                                periapsis_altitude_m: result.periapsis_altitude_m,
                                integration_steps: result.integration_steps,
                            });
                        }
                    }
                }
            }
        }
    }

    let mut capture_delta_v =
        capture_delta_v(destination.mu_km3_s2, parking_radius, effective_v_infinity);
    capture_delta_v = capture_delta_v.max(0.0);

    let burn_duration = match vehicle.propulsion {
        PropulsionMode::Continuous { .. } => None,
        PropulsionMode::Impulsive { .. } => None,
        PropulsionMode::Hybrid => None,
    };

    Ok(ArrivalPlan {
        delta_v_required: capture_delta_v,
        burn_duration_s: burn_duration,
        aerobraking,
        aerobrake_report,
    })
}
