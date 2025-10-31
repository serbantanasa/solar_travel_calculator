//! Re-exported APIs for consumers of the transfer crate.

pub use crate::mission::arrival::{AerobrakingOption, ArrivalConfig, ArrivalError, ArrivalPlan};
pub use crate::mission::departure::{DepartureConfig, DepartureError, DeparturePlan};
pub use crate::mission::interplanetary::{
    InterplanetaryConfig, InterplanetaryError, InterplanetaryPlan,
};
pub use crate::mission::{MissionConfig, MissionError, MissionProfile, plan_mission};
pub use solar_propulsion::{PropulsionMode, Vehicle};

pub mod vehicle {
    use solar_config::{VehicleConfig, VehiclePropulsionConfig};
    use solar_propulsion::{PropulsionMode, Vehicle};
    use thiserror::Error;

    /// Errors surfaced when selecting or converting vehicles.
    #[derive(Debug, Error)]
    pub enum VehicleError {
        #[error("vehicle '{0}' not found in catalog")]
        NotFound(String),
        #[error("vehicle catalog is empty")]
        EmptyCatalog,
        #[error("propulsion configuration is not supported yet")]
        UnsupportedPropulsion,
    }

    /// Convert a `VehicleConfig` into runtime `Vehicle` representation.
    pub fn from_config(config: &VehicleConfig) -> Result<Vehicle, VehicleError> {
        let propulsion = match &config.propulsion {
            VehiclePropulsionConfig::Continuous {
                max_thrust_newtons,
                isp_seconds,
                max_acceleration_m_s2,
            } => PropulsionMode::Continuous {
                max_thrust_newtons: *max_thrust_newtons,
                isp_seconds: *isp_seconds,
                max_acceleration_m_s2: *max_acceleration_m_s2,
            },
            VehiclePropulsionConfig::Impulsive {
                max_delta_v_km_s,
                isp_seconds,
                max_thrust_newtons,
            } => PropulsionMode::Impulsive {
                max_delta_v_km_s: *max_delta_v_km_s,
                isp_seconds: *isp_seconds,
                max_thrust_newtons: *max_thrust_newtons,
            },
            VehiclePropulsionConfig::Unsupported => {
                return Err(VehicleError::UnsupportedPropulsion);
            }
        };

        Ok(Vehicle {
            name: config.name.clone(),
            dry_mass_kg: config.dry_mass_kg,
            propellant_mass_kg: config.propellant_mass_kg,
            propulsion,
        })
    }

    /// Select a vehicle from the catalog by optional name, defaulting to continuous propulsion entries.
    pub fn select<'a>(
        configs: &'a [VehicleConfig],
        requested: Option<&str>,
    ) -> Result<Vehicle, VehicleError> {
        if configs.is_empty() {
            return Err(VehicleError::EmptyCatalog);
        }

        let chosen = if let Some(name) = requested {
            let upper = name.to_uppercase();
            configs
                .iter()
                .find(|cfg| cfg.name.to_uppercase() == upper)
                .ok_or_else(|| VehicleError::NotFound(name.to_string()))?
        } else {
            configs
                .iter()
                .find(|cfg| matches!(cfg.propulsion, VehiclePropulsionConfig::Continuous { .. }))
                .unwrap_or(&configs[0])
        };

        from_config(chosen)
    }
}
