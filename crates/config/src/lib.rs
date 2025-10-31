//! Configuration models and loaders for the Solar Travel Calculator.

use std::fs::File;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

/// Planetary configuration parsed from scenario manifests.
#[derive(Debug, Deserialize, Clone)]
pub struct PlanetConfig {
    pub name: String,
    pub spice_name: String,
    #[serde(default)]
    pub parent_spice: Option<String>,
    pub mu_km3_s2: f64,
    pub radius_km: f64,
    pub soi_radius_km: f64,
    pub default_parking_altitude_km: f64,
    pub surface_gravity_m_s2: f64,
    pub mass_kg: f64,
    pub atmosphere: Option<AtmosphereConfig>,
    #[serde(default)]
    pub kernel_dependencies: Vec<String>,
}

/// Atmospheric metadata for capture/aerobraking heuristics.
#[derive(Debug, Deserialize, Clone)]
pub struct AtmosphereConfig {
    pub exists: bool,
    pub scale_height_km: f64,
    pub surface_density_kg_m3: f64,
}

/// Vehicle configuration parsed from scenario catalogs.
#[derive(Debug, Deserialize, Clone)]
pub struct VehicleConfig {
    pub name: String,
    pub dry_mass_kg: f64,
    pub propellant_mass_kg: f64,
    pub propulsion: VehiclePropulsionConfig,
}

/// Propulsion configuration in scenario manifests.
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
    Impulsive {
        max_delta_v_km_s: f64,
        isp_seconds: f64,
        #[serde(default)]
        max_thrust_newtons: Option<f64>,
    },
    #[serde(other)]
    Unsupported,
}

/// Errors that can occur while loading configuration files.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read YAML: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse YAML: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Load planet configurations from a YAML file.
pub fn load_planets<P: AsRef<Path>>(path: P) -> Result<Vec<PlanetConfig>, ConfigError> {
    let mut planets: Vec<PlanetConfig> = load_records(path)?;
    for planet in &mut planets {
        if planet.kernel_dependencies.is_empty() {
            planet.kernel_dependencies = infer_kernel_dependencies(&planet.spice_name);
        }
    }
    Ok(planets)
}

/// Load vehicle configurations from a YAML file.
pub fn load_vehicle_configs<P: AsRef<Path>>(path: P) -> Result<Vec<VehicleConfig>, ConfigError> {
    load_records(path)
}

fn load_records<T, P>(path: P) -> Result<Vec<T>, ConfigError>
where
    T: for<'de> Deserialize<'de>,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if path.is_dir() {
        read_dir_records(path)
    } else if path.extension().map(|ext| ext == "toml").unwrap_or(false) {
        let contents = std::fs::read_to_string(path)?;
        let record: T = toml::from_str(&contents)?;
        Ok(vec![record])
    } else {
        let reader = File::open(path)?;
        Ok(serde_yaml::from_reader(reader)?)
    }
}

fn read_dir_records<T>(dir: &Path) -> Result<Vec<T>, ConfigError>
where
    T: for<'de> Deserialize<'de>,
{
    let mut records = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().map(|ext| ext == "toml").unwrap_or(false))
        .collect();
    entries.sort();
    for path in entries {
        let contents = std::fs::read_to_string(&path)?;
        let record: T = toml::from_str(&contents)?;
        records.push(record);
    }
    Ok(records)
}

fn infer_kernel_dependencies(spice_name: &str) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut deps: BTreeSet<&'static str> = ["de440s.bsp", "naif0012.tls", "pck00011.tpc"].into();
    let upper = spice_name.to_ascii_uppercase();

    let mut add = |kernel: &'static str| {
        deps.insert(kernel);
    };

    if matches!(
        upper.as_str(),
        "EARTH"
            | "MOON"
            | "MARS"
            | "VENUS"
            | "MERCURY"
            | "JUPITER"
            | "SATURN"
            | "URANUS"
            | "NEPTUNE"
            | "PLUTO"
            | "EARTH BARYCENTER"
            | "MARS BARYCENTER"
            | "JUPITER BARYCENTER"
            | "SATURN BARYCENTER"
            | "URANUS BARYCENTER"
            | "NEPTUNE BARYCENTER"
            | "PLUTO BARYCENTER"
    ) {
        // base bodies already covered by defaults
    }

    if upper.contains("JUPITER")
        || matches!(upper.as_str(), "IO" | "EUROPA" | "GANYMEDE" | "CALLISTO")
    {
        add("jup365.bsp");
    }

    if upper.contains("SATURN") || matches!(upper.as_str(), "TITAN" | "ENCELADUS") {
        add("sat455.bsp");
    }

    if upper.contains("MARS") || matches!(upper.as_str(), "PHOBOS" | "DEIMOS") {
        add("mar099.bsp");
    }

    if upper.contains("NEPTUNE") || upper.contains("TRITON") {
        add("nep095.bsp");
    }

    if upper.contains("PLUTO") || upper.contains("CHARON") {
        add("plu060.bsp");
    }

    if matches!(upper.as_str(), "CERES" | "VESTA" | "PALLAS") {
        add("codes_300ast_20100725.bsp");
        add("codes_300ast_20100725.tf");
    }

    if matches!(upper.as_str(), "ERIS" | "HAUMEA" | "MAKEMAKE") {
        add("tnosat_v001_20000617_jpl082_20230601.bsp");
    }

    deps.into_iter().map(|s| s.to_string()).collect()
}
