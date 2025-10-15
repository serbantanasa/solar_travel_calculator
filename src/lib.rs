//! Core physics and solver logic lives here.
//!
//! The initial implementation will host abstractions for orbital bodies,
//! ephemeris data, and optimal trajectory solvers. Keeping this logic in
//! a library crate lets multiple front-ends (CLI, GUI, web) share it.

pub mod dynamics;
pub mod ephemeris;
pub mod mission;
pub mod scenario;

/// Returns the version of the library for smoke tests while scaffolding.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
