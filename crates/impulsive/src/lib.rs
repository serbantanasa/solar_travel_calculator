//! Impulsive transfer utilities: Lambert solver and classical transfer approximations.

pub mod lambert;
pub mod transfers;

pub use lambert::{LambertSolverError, solve as lambert_solve};
