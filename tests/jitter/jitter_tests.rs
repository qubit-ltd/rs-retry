/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::Jitter;

/// Verifies factor jitter application and validation bounds.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when jitter output or validation behavior
/// is incorrect.
#[test]
fn test_apply_symmetric_factor_and_validate_bounds() {
    let base = Duration::from_millis(100);
    assert_eq!(Jitter::none().apply(base), base);
    assert_eq!(Jitter::factor(0.0).apply(base), base);
    assert_eq!(Jitter::factor(0.5).apply(Duration::ZERO), Duration::ZERO);
    assert_eq!(Jitter::default(), Jitter::None);

    for _ in 0..30 {
        let delay = Jitter::factor(0.2).apply(base);
        assert!(delay >= Duration::from_millis(80));
        assert!(delay <= Duration::from_millis(120));
    }

    assert!(Jitter::factor(0.0).validate().is_ok());
    assert!(Jitter::factor(1.0).validate().is_ok());
    assert!(Jitter::factor(-0.1).validate().is_err());
    assert!(Jitter::factor(1.1).validate().is_err());
    assert!(Jitter::factor(f64::NAN).validate().is_err());
}
