use std::time::Duration;

use qubit_retry::{
    AttemptCancelToken,
    AttemptTimeoutSource,
    Retry,
    RetryContext,
};

#[test]
fn test_retry_context_parts_are_observable_in_before_attempt_context() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .attempt_timeout(Some(Duration::from_millis(50)))
        .before_attempt(|context: &RetryContext| {
            assert_eq!(1, context.attempt());
            assert_eq!(1, context.max_attempts());
            assert_eq!(Some(Duration::from_millis(50)), context.attempt_timeout());
            assert_eq!(
                Some(AttemptTimeoutSource::Configured),
                context.attempt_timeout_source(),
            );
        })
        .build()
        .expect("retry should build");

    let value = retry
        .run_in_worker(|_token: AttemptCancelToken| Ok("ok"))
        .expect("worker retry should succeed");

    assert_eq!("ok", value);
}
