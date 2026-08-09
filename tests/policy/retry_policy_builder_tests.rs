use qubit_retry::BackoffPolicy;
use qubit_retry::RetryPolicy;

#[test]
fn builder_accepts_backoff_policy() {
    let delay = std::time::Duration::from_millis(1);
    let policy = RetryPolicy::builder()
        .backoff(BackoffPolicy::fixed(delay))
        .build()
        .unwrap();
    assert_eq!(policy.backoff().maximum_delay(), Some(delay));
}
