use qubit_retry::RetryPolicy;

#[test]
fn policy_exposes_retry_limits() {
    let policy = RetryPolicy::builder().max_attempts(4).build().unwrap();
    assert_eq!(policy.limits().max_attempts().get(), 4);
}
