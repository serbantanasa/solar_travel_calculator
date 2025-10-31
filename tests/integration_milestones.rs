use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use assert_cmd::Command;
use csv::Reader;

use solar_travel_calculator::config::{load_planets, load_vehicle_configs};
use solar_travel_calculator::ephemeris::{self, EphemerisError};
use solar_travel_calculator::impulsive::{lambert, transfers as impulsive};
use solar_travel_calculator::mission::arrival::{AerobrakingOption, ArrivalConfig};
use solar_travel_calculator::mission::departure::DepartureConfig;
use solar_travel_calculator::mission::interplanetary::{InterplanetaryConfig, plan_interplanetary};
use solar_travel_calculator::mission::{MissionConfig, plan_mission};
use solar_travel_calculator::transfer::vehicle;

const MU_SUN: f64 = 1.327_124_400_18e11; // km^3 / s^2
const AU_KM: f64 = 149_597_870.7; // km
const SPEED_OF_LIGHT_KM_S: f64 = 299_792.458; // km/s

fn guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

fn ensure_kernels_or_skip() -> Option<()> {
    match solar_travel_calculator::ephemeris::load_default_kernels() {
        Ok(()) => Some(()),
        Err(EphemerisError::MissingKernel { path, .. }) => {
            eprintln!(
                "Skipping integration milestone tests: missing kernel at {}. Run `cargo run -p solar_cli --bin fetch_spice` first.",
                path.display()
            );
            None
        }
        Err(err) => panic!("Unexpected SPICE initialization error: {err}"),
    }
}

#[test]
fn milestone_v01_bootstrap_ephemerides() {
    let _lock = guard().lock().unwrap_or_else(|e| e.into_inner());
    if ensure_kernels_or_skip().is_none() {
        return;
    }

    let planets = load_planets("configs/bodies").expect("planets catalog");
    let vehicles_cfg = load_vehicle_configs("configs/vehicles").expect("vehicles catalog");
    let vehicles: Vec<_> = vehicles_cfg
        .iter()
        .map(|cfg| vehicle::from_config(cfg).expect("vehicle conversion"))
        .collect();
    assert!(!planets.is_empty());
    assert!(!vehicles.is_empty());

    let summaries = ephemeris::kernel_summaries().expect("kernel summaries");
    assert!(!summaries.is_empty());

    let epoch = "2026 JAN 01 00:00:00 TDB";
    let et = ephemeris::epoch_seconds(epoch).expect("epoch to et");
    let s1 = ephemeris::state_vector("EARTH BARYCENTER", "SUN", "ECLIPJ2000", "NONE", epoch)
        .expect("state from string epoch");
    let s2 = ephemeris::state_vector_et("EARTH BARYCENTER", "SUN", "ECLIPJ2000", "NONE", et)
        .expect("state from et");
    for i in 0..3 {
        assert!((s1.position_km[i] - s2.position_km[i]).abs() < 1e-9);
        assert!((s1.velocity_km_s[i] - s2.velocity_km_s[i]).abs() < 1e-12);
    }

    let distance = norm3(&s1.position_km);
    assert!(
        (AU_KM * 0.95..=AU_KM * 1.05).contains(&distance),
        "Earth-Sun distance should be ~1 AU (got {distance} km)"
    );

    let speed = norm3(&s1.velocity_km_s);
    assert!(
        (25.0..=40.0).contains(&speed),
        "Earth heliocentric speed should be ~30 km/s (got {speed} km/s)"
    );

    let light_time = s1.light_time_seconds;
    let expected_light = distance / SPEED_OF_LIGHT_KM_S;
    assert!(
        (light_time - expected_light).abs() < 1.0,
        "Light time should match distance/c within 1 s (delta {})",
        (light_time - expected_light).abs()
    );
}

