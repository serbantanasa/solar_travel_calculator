//! Continuous-thrust analytical utilities.

use serde::Serialize;
use solar_core::constants::G0;

const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Inputs describing a constant-acceleration, symmetric accelerate/decelerate profile.
#[derive(Debug, Clone)]
pub struct ConstantAccelInputs {
    pub acceleration_m_s2: f64,
    pub isp_seconds: f64,
    pub initial_mass_kg: f64,
    pub dry_mass_kg: f64,
}

/// Per-sample telemetry record for the continuous profile.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileSample {
    pub time_s: f64,
    pub distance_m: f64,
    pub velocity_m_s: f64,
    pub mass_kg: f64,
}

/// Summary metrics for the computed profile.
#[derive(Debug, Clone)]
pub struct ContinuousTransferSummary {
    pub burn_time_total_s: f64,
    pub propellant_used_kg: f64,
    pub final_mass_kg: f64,
    pub dv_each_km_s: f64,
    pub dv_total_km_s: f64,
    pub max_velocity_m_s: f64,
    pub max_velocity_fraction_c: f64,
    pub total_distance_m: f64,
    pub kinetic_energy_joules: f64,
    pub time_of_flight_s: f64,
    pub samples: Vec<ProfileSample>,
}

/// Generates the symmetric accelerate/decelerate profile for a given total time.
/// Returns `None` if the requested acceleration/time exceeds the vehicle's propellant supply.
pub fn constant_accel_profile(
    inputs: &ConstantAccelInputs,
    total_time_s: f64,
) -> Option<ContinuousTransferSummary> {
    if total_time_s <= 0.0 {
        return None;
    }
    let a = inputs.acceleration_m_s2;
    if a <= 0.0 {
        return None;
    }
    let t_half = total_time_s * 0.5;
    let isp = inputs.isp_seconds;
    if isp <= 0.0 {
        return None;
    }

    // mass evolution with throttle adjusted to maintain constant acceleration a
    let exponent = -a * total_time_s / (isp * G0);
    let final_mass = inputs.initial_mass_kg * exponent.exp();
    if final_mass < inputs.dry_mass_kg - 1e-6 {
        return None;
    }
    let propellant_used = inputs.initial_mass_kg - final_mass;

    let burn_time_total = total_time_s;
    let max_velocity = a * t_half;
    let total_distance = a * t_half * t_half;
    let peak_mass = inputs.initial_mass_kg * (-a * t_half / (isp * G0)).exp();
    let kinetic_energy = 0.5 * peak_mass * max_velocity * max_velocity;

    let mut samples = build_samples(inputs, total_time_s);
    if let Some(last) = samples.last_mut() {
        last.time_s = total_time_s;
        last.distance_m = total_distance;
        last.velocity_m_s = 0.0;
        last.mass_kg = final_mass;
    }

    Some(ContinuousTransferSummary {
        burn_time_total_s: burn_time_total,
        propellant_used_kg: propellant_used,
        final_mass_kg: final_mass,
        dv_each_km_s: max_velocity / 1_000.0,
        dv_total_km_s: 2.0 * max_velocity / 1_000.0,
        max_velocity_m_s: max_velocity,
        max_velocity_fraction_c: max_velocity / SPEED_OF_LIGHT_M_S,
        total_distance_m: total_distance,
        kinetic_energy_joules: kinetic_energy,
        time_of_flight_s: total_time_s,
        samples,
    })
}

fn build_samples(inputs: &ConstantAccelInputs, total_time_s: f64) -> Vec<ProfileSample> {
    let a = inputs.acceleration_m_s2;
    let t_half = total_time_s * 0.5;
    let mut samples = Vec::new();
    samples.push(ProfileSample {
        time_s: 0.0,
        distance_m: 0.0,
        velocity_m_s: 0.0,
        mass_kg: inputs.initial_mass_kg,
    });

    let mut time = 0.0;
    let mut distance = 0.0;
    let mut velocity = 0.0;
    let mut mass = inputs.initial_mass_kg;

    let dt_base = if total_time_s > 18_000.0 {
        3_600.0
    } else {
        (total_time_s / 200.0).clamp(30.0, 3_600.0)
    };

    let isp = inputs.isp_seconds;
    let mass_factor = -a / (isp * G0);

    while time + 1e-9 < t_half {
        let dt = (t_half - time).min(dt_base);
        time += dt;
        velocity += a * dt;
        distance += 0.5 * a * (time * time - (time - dt) * (time - dt));
        mass = inputs.initial_mass_kg * (mass_factor * time).exp();
        samples.push(ProfileSample {
            time_s: time,
            distance_m: distance,
            velocity_m_s: velocity,
            mass_kg: mass,
        });
    }

    let peak_mass = mass;
    let peak_velocity = velocity;
    let peak_distance = distance;

    let mut dec_time = 0.0;
    while dec_time + 1e-9 < t_half {
        let dt = (t_half - dec_time).min(dt_base);
        dec_time += dt;
        time += dt;
        velocity = (peak_velocity - a * dec_time).max(0.0);
        let tau = dec_time;
        distance = peak_distance + peak_velocity * tau - 0.5 * a * tau * tau;
        mass = peak_mass * (mass_factor * dec_time).exp();
        samples.push(ProfileSample {
            time_s: time,
            distance_m: distance,
            velocity_m_s: velocity,
            mass_kg: mass,
        });
    }

    samples
}
