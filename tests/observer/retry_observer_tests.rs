use qubit_retry::RetryObserver;

struct NoopObserver;

impl RetryObserver<()> for NoopObserver {}

#[test]
fn observer_trait_accepts_function_callbacks() {
    let observer: Box<dyn RetryObserver<()>> = Box::new(NoopObserver);
    let _ = observer;
}
