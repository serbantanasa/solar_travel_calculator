//! Orbit utility helpers (patched-conic escape/capture estimates).
use solar_core::vector::{self, Vector3};

/// Euclidean norm helper maintained for backwards compatibility.
pub fn norm3(v: &Vector3) -> f64 {
    vector::norm(v)
}

/// Dot product helper maintained for backwards compatibility.
pub fn dot(a: &Vector3, b: &Vector3) -> f64 {
    vector::dot(a, b)
}

/// Vector addition helper maintained for backwards compatibility.
pub fn add(a: &Vector3, b: &Vector3) -> Vector3 {
    vector::add(a, b)
}

/// Vector subtraction helper maintained for backwards compatibility.
pub fn sub(a: &Vector3, b: &Vector3) -> Vector3 {
    vector::sub(a, b)
}

/// Vector scaling helper maintained for backwards compatibility.
pub fn scale(v: &Vector3, s: f64) -> Vector3 {
    vector::scale(v, s)
}

/// Patched-conic escape delta-v from a circular parking orbit.
pub fn escape_delta_v(mu_km3_s2: f64, parking_radius_km: f64, vinf_km_s: f64) -> f64 {
    let circular_speed = (mu_km3_s2 / parking_radius_km).sqrt();
    let hyperbolic_speed = (vinf_km_s * vinf_km_s + 2.0 * mu_km3_s2 / parking_radius_km).sqrt();
    (hyperbolic_speed - circular_speed).max(0.0)
}

/// Patched-conic capture delta-v for a rendezvous into a circular parking orbit.
pub fn capture_delta_v(mu_km3_s2: f64, parking_radius_km: f64, vinf_km_s: f64) -> f64 {
    let circular_speed = (mu_km3_s2 / parking_radius_km).sqrt();
    let hyperbolic_speed = (vinf_km_s * vinf_km_s + 2.0 * mu_km3_s2 / parking_radius_km).sqrt();
    (hyperbolic_speed - circular_speed).max(0.0)
}
