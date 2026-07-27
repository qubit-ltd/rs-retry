use qubit_retry::AttemptTimeoutSource;

#[test]
fn test_attempt_timeout_source_orders_and_serializes_sources() {
    assert!(AttemptTimeoutSource::Configured < AttemptTimeoutSource::MaxOperationElapsed);
    assert!(AttemptTimeoutSource::MaxOperationElapsed < AttemptTimeoutSource::MaxTotalElapsed);
    assert_eq!(
        "\"MaxTotalElapsed\"",
        serde_json::to_string(&AttemptTimeoutSource::MaxTotalElapsed).unwrap(),
    );
    assert_eq!(
        AttemptTimeoutSource::Configured,
        serde_json::from_str("\"Configured\"").unwrap(),
    );
}
