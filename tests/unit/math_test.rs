#[test] fn clamp01_bounds() {
    assert_eq!(crate::core::math::clamp01(-1.0), 0.0);
    assert_eq!(crate::core::math::clamp01(2.0), 1.0);
}
