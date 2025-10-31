use std::sync::{Mutex, OnceLock};

use solar_travel_calculator::config::{load_planets, load_vehicle_configs};
use solar_travel_calculator::mission::MissionConfig;
use solar_travel_calculator::mission::arrival::{AerobrakingOption, ArrivalConfig, plan_arrival};
use solar_travel_calculator::mission::departure::{DepartureConfig, plan_departure};
use solar_travel_calculator::mission::interplanetary::{InterplanetaryConfig, plan_interplanetary};
use solar_travel_calculator::transfer::vehicle;

fn guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

fn ensure_kernels_or_skip() -> Option<()> {
    match solar_travel_calculator::ephemeris::load_default_kernels() {
        Ok(()) => Some(()),
        Err(solar_travel_calculator::ephemeris::EphemerisError::MissingKernel { path, .. }) => {
            eprintln!(
                "Skipping phase tests: missing kernel at {}. Run `cargo run -p solar_cli --bin fetch_spice` first.",
                path.display()
            );
            None
        }
        Err(err) => panic!("Unexpected SPICE initialization error: {err}"),
    }
}

fn earth_mars_setup() -> (
    MissionConfig,
    solar_travel_calculator::mission::interplanetary::InterplanetaryPlan,
) {
    let planets = load_planets("configs/bodies").expect("planets catalog");
    let vehicles_cfg = load_vehicle_configs("configs/vehicles").expect("vehicles catalog");
    let vehicles: Vec<_> = vehicles_cfg
        .iter()
        .map(|cfg| vehicle::from_config(cfg).expect("convert vehicle"))
        .collect();

    let origin = planets.iter().find(|p| p.name == "EARTH").unwrap().clone();
    let destination = planets.iter().find(|p| p.name == "MARS").unwrap().clone();
    let vehicle = vehicles
        .iter()
        .find(|v| v.name.contains("Ion"))
        .unwrap()
        .clone();
    let propulsion_mode = vehicle.propulsion.clone();

    let departure_cfg = DepartureConfig {
        origin_body: origin.spice_name.clone(),
        parking_altitude_km: origin.default_parking_altitude_km,
        departure_epoch: "2025 OCT 14 23:28:58 TDB".to_string(),
        required_v_infinity: Some(3.2),
        propulsion_mode: propulsion_mode.clone(),
    };

    let cruise_cfg = InterplanetaryConfig {
        departure_body: origin.spice_name.clone(),
        destination_body: destination.spice_name.clone(),
        departure_epoch: "2025 OCT 14 23:28:58 TDB".to_string(),
        arrival_epoch: Some("2026 APR 12 23:28:58 TDB".to_string()),
        propulsion_mode: propulsion_mode.clone(),
    };

    let arrival_cfg = ArrivalConfig {
        destination_body: destination.spice_name.clone(),
        target_parking_altitude_km: destination.default_parking_altitude_km,
        encounter_epoch: "2026 APR 12 23:28:58 TDB".to_string(),
        propulsion_mode: propulsion_mode,
        aerobraking: None,
    };

    let cruise =
        plan_interplanetary(&vehicle, &cruise_cfg, &origin, &destination).expect("interplanetary");

    (
        MissionConfig {
            vehicle,
            origin,
            destination,
            departure: departure_cfg,
            cruise: cruise_cfg,
            arrival: arrival_cfg,
        },
        cruise,
    )
}

#[test]
fn departure_delta_v_is_positive() {
    let _lock = guard().lock().unwrap();
    if ensure_kernels_or_skip().is_none() {
        return;
    }
    let (config, cruise) = earth_mars_setup();
    let departure = plan_departure(
        &config.vehicle,
        &config.departure,
        &config.origin,
        &config.cruise,
        &cruise,
    )
    .expect("departure");

    assert!(departure.delta_v_required > 0.0);
    assert!(departure.hyperbolic_excess_km_s > 0.0);
}

#[test]
fn aerobraking_reduces_capture_delta_v() {
    let _lock = guard().lock().unwrap();
    if ensure_kernels_or_skip().is_none() {
        return;
    }
    let (mut config, cruise) = earth_mars_setup();
    let propulsive = plan_arrival(
        &config.vehicle,
        &config.arrival,
        &config.destination,
        &config.cruise,
        None,
        &cruise,
    )
    .expect("arrival");

    config.arrival.aerobraking = Some(AerobrakingOption::Full {
        periapsis_altitude_km: 80.0,
    });
    let aero = plan_arrival(
        &config.vehicle,
        &config.arrival,
        &config.destination,
        &config.cruise,
        config.arrival.aerobraking,
        &cruise,
    )
    .expect("arrival aero");

    assert!(aero.delta_v_required < propulsive.delta_v_required);
}

#[test]
fn continuous_solver_consumes_propellant() {
    let _lock = guard().lock().unwrap();
    if ensure_kernels_or_skip().is_none() {
        return;
    }
    let (config, cruise) = earth_mars_setup();
    let prop_used = cruise.propellant_used_kg.expect("propellant");
    assert!(prop_used >= 0.0);
    assert!(prop_used <= config.vehicle.propellant_mass_kg);
}
