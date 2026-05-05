use std::sync::{
    Arc,
    Mutex,
};

use qubit_retry::{
    AttemptFailureDecision,
    Retry,
    RetryContext,
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
        .before_attempt(move |context: &RetryContext| before.lock().unwrap().push(format!("before:{}", context.attempt())))
        .on_failure(move |_failure, context| {
            failure.lock().unwrap().push(format!("failure:{}", context.attempt()));
            AttemptFailureDecision::UseDefault
        })
        .on_retry_scheduled(move |_failure, context, _delay| scheduled.lock().unwrap().push(format!("retry:{}", context.attempt())))
        .on_error(move |_error, context| error.lock().unwrap().push(format!("error:{}", context.attempt())))
        .build()
        .unwrap();

    let result = retry.run(|| -> Result<(), &'static str> { Err("fail") });
    assert!(result.is_err());
    assert_eq!(
        vec!["before:1", "failure:1", "retry:1", "before:2", "failure:2", "error:2"],
        *events.lock().unwrap(),
    );
}
