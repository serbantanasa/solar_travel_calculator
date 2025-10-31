use std::error::Error;

use solar_orbits::{capture_delta_v, escape_delta_v};
use solar_travel_calculator::config::{PlanetConfig, load_planets, load_vehicle_configs};
use solar_travel_calculator::impulsive::lambert;
use solar_travel_calculator::transfer::vehicle;
use solar_travel_calculator::transfer::{
    AerobrakingOption, ArrivalConfig, DepartureConfig, InterplanetaryConfig, MissionConfig,
    plan_mission,
};

const MU_SUN: f64 = 1.327_124_400_18e11;

#[test]
fn chemical_upper_stage_lambert_consistency() -> Result<(), Box<dyn Error>> {
    let planets = load_planets("configs/bodies")?;
    let vehicles = load_vehicle_configs("configs/vehicles")?;

    let origin = find_body(&planets, "Earth")?;
    let destination = find_body(&planets, "Mars")?;
    let vehicle = vehicle::select(&vehicles, Some("Chemical Upper Stage"))?;

    let departure_cfg = DepartureConfig {
        origin_body: origin.spice_name.clone(),
        parking_altitude_km: origin.default_parking_altitude_km,
        departure_epoch: "2026-01-01T00:00:00".to_string(),
        required_v_infinity: None,
        propulsion_mode: vehicle.propulsion.clone(),
    };

    let cruise_cfg = InterplanetaryConfig {
        departure_body: origin.spice_name.clone(),
        destination_body: destination.spice_name.clone(),
        departure_epoch: departure_cfg.departure_epoch.clone(),
        arrival_epoch: None,
        propulsion_mode: vehicle.propulsion.clone(),
    };

    let arrival_cfg = ArrivalConfig {
        destination_body: destination.spice_name.clone(),
        target_parking_altitude_km: destination.default_parking_altitude_km,
        encounter_epoch: departure_cfg.departure_epoch.clone(),
        propulsion_mode: vehicle.propulsion.clone(),
        aerobraking: Some(AerobrakingOption::Disabled),
    };

    let origin_parking_radius = origin.radius_km + origin.default_parking_altitude_km;
    let origin_mu = origin.mu_km3_s2;
    let destination_parking_radius =
        destination.radius_km + destination.default_parking_altitude_km;
    let destination_mu = destination.mu_km3_s2;

    let mission_cfg = MissionConfig {
        vehicle,
        origin,
        destination,
        departure: departure_cfg,
        cruise: cruise_cfg,
        arrival: arrival_cfg,
    };

    let profile = plan_mission(mission_cfg)?;

    let tof_seconds = profile.cruise.time_of_flight_days * 86_400.0;
    assert!(
        tof_seconds.is_finite() && tof_seconds > 0.0,
        "time of flight should be positive"
    );

    let mut best_dep_v_inf = None;
    let mut best_arr_v_inf = None;
    let mut best_score = None;

    for &short in &[true, false] {
        if let Ok((lambert_v1, lambert_v2)) = lambert::solve(
            profile.cruise.departure_state.position_km,
            profile.cruise.arrival_state.position_km,
            tof_seconds,
            MU_SUN,
            short,
        ) {
            let dep_v_inf = magnitude([
                lambert_v1[0] - profile.cruise.departure_state.velocity_km_s[0],
                lambert_v1[1] - profile.cruise.departure_state.velocity_km_s[1],
                lambert_v1[2] - profile.cruise.departure_state.velocity_km_s[2],
            ]);
            let arr_v_inf = magnitude([
                lambert_v2[0] - profile.cruise.arrival_state.velocity_km_s[0],
                lambert_v2[1] - profile.cruise.arrival_state.velocity_km_s[1],
                lambert_v2[2] - profile.cruise.arrival_state.velocity_km_s[2],
            ]);
            let score = dep_v_inf + arr_v_inf;
            if best_score.map_or(true, |current| score < current) {
                best_score = Some(score);
                best_dep_v_inf = Some(dep_v_inf);
                best_arr_v_inf = Some(arr_v_inf);
            }
        }
    }

    let dep_v_inf = best_dep_v_inf.expect("lambert solution for departure");
    let arr_v_inf = best_arr_v_inf.expect("lambert solution for arrival");

    assert!(
        (dep_v_inf - profile.departure.hyperbolic_excess_km_s).abs() < 1.0e-3,
        "departure v_inf mismatch: expected {:.6}, planner reported {:.6}",
        dep_v_inf,
        profile.departure.hyperbolic_excess_km_s
    );

    let expected_departure_delta = escape_delta_v(origin_mu, origin_parking_radius, dep_v_inf);
    assert!(
        (expected_departure_delta - profile.departure.delta_v_required).abs() < 1.0e-6,
        "departure delta-v mismatch: expected {:.6}, planner reported {:.6}",
        expected_departure_delta,
        profile.departure.delta_v_required
    );

    let expected_capture_delta =
        capture_delta_v(destination_mu, destination_parking_radius, arr_v_inf);
    assert!(
        (expected_capture_delta - profile.arrival.delta_v_required).abs() < 1.0e-6,
        "arrival delta-v mismatch: expected {:.6}, planner reported {:.6}",
        expected_capture_delta,
        profile.arrival.delta_v_required
    );

    Ok(())
}

fn find_body(planets: &[PlanetConfig], name: &str) -> Result<PlanetConfig, Box<dyn Error>> {
    let upper = name.to_uppercase();
    planets
        .iter()
        .find(|p| p.name.to_uppercase() == upper)
        .cloned()
        .ok_or_else(|| format!("Planet/moon '{name}' not found in catalog").into())
}

fn magnitude(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
