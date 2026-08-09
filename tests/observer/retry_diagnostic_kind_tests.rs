use qubit_retry::RetryDiagnosticKind;

#[test]
fn observer_is_a_stable_diagnostic_kind() {
    assert_eq!(
        RetryDiagnosticKind::ObserverPanicked,
        RetryDiagnosticKind::ObserverPanicked
    );
}
