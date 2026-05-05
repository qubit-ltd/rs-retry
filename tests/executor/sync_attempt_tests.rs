use qubit_retry::{
    AttemptFailureDecision,
    Retry,
};

#[test]
fn test_sync_attempt_failure_is_observable_through_failure_listener() {
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

    assert!(retry.run(|| -> Result<(), &'static str> { Err("boom") }).is_err());
}
