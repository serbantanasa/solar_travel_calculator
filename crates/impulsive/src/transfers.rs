//! Analytic estimators for impulsive transfers in the coplanar, circular limit.
//!
//! Provides Hohmann and bi-elliptic transfer calculators that return delta-v components
//! and time of flight for two-body Keplerian motion with a specified central GM.

/// Result for a Hohmann transfer between circular, coplanar orbits of radii r1 and r2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HohmannResult {
    pub dv1_km_s: f64,      // signed: negative for inward (retro) burn
    pub dv2_km_s: f64,      // signed: negative for retro capture when arriving inward
    pub dv_total_km_s: f64, // |dv1| + |dv2|
    pub tof_seconds: f64,
}

/// Compute the classical Hohmann transfer between two circular coplanar orbits.
///
/// Inputs:
/// - `r1_km`: initial circular orbit radius (km)
/// - `r2_km`: target circular orbit radius (km)
/// - `mu_km3_s2`: gravitational parameter of central body (km^3/s^2)
pub fn hohmann(r1_km: f64, r2_km: f64, mu_km3_s2: f64) -> HohmannResult {
    assert!(r1_km > 0.0 && r2_km > 0.0 && mu_km3_s2 > 0.0);

    let v1 = (mu_km3_s2 / r1_km).sqrt();
    let v2 = (mu_km3_s2 / r2_km).sqrt();
    let a_t = 0.5 * (r1_km + r2_km);
    let tof = std::f64::consts::PI * (a_t.powi(3) / mu_km3_s2).sqrt();

    // Transfer periapsis speed (at r1) and apoapsis speed (at r2)
    let v_t1 = (mu_km3_s2 * (2.0 / r1_km - 1.0 / a_t)).sqrt();
    let v_t2 = (mu_km3_s2 * (2.0 / r2_km - 1.0 / a_t)).sqrt();

    // Signed burns: inward transfers have negative dv1 (retro burn) and negative dv2 (retro capture)
    let dv1 = v_t1 - v1; // positive for outward, negative for inward
    let dv2 = v2 - v_t2; // positive for outward (prograde capture), negative for inward (retro capture)
    let dv_total = dv1.abs() + dv2.abs();

    HohmannResult {
        dv1_km_s: dv1,
        dv2_km_s: dv2,
        dv_total_km_s: dv_total,
        tof_seconds: tof,
    }
}

/// Result for a bi-elliptic transfer parameterized by the intermediate apoapsis radius r_b.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiEllipticResult {
    pub rb_km: f64,
    pub dv1_km_s: f64,
    pub dv2_km_s: f64,
    pub dv3_km_s: f64,
    pub dv_total_km_s: f64,
    pub tof_seconds: f64,
}

/// Compute a bi-elliptic transfer using a specified intermediate apoapsis radius `rb_km`.
///
/// This function does not optimize `rb_km`; it evaluates the three impulsive burns and TOF
/// for the two transfer ellipses: (r1 -> rb) and (rb -> r2). Users may sweep `rb_km` to study
/// trade-offs; for very large r2/r1 ratios, bi-elliptic can beat Hohmann beyond ~11.94.
pub fn bi_elliptic(r1_km: f64, r2_km: f64, rb_km: f64, mu_km3_s2: f64) -> BiEllipticResult {
    assert!(r1_km > 0.0 && r2_km > 0.0 && rb_km > 0.0 && mu_km3_s2 > 0.0);

    let v1 = (mu_km3_s2 / r1_km).sqrt();
    let v2 = (mu_km3_s2 / r2_km).sqrt();

    // First ellipse: r1 -> rb
    let a1 = 0.5 * (r1_km + rb_km);
    let v_peri_1 = (mu_km3_s2 * (2.0 / r1_km - 1.0 / a1)).sqrt();
    let v_apo_1 = (mu_km3_s2 * (2.0 / rb_km - 1.0 / a1)).sqrt();

    // Second ellipse: rb -> r2
    let a2 = 0.5 * (rb_km + r2_km);
    let v_peri_2 = (mu_km3_s2 * (2.0 / rb_km - 1.0 / a2)).sqrt();
    let v_apo_2 = (mu_km3_s2 * (2.0 / r2_km - 1.0 / a2)).sqrt();

    // Burns (signed)
    let dv1 = v_peri_1 - v1; // at r1
    let dv2 = v_peri_2 - v_apo_1; // at rb (match velocities)
    let dv3 = v2 - v_apo_2; // at r2

    let tof =
        std::f64::consts::PI * ((a1.powi(3) / mu_km3_s2).sqrt() + (a2.powi(3) / mu_km3_s2).sqrt());
    let dv_total = dv1.abs() + dv2.abs() + dv3.abs();

    BiEllipticResult {
        rb_km,
        dv1_km_s: dv1,
        dv2_km_s: dv2,
        dv3_km_s: dv3,
        dv_total_km_s: dv_total,
        tof_seconds: tof,
    }
}
