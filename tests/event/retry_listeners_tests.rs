use std::sync::{
    Arc,
    Mutex,
};

use qubit_retry::{
    AttemptFailure,
    AttemptFailureDecision,
    Retry,
    RetryContext,
    RetryError,
};

#[test]
fn test_retry_listeners_default_collection_is_populated_by_builder_callbacks() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let before = Arc::clone(&events);
    let failure = Arc::clone(&events);
    let scheduled = Arc::clone(&events);
    let error = Arc::clone(&events);

    let retry = Retry::<&'static str>::builder()
        .max_attempts(2)
        .no_delay()
        .before_attempt(move |context: &RetryContext| {
            before
                .lock()
                .expect("before events should be lockable")
                .push(format!("before:{}", context.attempt()));
        })
        .on_failure(
            move |_failure: &AttemptFailure<&'static str>, context: &RetryContext| {
                failure
                    .lock()
                    .expect("failure events should be lockable")
                    .push(format!("failure:{}", context.attempt()));
                AttemptFailureDecision::UseDefault
            },
        )
        .on_retry(
            move |_failure: &AttemptFailure<&'static str>, context: &RetryContext| {
                scheduled
                    .lock()
                    .expect("retry events should be lockable")
                    .push(format!("retry:{}", context.attempt()));
            },
        )
        .on_error(
            move |_error: &RetryError<&'static str>, context: &RetryContext| {
                error
                    .lock()
                    .expect("error events should be lockable")
                    .push(format!("error:{}", context.attempt()));
            },
        )
        .build()
        .expect("retry should build");

    let result = retry.run(|| -> Result<(), &'static str> { Err("fail") });
    assert!(result.is_err());
    assert_eq!(
        vec![
            "before:1",
            "failure:1",
            "retry:1",
            "before:2",
            "failure:2",
            "error:2",
        ],
        *events.lock().expect("events should be lockable"),
    );
}
