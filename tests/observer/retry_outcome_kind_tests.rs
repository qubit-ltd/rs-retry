use qubit_retry::RetryOutcomeKind;

#[test]
fn outcome_categories_are_distinct() {
    assert_ne!(RetryOutcomeKind::Succeeded, RetryOutcomeKind::Failed);
}
