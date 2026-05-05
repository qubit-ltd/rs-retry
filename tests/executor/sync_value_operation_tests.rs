use qubit_retry::Retry;

#[test]
fn test_sync_value_operation_is_observable_through_non_clone_success_value() {
    #[derive(Debug, PartialEq, Eq)]
    struct Token(String);

    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .unwrap();

    let value = retry.run(|| Ok(Token("captured".to_owned()))).unwrap();
    assert_eq!(Token("captured".to_owned()), value);
}