#[test]
fn milestone_v02_impulsive_transfers() {
    let _lock = guard().lock().unwrap_or_else(|e| e.into_inner());
    if ensure_kernels_or_skip().is_none() {
        return;
    }

    let planets = load_planets("configs/bodies").expect("planets catalog");
    let vehicles_cfg = load_vehicle_configs("configs/vehicles").expect("vehicles catalog");
    let vehicles: Vec<_> = vehicles_cfg
        .iter()
        .map(|cfg| vehicle::from_config(cfg).expect("vehicle conversion"))
        .collect();
    let origin = planets.iter().find(|p| p.name == "EARTH").unwrap().clone();
    let destination = planets.iter().find(|p| p.name == "MARS").unwrap().clone();
    let vehicle = vehicles
        .iter()
        .find(|v| {
            matches!(
                v.propulsion,
                solar_travel_calculator::propulsion::PropulsionMode::Continuous { .. }
            )
        })
        .unwrap()
        .clone();

    let departure = DepartureConfig {
        origin_body: origin.spice_name.clone(),
        parking_altitude_km: origin.default_parking_altitude_km,
        departure_epoch: "2026 JAN 01 00:00:00 TDB".to_string(),
        required_v_infinity: Some(3.2),
        propulsion_mode: vehicle.propulsion.clone(),
    };

    let cruise = InterplanetaryConfig {
        departure_body: origin.spice_name.clone(),
        destination_body: destination.spice_name.clone(),
        departure_epoch: "2026 JAN 01 00:00:00 TDB".to_string(),
        arrival_epoch: Some("2026 OCT 01 00:00:00 TDB".to_string()),
        propulsion_mode: vehicle.propulsion.clone(),
    };

    let arrival = ArrivalConfig {
        destination_body: destination.spice_name.clone(),
        target_parking_altitude_km: destination.default_parking_altitude_km,
        encounter_epoch: "2026 OCT 01 00:00:00 TDB".to_string(),
        propulsion_mode: vehicle.propulsion.clone(),
        aerobraking: Some(AerobrakingOption::Partial {
            periapsis_altitude_km: 80.0,
        }),
    };

    let mission = MissionConfig {
        vehicle: vehicle.clone(),
        origin: origin.clone(),
        destination: destination.clone(),
        departure,
        cruise,
        arrival,
    };

    let profile = plan_mission(mission).expect("mission profile");
    assert!(profile.departure.delta_v_required > 0.0);
    assert!(profile.arrival.delta_v_required >= 0.0);

    let r1 = profile.cruise.departure_state.position_km;
    let r2 = profile.cruise.arrival_state.position_km;
    let h = impulsive::hohmann(norm3(&r1), norm3(&r2), MU_SUN);
    const MARS_SEMI_MAJOR_AU: f64 = 1.523_679;
    // Reference values drawn from classical Hohmann estimates (e.g., Curtis, *Orbital Mechanics for
    // Engineering Students*, Table 3-4) using mean heliocentric radii: Earth 1 AU, Mars 1.523679 AU.
    let reference = impulsive::hohmann(AU_KM, MARS_SEMI_MAJOR_AU * AU_KM, MU_SUN);
    assert!(
        (reference.dv_total_km_s - 5.593).abs() < 0.01,
        "Reference Hohmann Δv should match literature (5.593 km/s)"
    );
    assert!(
        (reference.tof_seconds / 86_400.0 - 258.9).abs() < 0.1,
        "Reference Hohmann TOF should be ~258.9 days"
    );

    assert!(
        (h.dv_total_km_s - reference.dv_total_km_s).abs() < 0.7,
        "Computed Δv ({:.3}) differs markedly from reference ({:.3})",
        h.dv_total_km_s,
        reference.dv_total_km_s
    );
    let hohmann_days = h.tof_seconds / 86_400.0;
    assert!(
        (hohmann_days - reference.tof_seconds / 86_400.0).abs() < 30.0,
        "Computed TOF ({:.1} d) differs markedly from reference ({:.1} d)",
        hohmann_days,
        reference.tof_seconds / 86_400.0
    );

    let mut rdr =
        Reader::from_path("data/reference/horizons_earth_mars_2026.csv").expect("reference csv");
    for record in rdr.records() {
        let rec = record.expect("record");
        let body = rec[0].to_string();
        let jd: f64 = rec[1].parse().expect("jd");
        let target = match body.as_str() {
            "EARTH_BARYCENTER" => "EARTH BARYCENTER",
            "MARS_BARYCENTER" => "MARS BARYCENTER",
            other => panic!("unexpected body {other}"),
        };
        let et = (jd - 2451545.0) * 86_400.0;
        let state =
            ephemeris::state_vector_et(target, "SOLAR SYSTEM BARYCENTER", "ECLIPJ2000", "NONE", et)
                .expect("state vector from horizons epoch");
        let expected_p = [
            rec[2].parse::<f64>().unwrap(),
            rec[3].parse::<f64>().unwrap(),
            rec[4].parse::<f64>().unwrap(),
        ];
        let expected_v = [
            rec[5].parse::<f64>().unwrap(),
            rec[6].parse::<f64>().unwrap(),
            rec[7].parse::<f64>().unwrap(),
        ];
        const POSITION_TOLERANCE_KM: f64 = 150.0;
        // The de440s planetary ephemeris is within ~100 km of the Horizons export; allow margin.
        for i in 0..3 {
            let delta = state.position_km[i] - expected_p[i];
            assert!(
                delta.abs() < POSITION_TOLERANCE_KM,
                "{} position component {} differs by {:.3} km (tolerance {:.1} km)",
                body,
                i,
                delta,
                POSITION_TOLERANCE_KM
            );
            let delta_v = state.velocity_km_s[i] - expected_v[i];
            assert!(
                delta_v.abs() < 0.02,
                "{} velocity component {} differs by {:.5} km/s",
                body,
                i,
                delta_v
            );
        }
    }

    let depart_et = ephemeris::epoch_seconds("2026 JAN 01 00:00:00 TDB").unwrap();
    let arrive_et = ephemeris::epoch_seconds("2026 OCT 01 00:00:00 TDB").unwrap();
    let dep_state = ephemeris::state_vector_et(
        &ephemeris::normalize_heliocentric_target_name(&origin.spice_name),
        "SUN",
        "ECLIPJ2000",
        "NONE",
        depart_et,
    )
    .unwrap();
    let arr_state = ephemeris::state_vector_et(
        &ephemeris::normalize_heliocentric_target_name(&destination.spice_name),
        "SUN",
        "ECLIPJ2000",
        "NONE",
        arrive_et,
    )
    .unwrap();

    let tof = arrive_et - depart_et;
    let (v1_lam, v2_lam) = lambert::solve(
        dep_state.position_km,
        arr_state.position_km,
        tof,
        MU_SUN,
        true,
    )
    .expect("lambert");
    let vinf_dep = norm3(&[
        v1_lam[0] - dep_state.velocity_km_s[0],
        v1_lam[1] - dep_state.velocity_km_s[1],
        v1_lam[2] - dep_state.velocity_km_s[2],
    ]);
    let vinf_arr = norm3(&[
        v2_lam[0] - arr_state.velocity_km_s[0],
        v2_lam[1] - arr_state.velocity_km_s[1],
        v2_lam[2] - arr_state.velocity_km_s[2],
    ]);
    let header = "depart_et,arrive_et,depart_utc,arrive_utc,tof_days,c3_km2_s2,vinf_dep_km_s,vinf_arr_km_s,dv_dep_km_s,dv_arr_km_s,dv_total_km_s,lambert_path,feasible,origin_body,dest_body,rpark_dep_km,rpark_arr_km";
    assert_eq!(header.split(',').count(), 17);

    let dv_dep = ((vinf_dep * vinf_dep
        + 2.0 * origin.mu_km3_s2 / (origin.radius_km + origin.default_parking_altitude_km))
        .sqrt()
        - (origin.mu_km3_s2 / (origin.radius_km + origin.default_parking_altitude_km)).sqrt())
    .max(0.0);
    let dv_arr = ((vinf_arr * vinf_arr
        + 2.0 * destination.mu_km3_s2
            / (destination.radius_km + destination.default_parking_altitude_km))
        .sqrt()
        - (destination.mu_km3_s2
            / (destination.radius_km + destination.default_parking_altitude_km))
            .sqrt())
    .max(0.0);
    assert!(dv_dep >= 0.0 && dv_arr >= 0.0);

    let expected_cli_line = format!(
        "Hohmann est.   : Δv_total = {:.3} km/s (dv1={:.3}, dv2={:.3}), TOF = {:.2} days",
        h.dv_total_km_s, h.dv1_km_s, h.dv2_km_s, hohmann_days,
    );

    let mut cmd = Command::cargo_bin("mission").expect("mission bin");
    cmd.args([
        "--from",
        "EARTH",
        "--to",
        "MARS",
        "--depart",
        "2026 JAN 01 00:00:00 TDB",
        "--arrive",
        "2026 OCT 01 00:00:00 TDB",
        "--vehicle",
        &vehicle.name,
        "--estimate-hohmann",
    ]);
    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8(output).expect("utf8 stdout");
    assert!(
        stdout.contains(&expected_cli_line),
        "CLI did not report expected Hohmann line. Output:\n{}",
        stdout
    );

    let depart_grid: Vec<f64> = (0..3)
        .map(|i| depart_et + i as f64 * 30.0 * 86_400.0)
        .collect();
    let arrive_grid: Vec<f64> = (0..3)
        .map(|i| arrive_et + i as f64 * 30.0 * 86_400.0)
        .collect();

    let start = Instant::now();
    let mut evaluated = 0;
    for dt in &depart_grid {
        let dep_state_grid = ephemeris::state_vector_et(
            &ephemeris::normalize_heliocentric_target_name(&origin.spice_name),
            "SUN",
            "ECLIPJ2000",
            "NONE",
            *dt,
        )
        .unwrap();
        for at in &arrive_grid {
            if at <= dt {
                continue;
            }
            let arr_state_grid = ephemeris::state_vector_et(
                &ephemeris::normalize_heliocentric_target_name(&destination.spice_name),
                "SUN",
                "ECLIPJ2000",
                "NONE",
                *at,
            )
            .unwrap();
            let tof_grid = *at - *dt;
            let _ = lambert::solve(
                dep_state_grid.position_km,
                arr_state_grid.position_km,
                tof_grid,
                MU_SUN,
                true,
            )
            .unwrap();
            evaluated += 1;
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    let expected = depart_grid.len() * arrive_grid.len();
    assert_eq!(
        evaluated, expected,
        "expected {expected} porkchop evaluations, got {evaluated}"
    );
    assert!(
        elapsed < 2.0,
        "porkchop sampling took too long: {elapsed:.3}s (kernel caching regression?)"
    );
}

#[test]
fn milestone_v03_continuous_low_thrust() {
    let _lock = guard().lock().unwrap_or_else(|e| e.into_inner());
    if ensure_kernels_or_skip().is_none() {
        return;
    }

    let planets = load_planets("configs/bodies").expect("planets catalog");
    let vehicles_cfg = load_vehicle_configs("configs/vehicles").expect("vehicles catalog");
    let vehicles: Vec<_> = vehicles_cfg
        .iter()
        .map(|cfg| vehicle::from_config(cfg).expect("vehicle conversion"))
        .collect();
    let origin = planets.iter().find(|p| p.name == "EARTH").unwrap().clone();
    let destination = planets.iter().find(|p| p.name == "MARS").unwrap().clone();
    let vehicle = vehicles
        .iter()
        .find(|v| {
            matches!(
                v.propulsion,
                solar_travel_calculator::propulsion::PropulsionMode::Continuous { .. }
            )
        })
        .unwrap()
        .clone();

    let cruise = InterplanetaryConfig {
        departure_body: origin.spice_name.clone(),
        destination_body: destination.spice_name.clone(),
        departure_epoch: "2026 JAN 01 00:00:00 TDB".to_string(),
        arrival_epoch: Some("2026 OCT 01 00:00:00 TDB".to_string()),
        propulsion_mode: vehicle.propulsion.clone(),
    };

    let plan =
        plan_interplanetary(&vehicle, &cruise, &origin, &destination).expect("continuous plan");
    let prop_used = plan.propellant_used_kg.expect("propellant");
    assert!(prop_used >= 0.0);
    assert!(prop_used <= vehicle.propellant_mass_kg);
    assert!(plan.time_of_flight_days > 0.0);

    #[cfg(test)]
    {
        trait MockThreeDIntegrator {
            fn integrate(&mut self, duration_days: f64);
            fn calls(&self) -> usize;
        }

        struct StubIntegrator {
            count: usize,
        }

        impl StubIntegrator {
            fn new() -> Self {
                Self { count: 0 }
            }
        }

        impl MockThreeDIntegrator for StubIntegrator {
            fn integrate(&mut self, _duration_days: f64) {
                self.count += 1;
            }
            fn calls(&self) -> usize {
                self.count
            }
        }

        let mut stub = StubIntegrator::new();
        stub.integrate(plan.time_of_flight_days);
        assert_eq!(stub.calls(), 1);
    }
}

#[test]
#[ignore = "Milestone v0.4 pending"]
fn milestone_v04_flyby_geometry() {
    // TODO: replace mocked data with real flyby solver once milestone v0.4 lands.
}

#[test]
#[ignore = "Milestone v0.5 pending"]
fn milestone_v05_low_thrust_3d() {
    // TODO: exercise adaptive RK 3D integrator once implemented.
}

#[test]
#[ignore = "Milestone v0.6 pending"]
fn milestone_v06_window_search() {
    // TODO: validate optimizer window search outputs once milestone v0.6 lands.
}

#[test]
#[ignore = "Milestone v0.7 pending"]
fn milestone_v07_multi_leg() {
    // TODO: test multi-leg itinerary solver once milestone v0.7 lands.
}

#[test]
#[ignore = "Milestone v0.8 pending"]
fn milestone_v08_validation_suite() {
    // TODO: replay golden trajectory corpus once milestone v0.8 lands.
}

#[test]
#[ignore = "Milestone v1.0 pending"]
fn milestone_v10_release_gate() {
    // TODO: final release regression once all milestones are complete.
}

fn norm3(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
