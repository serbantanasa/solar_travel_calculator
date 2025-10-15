use std::sync::{Mutex, OnceLock};

use solar_travel_calculator::ephemeris::EphemerisError;
use solar_travel_calculator::mission::arrival::AerobrakingOption;
use solar_travel_calculator::mission::departure::DepartureConfig;
use solar_travel_calculator::mission::interplanetary::InterplanetaryConfig;
use solar_travel_calculator::mission::{MissionConfig, arrival::ArrivalConfig, plan_mission};
use solar_travel_calculator::scenario::{load_planets, load_vehicles};

fn guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

fn ensure_kernels_or_skip() -> Option<()> {
    match solar_travel_calculator::ephemeris::load_default_kernels() {
        Ok(()) => Some(()),
        Err(EphemerisError::MissingKernel { path, .. }) => {
            eprintln!(
                "Skipping mission test: missing kernel at {}. Run `cargo run --bin fetch_spice` first.",
                path.display()
            );
            None
        }
        Err(err) => panic!("Unexpected SPICE initialization error: {err}"),
    }
}

#[test]
fn stub_mission_planner_runs() {
    let _lock = guard().lock().unwrap();
    if ensure_kernels_or_skip().is_none() {
        return;
    }

    let planets = load_planets("data/scenarios/planets.yaml").expect("planets yaml");
    let vehicles = load_vehicles("data/scenarios/vehicles.yaml").expect("vehicles yaml");
    let origin = planets.iter().find(|p| p.name == "EARTH").unwrap().clone();
    let destination = planets.iter().find(|p| p.name == "MARS").unwrap().clone();
    let scenario_vehicle = vehicles
        .iter()
        .find(|v| v.name.contains("Ion"))
        .unwrap()
        .clone();
    let propulsion_mode = scenario_vehicle.propulsion.clone();

    let departure = DepartureConfig {
        origin_body: origin.spice_name.clone(),
        parking_altitude_km: origin.default_parking_altitude_km,
        departure_epoch: "2025 OCT 14 23:28:58 TDB".to_string(),
        required_v_infinity: Some(3.2),
        propulsion_mode: propulsion_mode.clone(),
    };

    let cruise = InterplanetaryConfig {
        departure_body: origin.spice_name.clone(),
        destination_body: destination.spice_name.clone(),
        departure_epoch: "2025 OCT 14 23:28:58 TDB".to_string(),
        arrival_epoch: Some("2026 APR 12 23:28:58 TDB".to_string()),
        propulsion_mode: propulsion_mode.clone(),
    };

    let arrival = ArrivalConfig {
        destination_body: destination.spice_name.clone(),
        target_parking_altitude_km: destination.default_parking_altitude_km,
        encounter_epoch: "2026 APR 12 23:28:58 TDB".to_string(),
        propulsion_mode: propulsion_mode.clone(),
        aerobraking: Some(AerobrakingOption::Partial {
            periapsis_altitude_km: 80.0,
        }),
    };

    let profile = plan_mission(MissionConfig {
        vehicle: scenario_vehicle,
        origin,
        destination,
        departure,
        cruise,
        arrival,
    })
    .expect("mission planner should return placeholder results");

    assert!(profile.departure.delta_v_required > 0.0);
    assert!(profile.cruise.time_of_flight_days > 0.0);
    assert!(profile.arrival.delta_v_required >= 0.0);
}
