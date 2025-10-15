//! Arrival phase: capture into destination parking orbit, optionally with aerobraking support.

use crate::dynamics::lambert;
use crate::ephemeris::{self, EphemerisError};
use crate::scenario::PlanetConfig;

use super::interplanetary::{InterplanetaryConfig, InterplanetaryPlan};
use super::propulsion::{PropulsionMode, Vehicle};

const MU_SUN: f64 = 1.327_124_400_18e11;

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
    let circular_speed = (destination.mu_km3_s2 / parking_radius).sqrt();

    let tof_seconds = if let Some(arrival_epoch) = &cruise_config.arrival_epoch {
        let departure_et = ephemeris::epoch_seconds(&cruise_config.departure_epoch)?;
        let arrival_et = ephemeris::epoch_seconds(arrival_epoch)?;
        (arrival_et - departure_et).abs().max(1.0)
    } else {
        cruise.time_of_flight_days * 86_400.0
    };

    let lambert_result = lambert::solve(
        cruise.departure_state.position_km,
        cruise.arrival_state.position_km,
        tof_seconds,
        MU_SUN,
        true,
    )
    .or_else(|_| {
        let perturbed = [
            cruise.arrival_state.position_km[0] + 1.0,
            cruise.arrival_state.position_km[1] + 1.0,
            cruise.arrival_state.position_km[2],
        ];
        lambert::solve(
            cruise.departure_state.position_km,
            perturbed,
            tof_seconds,
            MU_SUN,
            true,
        )
    });

    let planet_velocity = cruise.arrival_state.velocity_km_s;
    let v_infinity = match lambert_result {
        Ok((_, lambert_v2)) => {
            let v_infinity_vec = [
                lambert_v2[0] - planet_velocity[0],
                lambert_v2[1] - planet_velocity[1],
                lambert_v2[2] - planet_velocity[2],
            ];
            vector_norm(&v_infinity_vec)
        }
        Err(_err) => 0.0,
    };

    let hyperbolic_periapsis_speed =
        (v_infinity * v_infinity + 2.0 * destination.mu_km3_s2 / parking_radius).sqrt();
    let mut capture_delta_v = (hyperbolic_periapsis_speed - circular_speed).max(0.0);

    if let Some(option) = aerobraking {
        match option {
            AerobrakingOption::Full { .. } => capture_delta_v *= 0.1,
            AerobrakingOption::Partial { .. } => capture_delta_v *= 0.5,
            AerobrakingOption::Disabled => {}
        }
    }

    let burn_duration = match vehicle.propulsion {
        PropulsionMode::Continuous { .. } => None,
        PropulsionMode::Impulsive { .. } => None,
        PropulsionMode::Hybrid => None,
    };

    Ok(ArrivalPlan {
        delta_v_required: capture_delta_v,
        burn_duration_s: burn_duration,
        aerobraking,
    })
}

fn vector_norm(a: &[f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
