use qubit_retry::BackoffDelaySource;

#[test]
fn policy_is_a_stable_delay_source() {
    assert_eq!(BackoffDelaySource::Policy, BackoffDelaySource::Policy);
}
