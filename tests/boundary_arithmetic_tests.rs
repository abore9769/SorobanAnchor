#[path = "boundary_test_helpers.rs"]
mod boundary_test_helpers;

use boundary_test_helpers::BoundaryScenario;

#[test]
fn ttl_expiry_uses_exact_boundary_arithmetic() {
    let scenario = BoundaryScenario::ttl_expiry(300);

    assert_eq!(scenario.time_before_expiry(), 10_299);
    assert_eq!(scenario.time_at_expiry(), 10_300);
    assert_eq!(scenario.time_after_expiry(), 10_301);
}

#[test]
#[should_panic]
fn zero_ttl_cannot_calculate_time_before_expiry() {
    BoundaryScenario::ttl_expiry(0).time_before_expiry();
}
