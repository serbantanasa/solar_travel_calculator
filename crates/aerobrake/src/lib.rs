//! Simple aerobraking pass integrator using exponential atmospheres.

use thiserror::Error;

/// Properties describing the target planet and its atmosphere.
#[derive(Debug, Clone)]
pub struct PlanetEntryContext {
    pub mu_m3_s2: f64,
    pub radius_m: f64,
    pub surface_density_kg_m3: f64,
    pub scale_height_m: f64,
    pub target_periapsis_altitude_m: f64,
    pub exit_altitude_m: f64,
}

/// Aerodynamic characteristics of the vehicle during entry.
#[derive(Debug, Clone)]
pub struct VehicleEntryContext {
    pub ballistic_coefficient_kg_m2: f64,
    pub lift_to_drag: Option<f64>,
}

/// Request to simulate a single aerobraking pass.
#[derive(Debug, Clone)]
pub struct AerobrakeRequest {
    pub planet: PlanetEntryContext,
    pub vehicle: VehicleEntryContext,
    pub initial_vinf_m_s: f64,
}

/// Output from the aerobraking integrator.
#[derive(Debug, Clone)]
pub struct AerobrakeResult {
    pub delta_v_drag_m_s: f64,
    pub final_vinf_m_s: f64,
    pub peak_dynamic_pressure_pa: f64,
    pub peak_deceleration_m_s2: f64,
    pub periapsis_altitude_m: f64,
    pub integration_steps: usize,
}

#[derive(Debug, Error)]
pub enum AerobrakeError {
    #[error("atmosphere scale height must be positive")]
    InvalidScaleHeight,
    #[error("ballistic coefficient must be positive")]
    InvalidBallisticCoefficient,
    #[error("hyperbolic periapsis lies below the planetary surface")]
    PeriapsisBelowSurface,
    #[error("insufficient atmospheric data for aerobraking")]
    MissingAtmosphere,
}

/// Simulate a ballistic aerobraking pass and estimate drag impulse.
///
/// The integrator assumes a two-body hyperbolic fly-by and integrates
/// drag along the unperturbed trajectory using an exponential atmosphere.
pub fn simulate_ballistic_pass(
    request: &AerobrakeRequest,
) -> Result<AerobrakeResult, AerobrakeError> {
    if request.planet.scale_height_m <= 0.0 {
        return Err(AerobrakeError::InvalidScaleHeight);
    }
    if request.vehicle.ballistic_coefficient_kg_m2 <= 0.0 {
        return Err(AerobrakeError::InvalidBallisticCoefficient);
    }

    let mu = request.planet.mu_m3_s2;
    let radius = request.planet.radius_m;
    let h_target = request.planet.target_periapsis_altitude_m.max(0.0);
    let h_exit = request.planet.exit_altitude_m.max(h_target);
    let rp = radius + h_target;
    if rp <= radius {
        return Err(AerobrakeError::PeriapsisBelowSurface);
    }

    let v_inf = request.initial_vinf_m_s.max(0.0);
    if v_inf == 0.0 {
        return Ok(AerobrakeResult {
            delta_v_drag_m_s: 0.0,
            final_vinf_m_s: 0.0,
            peak_dynamic_pressure_pa: 0.0,
            peak_deceleration_m_s2: 0.0,
            periapsis_altitude_m: h_target,
            integration_steps: 0,
        });
    }

    let scale_height = request.planet.scale_height_m;
    let rho0 = request.planet.surface_density_kg_m3;
    if rho0 <= 0.0 {
        return Err(AerobrakeError::MissingAtmosphere);
    }

    // Hyperbolic orbit parameters.
    let a = -mu / (v_inf * v_inf);
    let e = 1.0 + (rp * v_inf * v_inf) / mu;
    let h_ang = (mu * (-a) * (e * e - 1.0)).sqrt();
    let p = h_ang * h_ang / mu;

    let r_exit = radius + h_exit;
    let cos_f_max = ((p / r_exit) - 1.0) / e;
    let f_max = cos_f_max.clamp(-1.0, 1.0).acos();

    let steps = 800; // dense sampling for smooth integrals
    let df = 2.0 * f_max / steps as f64;
    let mut delta_v_drag = 0.0_f64;
    let mut peak_q = 0.0_f64;
    let mut peak_accel = 0.0_f64;

    let mut beta = request.vehicle.ballistic_coefficient_kg_m2;
    if let Some(ld) = request.vehicle.lift_to_drag {
        let factor = (1.0 + ld * ld).sqrt();
        if factor > 0.0 {
            beta /= factor;
        }
    }
    if beta <= 0.0 {
        return Err(AerobrakeError::InvalidBallisticCoefficient);
    }

    for i in 0..=steps {
        let f = -f_max + df * i as f64;
        let cos_f = f.cos();
        let r = p / (1.0 + e * cos_f);
        if r < radius {
            continue;
        }
        let h = r - radius;
        let rho = rho0 * f64::exp(-h / scale_height);
        if rho < 1.0e-12 {
            continue;
        }

        let v = (mu * (2.0 / r - 1.0 / a)).sqrt();
        let a_drag = 0.5 * rho * v * v / beta;
        let dynamic_pressure = 0.5 * rho * v * v;
        peak_q = peak_q.max(dynamic_pressure);
        peak_accel = peak_accel.max(a_drag);

        let dt = (r * r / h_ang) * df;
        delta_v_drag += a_drag * dt;
    }

    let final_v_inf = (v_inf - delta_v_drag).max(0.0);

    Ok(AerobrakeResult {
        delta_v_drag_m_s: delta_v_drag,
        final_vinf_m_s: final_v_inf,
        peak_dynamic_pressure_pa: peak_q,
        peak_deceleration_m_s2: peak_accel,
        periapsis_altitude_m: h_target,
        integration_steps: steps + 1,
    })
}
