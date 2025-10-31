#[test]
fn barycenter_normalization_for_planets() {
    use solar_travel_calculator::ephemeris::normalize_heliocentric_target_name as norm;
    assert_eq!(norm("EARTH"), "EARTH BARYCENTER");
    assert_eq!(norm("MARS"), "MARS BARYCENTER");
    assert_eq!(norm("JUPITER"), "JUPITER BARYCENTER");
    assert_eq!(norm("MARS BARYCENTER"), "MARS BARYCENTER");
    // Non-planet should pass through unchanged
    assert_eq!(norm("CERES"), "CERES");
    assert_eq!(norm("MOON"), "MOON");
}
