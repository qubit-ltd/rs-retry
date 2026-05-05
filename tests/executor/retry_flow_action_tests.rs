use std::time::Duration;

use qubit_retry::{
    AttemptFailureDecision,
    Retry,
    RetryErrorReason,
};

#[test]
fn test_retry_flow_action_paths_cover_retry_and_finished_results() {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(3)
        .fixed_delay(Duration::ZERO)
        .on_failure(|_failure, context| {
            if context.attempt() == 1 {
                AttemptFailureDecision::UseDefault
            } else {
                AttemptFailureDecision::Abort
            }
        })
        .build()
        .unwrap();
    let mut attempts = 0;

    let error = retry
        .run(|| -> Result<(), &'static str> {
            attempts += 1;
            Err("fail")
        })
        .unwrap_err();

    assert_eq!(2, attempts);
    assert_eq!(RetryErrorReason::Aborted, error.reason());
}
