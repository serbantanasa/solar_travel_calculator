//! Core physics and solver logic lives here.
//!
//! The initial implementation will host abstractions for orbital bodies,
//! ephemeris data, and optimal trajectory solvers. Keeping this logic in
//! a library crate lets multiple front-ends (CLI, GUI, web) share it.

pub use solar_aerobrake as aerobrake;
pub use solar_config as config;
pub use solar_core as core;
pub use solar_ephem_spice as ephemeris;
pub use solar_export as export;
pub use solar_importer as importer;
pub use solar_impulsive as impulsive;
pub use solar_lowthrust as lowthrust;
pub use solar_orbits as orbits;
pub use solar_propulsion as propulsion;
pub use solar_transfer as transfer;
pub use transfer::mission;

/// Returns the version of the library for smoke tests while scaffolding.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
