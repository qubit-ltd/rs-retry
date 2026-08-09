use qubit_retry::RetryExecutionError;
use qubit_retry::RetryExecutionErrorKind;

#[test]
fn stores_infrastructure_category() {
    let error = RetryExecutionError::timer("clock");
    assert_eq!(error.kind(), RetryExecutionErrorKind::Timer);
    assert_eq!(error.message(), "clock");
}
