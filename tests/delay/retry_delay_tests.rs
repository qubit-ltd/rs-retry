/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::time::Duration;

use qubit_retry::RetryDelay;

/// Verifies every delay variant calculates the expected base delay.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when a delay calculation is incorrect.
#[test]
fn test_base_delay_none_fixed_random_and_exponential_values() {
    assert_eq!(RetryDelay::none().base_delay(1), Duration::ZERO);
    assert_eq!(
        RetryDelay::fixed(Duration::from_millis(12)).base_delay(9),
        Duration::from_millis(12)
    );
    assert_eq!(
        RetryDelay::random(Duration::from_millis(7), Duration::from_millis(7)).base_delay(1),
        Duration::from_millis(7)
    );

    let random = RetryDelay::random(Duration::from_millis(5), Duration::from_millis(8));
    for _ in 0..20 {
        let delay = random.base_delay(1);
        assert!(delay >= Duration::from_millis(5));
        assert!(delay <= Duration::from_millis(8));
    }

    let exponential =
        RetryDelay::exponential(Duration::from_millis(100), Duration::from_millis(500), 2.0);
    assert_eq!(exponential.base_delay(0), Duration::from_millis(100));
    assert_eq!(exponential.base_delay(1), Duration::from_millis(100));
    assert_eq!(exponential.base_delay(2), Duration::from_millis(200));
    assert_eq!(exponential.base_delay(4), Duration::from_millis(500));
    assert_eq!(exponential.base_delay(u32::MAX), Duration::from_millis(500));
}

/// Verifies delay validation rejects invalid strategy parameters.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when invalid values are accepted or valid
/// values are rejected.
#[test]
fn test_validate_rejects_invalid_values() {
    assert!(RetryDelay::fixed(Duration::ZERO).validate().is_err());
    assert!(RetryDelay::random(Duration::ZERO, Duration::from_millis(1))
        .validate()
        .is_err());
    assert!(
        RetryDelay::random(Duration::from_millis(2), Duration::from_millis(1))
            .validate()
            .is_err()
    );
    assert!(
        RetryDelay::random(Duration::from_millis(2), Duration::from_millis(2))
            .validate()
            .is_ok()
    );
    assert!(
        RetryDelay::exponential(Duration::ZERO, Duration::from_secs(1), 2.0)
            .validate()
            .is_err()
    );
    assert!(
        RetryDelay::exponential(Duration::from_secs(2), Duration::from_secs(1), 2.0)
            .validate()
            .is_err()
    );
    assert!(
        RetryDelay::exponential(Duration::from_secs(1), Duration::from_secs(2), 1.0)
            .validate()
            .is_err()
    );
    assert!(RetryDelay::exponential(
        Duration::from_secs(1),
        Duration::from_secs(2),
        f64::INFINITY
    )
    .validate()
    .is_err());
    assert!(RetryDelay::default().validate().is_ok());
}
