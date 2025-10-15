use std::sync::{Mutex, OnceLock};

use solar_travel_calculator::ephemeris;
use solar_travel_calculator::ephemeris::kernels::KERNEL_CATALOG;
use solar_travel_calculator::ephemeris::EphemerisError;

const SPEED_OF_LIGHT_KM_S: f64 = 299_792.458;
const AU_KM: f64 = 149_597_870.7;

fn guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

fn ensure_kernels_or_skip() -> Option<()> {
    match ephemeris::load_default_kernels() {
        Ok(()) => Some(()),
        Err(EphemerisError::MissingKernel { path, .. }) => {
            eprintln!(
                "Skipping ephemeris tests: missing kernel at {}. Run `cargo run --bin fetch_spice` first.",
                path.display()
            );
            None
        }
        Err(err) => panic!("Unexpected SPICE initialization error: {err}"),
    }
}

#[test]
fn kernel_catalog_is_present_and_indexable() {
    let _lock = guard().lock().unwrap();
    if ensure_kernels_or_skip().is_none() {
        return;
    }

    let summaries = ephemeris::kernel_summaries().expect("kernel summaries should load");
    assert_eq!(
        summaries.len(),
        KERNEL_CATALOG.len(),
        "all catalog kernels should be reported"
    );

    for summary in summaries {
        assert!(
            summary.file_size_bytes > 0,
            "kernel {} should have non-zero size",
            summary.descriptor.filename
        );
    }
}

#[test]
fn earth_heliocentric_state_vector_is_reasonable() {
    let _lock = guard().lock().unwrap();
    if ensure_kernels_or_skip().is_none() {
        return;
    }

    let state = ephemeris::state_vector(
        "EARTH",
        "SUN",
        "ECLIPJ2000",
        "NONE",
        "2024 JAN 01 00:00:00 TDB",
    )
    .expect("SPICE state vector should resolve");

    let distance = (state.position_km[0].powi(2)
        + state.position_km[1].powi(2)
        + state.position_km[2].powi(2))
        .sqrt();
    assert!(
        (AU_KM * 0.95..=AU_KM * 1.05).contains(&distance),
        "Earth-Sun distance should be ~1 AU (got {distance} km)"
    );

    let speed = (state.velocity_km_s[0].powi(2)
        + state.velocity_km_s[1].powi(2)
        + state.velocity_km_s[2].powi(2))
        .sqrt();
    assert!(
        (25.0..=40.0).contains(&speed),
        "Earth heliocentric speed should be ~30 km/s (got {speed} km/s)"
    );

    let expected_light_time = distance / SPEED_OF_LIGHT_KM_S;
    let light_time_delta = (state.light_time_seconds - expected_light_time).abs();
    assert!(
        light_time_delta < 1.0,
        "Light time should match distance/c within 1s (delta {light_time_delta})"
    );
}
