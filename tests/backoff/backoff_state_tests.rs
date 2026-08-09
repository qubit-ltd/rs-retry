use std::sync::Arc;
use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::RetryRandomSource;

struct FixedRandom;

impl RetryRandomSource for FixedRandom {
    fn random_u64_inclusive(&self, min: u64, _max: u64) -> u64 {
        min
    }

    fn random_f64_inclusive(&self, min: f64, _max: f64) -> f64 {
        min
    }
}

#[test]
fn test_backoff_state_advances_and_resets() {
    let policy =
        BackoffPolicy::exponential(Duration::from_millis(10), 2.0, Duration::from_millis(25))
            .expect("valid exponential policy");
    let mut state = policy.start_with_random_source(Arc::new(FixedRandom));
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(10)
    );
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(20)
    );
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(25)
    );
    state.reset();
    assert_eq!(state.retry_index(), 0);
    assert_eq!(state.next(BackoffRequest::policy()).retry_index(), 1);
}

#[test]
fn test_retry_after_hint_obeys_policy() {
    let policy = BackoffPolicy::fixed(Duration::from_secs(1));
    let mut state = policy.start_with_random_source(Arc::new(FixedRandom));
    let step = state.next(BackoffRequest::hint(Duration::from_secs(3)));
    assert_eq!(step.effective_delay(), Duration::from_secs(3));
}
