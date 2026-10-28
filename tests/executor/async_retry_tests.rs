// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Public behavior tests for the runtime-independent async retry facade.

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::task::Context;
use std::task::Poll;
use std::task::Wake;
use std::task::Waker;
use std::time::Duration;
use std::time::Instant;

use qubit_clock::test_util::FaultInjectingTimer;
use qubit_clock::test_util::TimerFailurePoint;
use qubit_retry::AsyncRetry;
use qubit_retry::BackoffPolicy;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryConfig;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryTimeoutScope;

#[derive(Default)]
struct WakeCounter(AtomicUsize);

impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct WakeSignal(mpsc::Sender<()>);

impl Wake for WakeSignal {
    fn wake(self: Arc<Self>) {
        let _ = self.0.send(());
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(());
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    block_on_with_timeout(future, Duration::from_secs(5))
}

fn block_on_with_timeout<F: Future>(future: F, watchdog: Duration) -> F::Output {
    let deadline = Instant::now() + watchdog;
    let (sender, receiver) = mpsc::channel();
    let waker = Waker::from(Arc::new(WakeSignal(sender)));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(!remaining.is_zero(), "async test future exceeded its watchdog");
                match receiver.recv_timeout(remaining) {
                    Ok(()) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        panic!("async test future exceeded its watchdog");
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        panic!("async test wake signal disconnected");
                    }
                }
            }
        }
    }
}

#[test]
fn test_block_on_repolls_after_wake() {
    let polls = Arc::new(AtomicUsize::new(0));
    let result = block_on_with_timeout(
        std::future::poll_fn({
            let polls = Arc::clone(&polls);
            move |context| {
                if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                    context.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(17)
                }
            }
        }),
        Duration::from_secs(1),
    );

    assert_eq!(result, 17);
    assert_eq!(polls.load(Ordering::SeqCst), 2);
}

#[test]
fn test_block_on_does_not_repoll_without_wake() {
    let polls = Arc::new(AtomicUsize::new(0));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe({
        let polls = Arc::clone(&polls);
        move || {
            block_on_with_timeout(
                std::future::poll_fn(move |_| {
                    polls.fetch_add(1, Ordering::SeqCst);
                    Poll::<()>::Pending
                }),
                Duration::from_millis(20),
            )
        }
    }));

    assert!(result.is_err(), "a future without a wake should hit the watchdog");
    assert_eq!(polls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_async_retry_succeeds_without_tokio() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .build()
        .expect("config should be valid");
    let result = block_on(AsyncRetry::new(&config).run(|| std::future::ready(Ok::<_, &str>(7))));

    let success = result.expect("async retry should succeed");
    assert_eq!(*success.value(), 7);
}

#[test]
fn test_async_retry_retries_application_failure_without_tokio() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("config should be valid");
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = block_on(AsyncRetry::new(&config).run({
        let attempts = Arc::clone(&attempts);
        move || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            std::future::ready(if attempt == 0 { Err("temporary") } else { Ok(9) })
        }
    }));

    assert_eq!(*result.expect("second attempt should succeed").value(), 9);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[test]
fn test_async_retry_nonzero_backoff_wakes_executor() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .fallback(RetryFallback::Retry)
        .backoff(BackoffPolicy::fixed(Duration::from_millis(10)))
        .build()
        .expect("config should be valid");
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = block_on(AsyncRetry::new(&config).run({
        let attempts = Arc::clone(&attempts);
        move || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            std::future::ready(if attempt == 0 { Err("temporary") } else { Ok(9) })
        }
    }));

    assert_eq!(*result.expect("second attempt should succeed").value(), 9);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[test]
