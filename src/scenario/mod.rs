use std::fs::File;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::mission::propulsion::{PropulsionMode, Vehicle};

#[derive(Debug, Deserialize, Clone)]
pub struct PlanetConfig {
    pub name: String,
    pub spice_name: String,
    pub mu_km3_s2: f64,
    pub radius_km: f64,
    pub soi_radius_km: f64,
    pub default_parking_altitude_km: f64,
    pub atmosphere: Option<AtmosphereConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AtmosphereConfig {
    pub exists: bool,
    pub scale_height_km: f64,
    pub surface_density_kg_m3: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VehicleConfig {
    pub name: String,
    pub dry_mass_kg: f64,
    pub propellant_mass_kg: f64,
    pub propulsion: VehiclePropulsionConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum VehiclePropulsionConfig {
    #[serde(rename = "continuous")]
    Continuous {
        max_thrust_newtons: f64,
        isp_seconds: f64,
        #[serde(default)]
        max_acceleration_m_s2: Option<f64>,
    },
    #[serde(rename = "impulsive")]
    Impulsive { max_delta_v_km_s: f64 },
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("failed to read YAML: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse YAML: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("vehicle propulsion type unsupported")]
    UnsupportedPropulsion,
}

pub fn load_planets<P: AsRef<Path>>(path: P) -> Result<Vec<PlanetConfig>, ScenarioError> {
    let reader = File::open(path)?;
    Ok(serde_yaml::from_reader(reader)?)
}

pub fn load_vehicles<P: AsRef<Path>>(path: P) -> Result<Vec<Vehicle>, ScenarioError> {
    let reader = File::open(path)?;
    let configs: Vec<VehicleConfig> = serde_yaml::from_reader(reader)?;
    configs.into_iter().map(|cfg| cfg.try_into()).collect()
}

impl TryFrom<VehicleConfig> for Vehicle {
    type Error = ScenarioError;

    fn try_from(value: VehicleConfig) -> Result<Self, Self::Error> {
        let propulsion = match value.propulsion {
            VehiclePropulsionConfig::Continuous {
                max_thrust_newtons,
                isp_seconds,
                max_acceleration_m_s2,
            } => PropulsionMode::Continuous {
                max_thrust_newtons,
                isp_seconds,
                max_acceleration_m_s2,
            },
            VehiclePropulsionConfig::Impulsive { max_delta_v_km_s } => {
                PropulsionMode::Impulsive { max_delta_v_km_s }
            }
            VehiclePropulsionConfig::Unsupported => {
                return Err(ScenarioError::UnsupportedPropulsion);
            }
        };

        Ok(Vehicle {
            name: value.name,
            dry_mass_kg: value.dry_mass_kg,
            propellant_mass_kg: value.propellant_mass_kg,
            propulsion,
        })
    }
}
