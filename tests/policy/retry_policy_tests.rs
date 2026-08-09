use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::RetryPolicy;

#[test]
fn test_retry_policy_rejects_zero_attempts() {
    assert!(RetryPolicy::builder().max_attempts(0).build().is_err());
}

#[test]
fn test_retry_policy_accepts_limits_and_backoff() {
    let backoff = BackoffPolicy::fixed(Duration::from_millis(10));
    let policy = RetryPolicy::builder()
        .max_attempts(4)
        .max_operation_elapsed(Duration::from_secs(2))
        .max_total_elapsed(Duration::from_secs(5))
        .backoff(backoff)
        .build()
        .expect("valid policy should build");
    assert_eq!(policy.limits().max_attempts().get(), 4);
    assert_eq!(
        policy.limits().max_operation_elapsed(),
        Some(Duration::from_secs(2))
    );
}
