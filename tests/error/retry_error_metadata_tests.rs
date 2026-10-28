// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

//! Metadata projections from terminal retry errors are covered here.

use qubit_retry::AttemptFailureMetadata;
use qubit_retry::Retry;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryObserver;

/// Emits a completion diagnostic after the terminal error is frozen.
struct MetadataAudit;

impl RetryObserver<&'static str> for MetadataAudit {
    fn on_terminal_failure(&self, _: &RetryErrorReason, _: &RetryContext) {
        panic!("metadata audit unavailable");
    }
}

#[test]
fn test_metadata_before_attempt_cancellation_has_no_retained_failure() {
    let config = RetryConfig::<&str>::builder().build().expect("valid config");
    let token = RetryCancellationToken::new();
    token.cancel();
    let error = Retry::new(&config)
        .cancellation_token(token)
        .run(|| -> Result<(), &str> { panic!("operation must not start") })
        .expect_err("pre-cancelled flow");
    let (metadata, application_error) = error.into_metadata_and_error();
    assert_eq!(metadata.context().attempts(), 0);
    assert!(metadata.last_attempt().is_none());
    assert!(application_error.is_none());
}

#[test]
fn test_metadata_split_preserves_completion_diagnostics() {
    let config = RetryConfig::<&str>::builder()
        .observer(MetadataAudit)
        .build()
        .expect("valid config");
    let error = Retry::new(&config)
        .run(|| Err::<(), _>("offline"))
        .expect_err("unclassified error aborts");
    let (metadata, application_error) = error.into_metadata_and_error();
    assert!(matches!(metadata.reason(), RetryErrorReason::Aborted));
    assert!(matches!(
        metadata.last_attempt(),
        Some(AttemptFailureMetadata::ApplicationError)
    ));
    assert_eq!(metadata.context().attempts(), 1);
    assert_eq!(application_error, Some("offline"));
    assert_eq!(metadata.completion_callback_failures().len(), 1);
    assert_eq!(
        metadata.completion_callback_failures()[0].phase(),
        RetryCallbackPhase::TerminalFailure
    );
}

#[cfg(feature = "tokio")]
mod asynchronous {
    use std::future;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use qubit_clock::test_util::FaultInjectingTimer;
    use qubit_clock::test_util::TimerFailurePoint;
    use qubit_retry::AttemptFailureMetadata;
    use qubit_retry::RetryCancellationPhase;
    use qubit_retry::RetryCancellationToken;
    use qubit_retry::RetryConfig;
    use qubit_retry::RetryErrorReason;
    use qubit_retry::RetryFallback;
    use qubit_retry::RetryInfrastructureFailure;
    use qubit_retry::TokioRetry;

    #[tokio::test(flavor = "current_thread")]
    async fn test_metadata_first_active_cancellation_has_no_retained_failure() {
        let config = RetryConfig::<&str>::builder().build().expect("valid config");
        let token = RetryCancellationToken::new();
        let operation_token = token.clone();
        let error = TokioRetry::new(&config)
            .cancellation_token(token)
            .run(move || {
                operation_token.cancel();
                future::pending::<Result<(), &str>>()
            })
            .await
            .expect_err("active cancellation");
        let (metadata, application_error) = error.into_metadata_and_error();
        assert_eq!(metadata.context().attempts(), 1);
        assert!(matches!(
            metadata.reason(),
            RetryErrorReason::Cancelled {
                phase: RetryCancellationPhase::Attempt,
            }
        ));
        assert!(metadata.last_attempt().is_none());
        assert!(application_error.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_metadata_active_cancellation_preserves_previous_failure() {
        let config = RetryConfig::<&str>::builder()
            .max_attempts(3)
            .fallback(RetryFallback::Retry)
            .build()
            .expect("valid config");
        let token = RetryCancellationToken::new();
        let calls = AtomicUsize::new(0);
        let error = TokioRetry::new(&config)
            .cancellation_token(token.clone())
            .run(|| {
                let first = calls.fetch_add(1, Ordering::SeqCst) == 0;
                let operation_token = token.clone();
                async move {
                    if first {
                        Err("first failure")
                    } else {
                        operation_token.cancel();
                        future::pending::<Result<(), &str>>().await
                    }
                }
            })
            .await
            .expect_err("second attempt cancelled");
        let (metadata, application_error) = error.into_metadata_and_error();
        assert_eq!(metadata.context().attempts(), 2);
        assert!(matches!(
            metadata.reason(),
            RetryErrorReason::Cancelled {
                phase: RetryCancellationPhase::Attempt,
            }
        ));
        assert!(matches!(
            metadata.last_attempt(),
            Some(AttemptFailureMetadata::ApplicationError)
        ));
        assert_eq!(application_error, Some("first failure"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_metadata_timer_registration_failure_has_no_attempt() {
        let config = RetryConfig::<&str>::builder().build().expect("valid config");
        let timer = Arc::new(FaultInjectingTimer::backend_unavailable(
            TimerFailurePoint::Registration,
            "test",
            "timer unavailable",
        ));
        let error = TokioRetry::new(&config)
            .timer(timer)
            .hard_attempt_timeout(Duration::from_secs(1))
            .run(|| {
                future::poll_fn(|_| -> std::task::Poll<Result<(), &str>> {
                    panic!("operation must not be admitted");
                })
            })
            .await
            .expect_err("timer registration failure");
        let (metadata, application_error) = error.into_metadata_and_error();
        assert_eq!(metadata.context().attempts(), 0);
        assert!(matches!(
            metadata.reason(),
            RetryErrorReason::Infrastructure {
                failure: RetryInfrastructureFailure::Timer { .. },
            }
        ));
        assert!(metadata.last_attempt().is_none());
        assert!(application_error.is_none());
    }
}
