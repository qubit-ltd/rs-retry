use qubit_retry::RetryDecision;

#[test]
fn default_decision_is_use_default() {
    assert_eq!(RetryDecision::default(), RetryDecision::UseDefault);
}
