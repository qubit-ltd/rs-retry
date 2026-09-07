use qubit_retry::AttemptCancellationToken;
use qubit_retry::Retry;
use qubit_retry::RetryPolicy;

fn main() {
    let _ = AttemptCancellationToken::new();
    let retry = Retry::<()>::builder(RetryPolicy::builder().build().unwrap()).build();
    let _ = retry.worker();
}
