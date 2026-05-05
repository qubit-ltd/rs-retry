#[cfg(feature = "tokio")]
use qubit_retry::{
    AttemptFailureDecision,
    Retry,
};

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_async_attempt_failure_is_observable_through_failure_listener() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .on_failure(|failure, context| {
            assert_eq!(1, context.attempt());
            assert_eq!(Some(&"boom"), failure.as_error());
            AttemptFailureDecision::Abort
        })
        .build()
        .unwrap();

    assert!(retry.run_async(|| async { Err::<(), _>("boom") }).await.is_err());
}
