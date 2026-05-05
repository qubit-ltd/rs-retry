use std::time::Duration;

use qubit_retry::{
    Retry,
    RetryErrorReason,
};

#[test]
fn test_blocking_attempt_message_paths_are_observable_through_blocking_timeout_and_success() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .attempt_timeout(Duration::from_millis(20))
        .worker_cancel_grace(Duration::from_millis(20))
        .build()
        .unwrap();

    assert_eq!("ok", retry.run_blocking(|| Ok("ok")).unwrap());

    let error = retry
        .run_blocking(|| {
            std::thread::sleep(Duration::from_millis(100));
            Err("late")
        })
        .unwrap_err();
    assert!(matches!(
        error.reason(),
        RetryErrorReason::MaxOperationElapsedExceeded | RetryErrorReason::MaxTotalElapsedExceeded | RetryErrorReason::AttemptsExceeded | RetryErrorReason::WorkerStillRunning,
    ));
}
