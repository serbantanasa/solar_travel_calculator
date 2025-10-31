use solar_core::constants::G0;
use solar_ephem_spice::StateVector;
use solar_orbits::{add, dot, norm3, scale, sub};
use solar_propulsion::{PropulsionMode, Vehicle};

use super::{InterplanetaryConfig, InterplanetaryError, InterplanetaryPlan};

const MU_SUN: f64 = 1.327_124_400_18e11; // km^3 / s^2

pub(super) fn solve(
    vehicle: &Vehicle,
    config: &InterplanetaryConfig,
    _origin: &solar_config::PlanetConfig,
    _destination: &solar_config::PlanetConfig,
    departure_state: StateVector,
    arrival_state: StateVector,
) -> Result<InterplanetaryPlan, InterplanetaryError> {
    let (max_thrust, isp, max_accel) = match (&vehicle.propulsion, &config.propulsion_mode) {
        (
            PropulsionMode::Continuous {
                max_thrust_newtons,
                isp_seconds,
                max_acceleration_m_s2,
            },
            PropulsionMode::Continuous { .. },
        ) => (*max_thrust_newtons, *isp_seconds, *max_acceleration_m_s2),
        _ => return Err(InterplanetaryError::UnsupportedPropulsion),
    };

    if isp <= 0.0 {
        return Err(InterplanetaryError::InvalidSpecificImpulse);
    }
    if max_thrust <= 0.0 {
        return Err(InterplanetaryError::InvalidAcceleration);
    }

    let displacement = sub(&arrival_state.position_km, &departure_state.position_km);
    let distance = norm3(&displacement);
    if distance == 0.0 {
        return Ok(InterplanetaryPlan {
            time_of_flight_days: 0.0,
            propellant_used_kg: Some(0.0),
            departure_state,
            arrival_state,
            peak_speed_km_s: Some(0.0),
        });
    }

    let direction = scale(&displacement, 1.0 / distance);

    let initial_mass = vehicle.initial_mass_kg();
    let mut mass = initial_mass;
    let dry_mass = vehicle.dry_mass_kg;
    let m_dot = max_thrust / (isp * G0);

    let mut accel_limit = max_accel.unwrap_or(0.0);
    if accel_limit <= 0.0 {
        accel_limit = max_thrust / mass;
    }
    if accel_limit <= 0.0 {
        return Err(InterplanetaryError::InvalidAcceleration);
    }

    let accel_km_s2 = accel_limit / 1_000.0;
    let half_time = f64::sqrt(distance / accel_km_s2);
    let total_time = 2.0 * half_time;
    let steps = 10_000.max((total_time / 10.0).ceil() as usize);
    let dt = total_time / steps as f64;

    let mut x = 0.0;
    let mut v = dot(&departure_state.velocity_km_s, &direction);
    let mut peak_speed = v.abs();
    let mut time = 0.0;

    for step in 0..steps {
        let thrust_dir = if x < distance / 2.0 { 1.0 } else { -1.0 };

        let thrust_accel_mag = (max_thrust / mass) / 1_000.0;
        let limited_accel = thrust_accel_mag.min(accel_limit / 1_000.0);
        let a_thrust = thrust_dir * limited_accel;

        let position_vec = add(&departure_state.position_km, &scale(&direction, x));
        let r_mag = norm3(&position_vec).max(1.0);
        let grav_vec = scale(&position_vec, -MU_SUN / (r_mag.powi(3)));
        let a_grav = dot(&grav_vec, &direction);

        let total_accel = a_thrust + a_grav;

        v += total_accel * dt;
        peak_speed = peak_speed.max(v.abs());
        x += v * dt;

        if x < 0.0 {
            x = 0.0;
            v = 0.0;
        }
        if x > distance {
            x = distance;
        }

        time += dt;

        if mass > dry_mass {
            mass -= m_dot * dt;
            if mass < dry_mass {
                mass = dry_mass;
            }
        }

        if step == steps - 1 {
            x = distance;
        }
    }

    let propellant_used = (initial_mass - mass)
        .min(vehicle.propellant_mass_kg)
        .max(0.0);

    Ok(InterplanetaryPlan {
        time_of_flight_days: time / 86_400.0,
        propellant_used_kg: Some(propellant_used),
        departure_state,
        arrival_state,
        peak_speed_km_s: Some(peak_speed.abs()),
    })
}
