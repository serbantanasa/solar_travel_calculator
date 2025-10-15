use lambert_bate::get_velocities;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LambertSolverError {
    #[error("lambert solver failed: {0}")]
    Failure(String),
}

pub fn solve(
    r1_km: [f64; 3],
    r2_km: [f64; 3],
    time_of_flight_s: f64,
    mu_km3_s2: f64,
    short: bool,
) -> Result<([f64; 3], [f64; 3]), LambertSolverError> {
    get_velocities(r1_km, r2_km, time_of_flight_s, mu_km3_s2, short, 1e-8, 500)
        .map_err(|e| LambertSolverError::Failure(format!("{e:?}")))
}
