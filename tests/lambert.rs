use solar_travel_calculator::impulsive::lambert;

const MU_SUN: f64 = 1.327_124_400_18e11; // km^3 / s^2
const AU_KM: f64 = 149_597_870.7; // km

#[test]
fn lambert_quarter_orbit_matches_expected_velocity() {
    let r1 = [AU_KM, 0.0, 0.0];
    let r2 = [0.0, AU_KM, 0.0];
    let tof = (std::f64::consts::PI / 2.0) * (AU_KM.powi(3) / MU_SUN).sqrt();

    let (v1, v2) = lambert::solve(r1, r2, tof, MU_SUN, true).expect("lambert solve");

    let v1_mag = (v1[0] * v1[0] + v1[1] * v1[1] + v1[2] * v1[2]).sqrt();
    let v2_mag = (v2[0] * v2[0] + v2[1] * v2[1] + v2[2] * v2[2]).sqrt();
    let expected_speed = (MU_SUN / AU_KM).sqrt();

    assert!(
        (vector_dot(&v1, &[0.0, 1.0, 0.0]) / v1_mag).abs() > 0.99,
        "expected near tangential velocity at departure: {:?}",
        v1
    );
    assert!(
        (vector_dot(&v2, &[-1.0, 0.0, 0.0]) / v2_mag).abs() > 0.99,
        "expected near tangential velocity at arrival: {:?}",
        v2
    );
    assert!((v1_mag - expected_speed).abs() < 0.5);
    assert!((v2_mag - expected_speed).abs() < 0.5);
}

fn vector_dot(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
