use qubit_retry::AttemptExecutionError;

#[test]
fn stores_message() {
    assert_eq!(AttemptExecutionError::new("failed").message(), "failed");
}
