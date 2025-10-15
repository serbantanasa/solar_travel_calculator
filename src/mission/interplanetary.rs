//! Interplanetary cruise phase: integrates the heliocentric transfer leg using the selected propulsion model.

use crate::ephemeris::{self, StateVector};
use crate::scenario::PlanetConfig;

use super::propulsion::{PropulsionMode, Vehicle};

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
    let (departure_state, arrival_state) = boundary_states(config)?;

    match &config.propulsion_mode {
        PropulsionMode::Continuous { .. } => continuous::solve(
            vehicle,
            config,
            origin,
            destination,
            departure_state,
            arrival_state,
        ),
        _ => Ok(InterplanetaryPlan {
            time_of_flight_days: 150.0,
            propellant_used_kg: None,
            departure_state,
            arrival_state,
        }),
    }
}

fn boundary_states(
    config: &InterplanetaryConfig,
) -> Result<(StateVector, StateVector), InterplanetaryError> {
    let departure_state = ephemeris::state_vector(
        &config.departure_body,
        "SUN",
        "ECLIPJ2000",
        "NONE",
        &config.departure_epoch,
    )?;

    let arrival_epoch = config
        .arrival_epoch
        .clone()
        .unwrap_or_else(|| config.departure_epoch.clone());
    let arrival_state = ephemeris::state_vector(
        &config.destination_body,
        "SUN",
        "ECLIPJ2000",
        "NONE",
        &arrival_epoch,
    )?;

    Ok((departure_state, arrival_state))
}
