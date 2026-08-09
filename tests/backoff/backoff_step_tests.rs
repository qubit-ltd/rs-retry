use qubit_retry::BackoffDelaySource;

#[test]
fn delay_source_categories_are_stable() {
    assert_ne!(BackoffDelaySource::Policy, BackoffDelaySource::Hint);
}
