use qubit_retry::{
    Retry,
    RetryErrorReason,
};

#[test]
fn test_retry_run_returns_value_and_exhaustion_error() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(2)
        .no_delay()
        .build()
        .unwrap();
    let mut attempts = 0;

    let value = retry
        .run(|| {
            attempts += 1;
            if attempts == 2 { Ok("done") } else { Err("again") }
        })
        .unwrap();
    assert_eq!("done", value);

    let error = retry.run(|| -> Result<(), &'static str> { Err("always") }).unwrap_err();
    assert_eq!(RetryErrorReason::AttemptsExceeded, error.reason());
}
