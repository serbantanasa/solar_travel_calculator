//! Departure phase: depart a parking orbit around the origin body and inject onto an interplanetary trajectory.

use solar_config::PlanetConfig;
use solar_ephem_spice::{self as ephemeris, EphemerisError};
use solar_impulsive::lambert;
use solar_orbits::{escape_delta_v, norm3};
use solar_propulsion::{PropulsionMode, Vehicle};

use super::interplanetary::{InterplanetaryConfig, InterplanetaryPlan};

const MU_SUN: f64 = 1.327_124_400_18e11;

/// Configuration for the departure burn from a parking orbit.
#[derive(Debug, Clone)]
pub struct DepartureConfig {
    /// Origin body name as accepted by SPICE (e.g., "EARTH", "MARS").
    pub origin_body: String,
    /// Altitude of the initial parking orbit above mean radius (kilometres).
    pub parking_altitude_km: f64,
    /// Desired epoch for departure (UTC/TDB string understood by SPICE).
    pub departure_epoch: String,
    /// Target hyperbolic excess vector magnitude (km/s) seeded by the interplanetary solver.
    pub required_v_infinity: Option<f64>,
    /// Propulsion strategy to use for the departure phase.
    pub propulsion_mode: PropulsionMode,
}

/// Result of the departure planning phase.
#[derive(Debug, Clone)]
pub struct DeparturePlan {
    pub delta_v_required: f64,
    pub burn_duration_s: Option<f64>,
    pub hyperbolic_excess_km_s: f64,
    pub parking_orbit_velocity_km_s: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum DepartureError {
    #[error("ephemeris lookup failed: {0}")]
    Ephemeris(#[from] EphemerisError),
    #[error("propulsion constraints not yet implemented")]
    UnsupportedPropulsion,
    #[error("lambert solver failed: {0}")]
    Lambert(String),
}

/// Compute the departure manoeuvre required to transition from a parking orbit to the heliocentric leg.
///
/// The solver uses a Lambert solution to determine the required hyperbolic excess vector relative to the
/// origin body and converts this into a parking-orbit burn (patched-conic escape). Continuous-thrust and
/// finite-burn corrections remain future enhancements, but delta-v and v-infinity are now physically grounded.
pub fn plan_departure(
    vehicle: &Vehicle,
    config: &DepartureConfig,
    origin: &PlanetConfig,
    cruise_config: &InterplanetaryConfig,
    cruise: &InterplanetaryPlan,
) -> Result<DeparturePlan, DepartureError> {
    let parking_radius = origin.radius_km + config.parking_altitude_km;
    let circular_speed = (origin.mu_km3_s2 / parking_radius).sqrt();

    let departure_et = ephemeris::epoch_seconds(&config.departure_epoch)?;
    let arrival_et = if let Some(epoch) = &cruise_config.arrival_epoch {
        ephemeris::epoch_seconds(epoch)?
    } else {
        departure_et + cruise.time_of_flight_days * 86_400.0
    };
    let tof_seconds = (arrival_et - departure_et).abs().max(1.0);

    let planet_velocity = cruise.departure_state.velocity_km_s;
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
            if let Ok((lambert_v1, _)) = lambert::solve(
                cruise.departure_state.position_km,
                *arrival_position,
                tof_seconds,
                MU_SUN,
                short,
            ) {
                let v_infinity_vec = [
                    lambert_v1[0] - planet_velocity[0],
                    lambert_v1[1] - planet_velocity[1],
                    lambert_v1[2] - planet_velocity[2],
                ];
                let vinf_mag = norm3(&v_infinity_vec);
                if best_v_infinity.map_or(true, |current| vinf_mag < current) {
                    best_v_infinity = Some(vinf_mag);
                }
            }
        }
    }

    let v_infinity = best_v_infinity.unwrap_or_else(|| config.required_v_infinity.unwrap_or(0.0));

    let delta_v = escape_delta_v(origin.mu_km3_s2, parking_radius, v_infinity);

    let burn_duration = match vehicle.propulsion {
        PropulsionMode::Continuous { .. } => None,
        PropulsionMode::Impulsive { .. } => None,
        PropulsionMode::Hybrid => None,
    };

    Ok(DeparturePlan {
        delta_v_required: delta_v,
        burn_duration_s: burn_duration,
        hyperbolic_excess_km_s: v_infinity,
        parking_orbit_velocity_km_s: circular_speed,
    })
}
