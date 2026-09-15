// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0 (the "License");
//    you may not use this file except in compliance with the License.
//    You may obtain a copy of the License at
//
//        http://www.apache.org/licenses/LICENSE-2.0
//
//    Unless required by applicable law or agreed to in writing, software
//    distributed under the License is distributed on an "AS IS" BASIS,
//    WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//    See the License for the specific language governing permissions and
//    limitations under the License.
// =============================================================================
//! Public behavior tests for the runtime-independent async retry facade.

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;

use qubit_clock::test_util::FaultInjectingTimer;
use qubit_clock::test_util::TimerFailurePoint;
use qubit_retry::AsyncRetry;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryConfig;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryTimeoutScope;

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

#[test]
fn test_async_retry_succeeds_without_tokio() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .build()
        .expect("config should be valid");
    let result = block_on(
        AsyncRetry::new(&config).run(|| std::future::ready(Ok::<_, &str>(7))),
    );

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
            std::future::ready(if attempt == 0 {
                Err("temporary")
            } else {
                Ok(9)
            })
        }
    }));

    assert_eq!(*result.expect("second attempt should succeed").value(), 9);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
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

    assert!(matches!(
        error.reason(),
        RetryErrorReason::Infrastructure { .. }
    ));
    assert_eq!(started.load(Ordering::SeqCst), 0);
}

#[test]
fn test_async_retry_cancellation_interrupts_pending_operation() {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .build()
        .expect("config should be valid");
    let token = RetryCancellationToken::new();
    let error = block_on(
        AsyncRetry::new(&config)
            .cancellation_token(token.clone())
            .run(move || {
                token.cancel();
                std::future::pending::<Result<(), &str>>()
            }),
    )
    .expect_err("cancelled operation should terminate the attempt");

    assert!(matches!(
        error.reason(),
        RetryErrorReason::Cancelled {
            phase: RetryCancellationPhase::Attempt,
        }
    ));
}
