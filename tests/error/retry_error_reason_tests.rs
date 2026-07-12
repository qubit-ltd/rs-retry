use qubit_retry::RetryErrorReason;

#[test]
fn test_retry_error_reason_serializes_and_compares_terminal_reasons() {
    let reasons = [
        RetryErrorReason::Aborted,
        RetryErrorReason::AttemptsExceeded,
        RetryErrorReason::MaxOperationElapsedExceeded,
        RetryErrorReason::MaxTotalElapsedExceeded,
        RetryErrorReason::UnsupportedOperation,
        RetryErrorReason::SleeperFailed,
        RetryErrorReason::WorkerStillRunning,
    ];

    assert_eq!(7, reasons.len());
    assert_eq!(
        "\"AttemptsExceeded\"",
        serde_json::to_string(&RetryErrorReason::AttemptsExceeded).unwrap(),
    );
    assert_eq!(
        RetryErrorReason::WorkerStillRunning,
        serde_json::from_str("\"WorkerStillRunning\"").unwrap(),
    );
}
