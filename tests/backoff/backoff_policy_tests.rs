use std::time::Duration;

use qubit_retry::BackoffPolicy;

#[test]
fn test_exponential_rejects_invalid_values() {
    assert!(
        BackoffPolicy::exponential(Duration::from_millis(10), f64::NAN, Duration::from_secs(1),)
            .is_err()
    );
    assert!(
        BackoffPolicy::exponential(Duration::from_secs(2), 2.0, Duration::from_secs(1),).is_err()
    );
}

#[test]
fn test_uniform_rejects_reversed_bounds() {
    assert!(BackoffPolicy::uniform(Duration::from_secs(2), Duration::from_secs(1),).is_err());
}
