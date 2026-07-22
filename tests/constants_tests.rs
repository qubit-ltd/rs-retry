use std::time::Duration;

use qubit_retry::constants::{
    DEFAULT_RETRY_DELAY,
    DEFAULT_RETRY_JITTER,
    DEFAULT_RETRY_MAX_ATTEMPTS,
    DEFAULT_RETRY_MAX_OPERATION_ELAPSED,
    DEFAULT_RETRY_WORKER_CANCEL_GRACE_MILLIS,
    KEY_DELAY,
    KEY_MAX_ATTEMPTS,
};
use qubit_retry::{
    RetryDelay,
    RetryJitter,
};

#[test]
fn test_retry_constants_match_parseable_defaults_and_config_keys() {
    assert_eq!("max_attempts", KEY_MAX_ATTEMPTS);
    assert_eq!("delay", KEY_DELAY);
    assert_eq!(5, DEFAULT_RETRY_MAX_ATTEMPTS);
    assert_eq!(None, DEFAULT_RETRY_MAX_OPERATION_ELAPSED);
    assert_eq!(
        Duration::from_millis(100),
        Duration::from_millis(DEFAULT_RETRY_WORKER_CANCEL_GRACE_MILLIS)
    );
    assert!(DEFAULT_RETRY_DELAY.parse::<RetryDelay>().is_ok());
    assert!(DEFAULT_RETRY_JITTER.parse::<RetryJitter>().is_ok());
}
