use qubit_retry::Retry;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

struct NoopObserver;

impl RetryObserver<()> for NoopObserver {}

#[test]
fn observers_are_registered_by_retry_builder() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).observer(NoopObserver).build();
    let _ = retry;
}
