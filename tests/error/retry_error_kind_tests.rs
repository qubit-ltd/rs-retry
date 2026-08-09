use qubit_retry::RetryErrorKind;
use qubit_retry::RetryErrorReason;

#[test]
fn test_retry_error_reason_maps_to_stable_kind() {
    assert_eq!(
        RetryErrorReason::AttemptsExhausted.kind(),
        RetryErrorKind::Exhausted
    );
    assert_eq!(
        RetryErrorReason::FlowTimedOut.kind(),
        RetryErrorKind::TimedOut
    );
    assert_eq!(
        RetryErrorReason::TimerFailed.kind(),
        RetryErrorKind::Infrastructure
    );
}
