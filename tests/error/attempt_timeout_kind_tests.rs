use qubit_retry::AttemptTimeoutKind;

#[test]
fn exposes_timeout_boundaries() {
    assert_ne!(AttemptTimeoutKind::Attempt, AttemptTimeoutKind::Flow);
}
