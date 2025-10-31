//! Mission planning orchestrator that sequences departure, interplanetary, and arrival phases.

pub mod arrival;
pub mod departure;
pub mod interplanetary;
pub mod porkchop;

use self::arrival::{ArrivalConfig, ArrivalPlan};
use self::departure::{DepartureConfig, DeparturePlan};
use self::interplanetary::{InterplanetaryConfig, InterplanetaryPlan};
use solar_config::PlanetConfig;
use solar_propulsion::Vehicle;

/// Aggregated mission profile describing the three sequential legs.
#[derive(Debug)]
pub struct MissionProfile {
    pub departure: DeparturePlan,
    pub cruise: InterplanetaryPlan,
    pub arrival: ArrivalPlan,
}

/// Top-level mission planning error.
#[derive(Debug, thiserror::Error)]
pub enum MissionError {
    #[error("departure planning failed: {0}")]
    Departure(#[from] departure::DepartureError),
    #[error("interplanetary planning failed: {0}")]
    Cruise(#[from] interplanetary::InterplanetaryError),
    #[error("arrival planning failed: {0}")]
    Arrival(#[from] arrival::ArrivalError),
}

/// Inputs necessary to compute an end-to-end transfer between parking orbits.
#[derive(Debug)]
pub struct MissionConfig {
    pub vehicle: Vehicle,
    pub origin: PlanetConfig,
    pub destination: PlanetConfig,
    pub departure: DepartureConfig,
    pub cruise: InterplanetaryConfig,
    pub arrival: ArrivalConfig,
}

/// Run the three-phase mission planner, chaining departure, interplanetary, and arrival calculations.
pub fn plan_mission(config: MissionConfig) -> Result<MissionProfile, MissionError> {
    let cruise = interplanetary::plan_interplanetary(
        &config.vehicle,
        &config.cruise,
        &config.origin,
        &config.destination,
    )?;
    let departure = departure::plan_departure(
        &config.vehicle,
        &config.departure,
        &config.origin,
        &config.cruise,
        &cruise,
    )?;
    let arrival = arrival::plan_arrival(
        &config.vehicle,
        &config.arrival,
        &config.destination,
        &config.cruise,
        config.arrival.aerobraking,
        &cruise,
    )?;

    Ok(MissionProfile {
        departure,
        cruise,
        arrival,
    })
}
