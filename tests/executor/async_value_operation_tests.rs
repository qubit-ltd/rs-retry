#[cfg(feature = "tokio")]
use qubit_retry::Retry;

#[cfg(feature = "tokio")]
#[tokio::test]
async fn test_async_value_operation_is_observable_through_async_success_value() {
    #[derive(Debug, PartialEq, Eq)]
    struct Token(String);

    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .unwrap();

    let value = retry.run_async(|| async { Ok(Token("captured".to_owned())) }).await.unwrap();
    assert_eq!(Token("captured".to_owned()), value);
}
