use solar_travel_calculator::config::{load_planets, load_vehicle_configs};
use solar_travel_calculator::transfer::vehicle;

#[test]
fn scenario_catalog_contains_major_bodies() {
    let planets = load_planets("configs/bodies").expect("planets catalog");
    assert!(planets.len() >= 10);
    assert!(planets.iter().any(|p| p.name == "JUPITER"));
    assert!(planets.iter().any(|p| p.name == "PLUTO"));
    assert!(planets.iter().any(|p| p.name == "TITAN"));
    let earth = planets.iter().find(|p| p.name == "EARTH").unwrap();
    assert!(earth.surface_gravity_m_s2 > 9.7 && earth.surface_gravity_m_s2 < 10.0);
    assert!(earth.mass_kg > 5.9e24 && earth.mass_kg < 6.1e24);
}

#[test]
fn scenario_vehicles_include_epstein_drive() {
    let vehicles_cfg = load_vehicle_configs("configs/vehicles").expect("vehicles catalog");
    let vehicles: Vec<_> = vehicles_cfg
        .iter()
        .map(|cfg| vehicle::from_config(cfg).expect("vehicle conversion"))
        .collect();
    assert!(vehicles.iter().any(|v| v.name.contains("Epstein")));
    let epstein = vehicles
        .iter()
        .find(|v| v.name.contains("Epstein"))
        .unwrap();
    match epstein.propulsion {
        solar_travel_calculator::propulsion::PropulsionMode::Continuous { isp_seconds, .. } => {
            assert!(
                isp_seconds > 1.0e5,
                "Epstein drive should have enormous ISP"
            );
        }
        _ => panic!("Epstein drive must be continuous"),
    }
}
