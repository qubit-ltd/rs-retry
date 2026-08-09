use qubit_retry::RetryDiagnosticKind;

#[test]
fn exposes_callback_diagnostic_categories() {
    assert_ne!(
        RetryDiagnosticKind::RulePanicked,
        RetryDiagnosticKind::ObserverPanicked
    );
}
