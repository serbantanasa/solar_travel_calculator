use solar_travel_calculator::scenario::{load_planets, load_vehicles};

#[test]
fn scenario_catalog_contains_major_bodies() {
    let planets = load_planets("data/scenarios/planets.yaml").expect("planets yaml");
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
    let vehicles = load_vehicles("data/scenarios/vehicles.yaml").expect("vehicles yaml");
    assert!(vehicles.iter().any(|v| v.name.contains("Epstein")));
    let epstein = vehicles
        .iter()
        .find(|v| v.name.contains("Epstein"))
        .unwrap();
    match &epstein.propulsion {
        solar_travel_calculator::mission::propulsion::PropulsionMode::Continuous {
            isp_seconds,
            ..
        } => {
            assert!(
                *isp_seconds > 1.0e5,
                "Epstein drive should have enormous ISP"
            );
        }
        _ => panic!("Epstein drive must be continuous"),
    }
}
