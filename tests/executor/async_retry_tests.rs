use qubit_retry::Retry;
use qubit_retry::RetryPolicy;

#[cfg(feature = "tokio")]
#[test]
fn async_facade_is_available() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).build();
    let _ = retry.asynchronous();
}