fn test_async_retry_cross_thread_cancel_wakes_executor() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .build()
        .expect("config should be valid");
    let token = RetryCancellationToken::new();
    let (started_sender, started_receiver) = mpsc::channel();
    let cancel_token = token.clone();
    let cancellation_thread = std::thread::spawn(move || {
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("operation should signal its first poll");
        cancel_token.cancel();
    });
    let attempts = Arc::new(AtomicUsize::new(0));
    let result = block_on(AsyncRetry::new(&config).cancellation_token(token).run({
        let attempts = Arc::clone(&attempts);
        let mut started_sender = Some(started_sender);
        move || {
            attempts.fetch_add(1, Ordering::SeqCst);
            let mut started_sender = started_sender.take();
            std::future::poll_fn(move |_| {
                if let Some(sender) = started_sender.take() {
                    sender.send(()).expect("cancellation thread should be waiting");
                }
                Poll::<Result<(), &str>>::Pending
            })
        }
    }))
    .expect_err("cross-thread cancellation should terminate the attempt");

    cancellation_thread.join().expect("cancellation thread should finish");
    assert!(matches!(
        result.reason(),
        RetryErrorReason::Cancelled {
            phase: RetryCancellationPhase::Attempt,
        }
    ));
    assert_eq!(result.context().attempts(), 1);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn test_async_retry_attempt_timeout_uses_std_timer() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(1)
        .fallback(RetryFallback::Retry)
        .build()
        .expect("config should be valid");
    let error = block_on(
        AsyncRetry::new(&config)
            .hard_attempt_timeout(Duration::from_millis(1))
            .run(std::future::pending::<Result<(), &str>>),
    )
    .expect_err("pending operation should time out");

    assert!(matches!(
        error.reason(),
        RetryErrorReason::TimedOut {
            scope: RetryTimeoutScope::Attempt,
        }
    ));
}

#[test]
fn test_async_retry_timer_injection_reports_registration_failure() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(1)
        .build()
        .expect("config should be valid");
    let timer = Arc::new(FaultInjectingTimer::backend_unavailable(
        TimerFailurePoint::Registration,
        "test",
        "timer unavailable",
    ));
    let started = Arc::new(AtomicUsize::new(0));
    let error = block_on(
        AsyncRetry::new(&config)
            .hard_attempt_timeout(Duration::from_secs(1))
            .timer(timer)
            .run({
                let started = Arc::clone(&started);
                move || {
                    started.fetch_add(1, Ordering::SeqCst);
                    std::future::ready(Ok::<(), &str>(()))
                }
            }),
    )
    .expect_err("timer registration should fail before the operation");

    assert!(matches!(error.reason(), RetryErrorReason::Infrastructure { .. }));
    assert_eq!(started.load(Ordering::SeqCst), 0);
}

#[test]
fn test_async_retry_cancellation_interrupts_pending_operation() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .build()
        .expect("config should be valid");
    let token = RetryCancellationToken::new();
    let error = block_on(AsyncRetry::new(&config).cancellation_token(token.clone()).run(move || {
        token.cancel();
        std::future::pending::<Result<(), &str>>()
    }))
    .expect_err("cancelled operation should terminate the attempt");

    assert!(matches!(
        error.reason(),
        RetryErrorReason::Cancelled {
            phase: RetryCancellationPhase::Attempt,
        }
    ));
}

#[test]
fn test_async_immediate_retry_yields_and_wakes_before_next_attempt() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .fallback(RetryFallback::Retry)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("config should be valid");
    let attempts = Arc::new(AtomicUsize::new(0));
    let execution = AsyncRetry::new(&config);
    let mut future = pin!(execution.run({
        let attempts = Arc::clone(&attempts);
        move || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            std::future::ready(if attempt == 0 { Err("offline") } else { Ok(7) })
        }
    }));
    let wake_counter = Arc::new(WakeCounter::default());
    let waker = Waker::from(Arc::clone(&wake_counter));
    let mut context = Context::from_waker(&waker);

    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(wake_counter.0.load(Ordering::SeqCst), 1);
    let Poll::Ready(result) = future.as_mut().poll(&mut context) else {
        panic!("the second immediate poll should complete the retry");
    };
    assert_eq!(*result.expect("second attempt should succeed").value(), 7);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[test]
fn test_async_immediate_retry_cancellation_is_attributed_to_backoff() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(3)
        .fallback(RetryFallback::Retry)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("config should be valid");
    let token = RetryCancellationToken::new();
    let attempts = Arc::new(AtomicUsize::new(0));
    let execution = AsyncRetry::new(&config).cancellation_token(token.clone());
    let mut future = pin!(execution.run({
        let attempts = Arc::clone(&attempts);
        move || {
            attempts.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Err::<(), _>("offline"))
        }
    }));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    token.cancel();
    let Poll::Ready(result) = future.as_mut().poll(&mut context) else {
        panic!("cancellation during the immediate backoff should finish");
    };
    let error = result.expect_err("the cancelled retry should fail");
    assert!(matches!(
        error.reason(),
        RetryErrorReason::Cancelled {
            phase: RetryCancellationPhase::Backoff,
        }
    ));
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.last_error(), Some(&"offline"));
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
