use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::AttemptFailureKind;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorKind;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryRule;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestError;

impl std::fmt::Display for TestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("test error")
    }
}

impl std::error::Error for TestError {}

#[test]
fn policy_validates_limits_and_backoff_state() {
    assert!(RetryPolicy::builder().max_attempts(0).build().is_err());
    let policy = BackoffPolicy::exponential(
        Duration::from_millis(10),
        2.0,
        Duration::from_millis(25),
    )
    .unwrap();
    let mut state = policy.start();
    assert_eq!(state.next(BackoffRequest::policy()).retry_index(), 1);
    assert_eq!(
        state.next(BackoffRequest::policy()).effective_delay(),
        Duration::from_millis(20)
    );
    state.reset();
    assert_eq!(state.next(BackoffRequest::policy()).retry_index(), 1);
}

#[test]
fn sync_facade_retries_application_failure() {
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::immediate())
        .build()
        .unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let attempts = AtomicU32::new(0);
    let result = retry.sync().run(|| {
        if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(TestError)
        } else {
            Ok(42_u32)
        }
    });
    let success = result.expect("second attempt succeeds");
    assert_eq!(*success.value(), 42);
    assert_eq!(success.context().attempt(), 2);
}

#[test]
fn first_rule_wins_and_failure_kind_is_stable() {
    struct RetryOnly;
    impl RetryRule<TestError> for RetryOnly {
        fn decide(
            &self,
            _: &AttemptFailure<TestError>,
            _: &RetryContext,
        ) -> RetryDecision {
            RetryDecision::Retry
        }
    }
    struct AbortRule;
    impl RetryRule<TestError> for AbortRule {
        fn decide(
            &self,
            _: &AttemptFailure<TestError>,
            _: &RetryContext,
        ) -> RetryDecision {
            RetryDecision::Abort
        }
    }
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<TestError>::builder(policy)
        .rule(RetryOnly)
        .rule(AbortRule)
        .build();
    let error = retry.sync().run::<(), _>(|| Err(TestError)).unwrap_err();
    assert_eq!(error.kind(), RetryErrorKind::Exhausted);
    assert_eq!(error.reason(), RetryErrorReason::AttemptsExhausted);
    assert_eq!(
        error.last_failure().unwrap().kind(),
        AttemptFailureKind::Application
    );
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_facade_retries_and_preserves_success() {
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let attempts = Arc::new(AtomicU32::new(0));
    let result = retry
        .asynchronous()
        .run({
            let attempts = Arc::clone(&attempts);
            move || {
                let attempts = Arc::clone(&attempts);
                async move {
                    if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                        Err(TestError)
                    } else {
                        Ok(7_u32)
                    }
                }
            }
        })
        .await
        .expect("second attempt succeeds");
    assert_eq!(*result.value(), 7);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn async_attempt_timeout_has_a_distinct_terminal_reason() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let error = retry
        .asynchronous()
        .attempt_timeout(Duration::from_millis(1))
        .run(|| async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Err::<(), _>(TestError)
        })
        .await
        .unwrap_err();

    assert_eq!(error.reason(), RetryErrorReason::AttemptTimedOut);
    assert_eq!(error.kind(), RetryErrorKind::TimedOut);
}

#[test]
fn worker_facade_retries_with_cooperative_token() {
    let policy = RetryPolicy::builder().max_attempts(2).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let attempts = Arc::new(AtomicU32::new(0));
    let result = retry
        .worker()
        .run({
            let attempts = Arc::clone(&attempts);
            move |_| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err(TestError)
                } else {
                    Ok(9_u32)
                }
            }
        })
        .expect("second worker attempt succeeds");
    assert_eq!(*result.value(), 9);
}

#[test]
fn worker_attempt_timeout_has_a_distinct_terminal_reason() {
    let policy = RetryPolicy::builder().max_attempts(1).build().unwrap();
    let retry = Retry::<TestError>::builder(policy).build();
    let error = retry
        .worker()
        .attempt_timeout(Duration::from_millis(1))
        .cancellation_grace(Duration::from_millis(50))
        .run(|token| {
            while !token.is_cancelled() {
                std::thread::yield_now();
            }
            Err::<(), _>(TestError)
        })
        .unwrap_err();

    assert_eq!(error.reason(), RetryErrorReason::AttemptTimedOut);
    assert_eq!(error.kind(), RetryErrorKind::TimedOut);
}
