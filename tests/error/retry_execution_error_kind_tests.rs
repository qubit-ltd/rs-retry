use qubit_retry::RetryExecutionErrorKind;

#[test]
fn timer_is_a_stable_infrastructure_kind() {
    assert_eq!(RetryExecutionErrorKind::Timer, RetryExecutionErrorKind::Timer);
}
