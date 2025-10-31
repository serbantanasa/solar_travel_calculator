use solar_travel_calculator::impulsive::transfers::{bi_elliptic, hohmann};

const MU_SUN: f64 = 1.327_124_400_18e11; // km^3 / s^2
const AU_KM: f64 = 149_597_870.7; // km

#[test]
fn hohmann_symmetry_and_time_match() {
    let r1 = 1.0 * AU_KM;
    let r2 = 1.524 * AU_KM; // Mars mean distance approx
    let h12 = hohmann(r1, r2, MU_SUN);
    let h21 = hohmann(r2, r1, MU_SUN);

    // Total dv symmetric under exchange of r1 and r2
    assert!((h12.dv_total_km_s - h21.dv_total_km_s).abs() < 1e-9);
    // Time of flight equal in both directions for Hohmann
    assert!((h12.tof_seconds - h21.tof_seconds).abs() < 1e-6);

    // Sanity: outward transfer dv1 positive, inward transfer dv1 negative
    assert!(h12.dv1_km_s > 0.0);
    assert!(h21.dv1_km_s < 0.0);
}

#[test]
fn bielliptic_can_outperform_hohmann_for_large_ratios() {
    let r1 = 1.0 * AU_KM;
    let r2 = 20.0 * AU_KM; // very large ratio
    let ho = hohmann(r1, r2, MU_SUN);
    // Choose a large intermediate apoapsis to illustrate potential benefit
    let rb = 50.0 * AU_KM;
    let bi = bi_elliptic(r1, r2, rb, MU_SUN);
    assert!(bi.dv_total_km_s < ho.dv_total_km_s);
}

#[test]
fn hohmann_earth_mars_reasonable_numbers() {
    // Semi-major axis ratios approximated by mean orbital radii in AU
    let r_earth = 1.0 * AU_KM;
    let r_mars = 1.523679 * AU_KM;
    let h = hohmann(r_earth, r_mars, MU_SUN);
    // Expected total dv ~ 5.6 km/s, TOF ~ 250-300 days (rough window)
    assert!(
        (h.dv_total_km_s - 5.6).abs() < 0.7,
        "dv_total = {}",
        h.dv_total_km_s
    );
    let days = h.tof_seconds / 86_400.0;
    assert!((200.0..=350.0).contains(&days), "tof_days = {}", days);
}

#[test]
fn hohmann_earth_venus_reasonable_numbers() {
    let r_earth = 1.0 * AU_KM;
    let r_venus = 0.723 * AU_KM;
    let h = hohmann(r_earth, r_venus, MU_SUN);
    assert!(
        (h.dv_total_km_s - 5.21).abs() < 0.1,
        "dv_total = {}",
        h.dv_total_km_s
    );
    let days = h.tof_seconds / 86_400.0;
    assert!((days - 145.0).abs() < 5.0, "tof_days = {}", days);
}
