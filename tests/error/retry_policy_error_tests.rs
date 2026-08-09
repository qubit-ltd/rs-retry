use qubit_retry::RetryPolicy;

#[test]
fn rejects_zero_attempts() {
    assert!(RetryPolicy::builder().max_attempts(0).build().is_err());
}
