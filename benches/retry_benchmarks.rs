// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Benchmarks for representative retry execution and policy paths.

use std::time::Duration;

use criterion::BatchSize;
use criterion::Criterion;
use criterion::black_box;
use criterion::criterion_group;
use criterion::criterion_main;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::Retry;
use qubit_retry::WorkerRetry;
use qubit_retry::TokioRetry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

/// No-op lifecycle observer used to isolate observer dispatch overhead.
struct NoopObserver;

impl RetryObserver<&'static str> for NoopObserver {
    fn on_success(&self, _context: &RetryContext) {}

    fn on_terminal_failure(&self, _reason: &RetryErrorReason, _context: &RetryContext) {}
}

/// No-op failure listener used to measure listener dispatch overhead.
fn observe_failure(_failure: &AttemptFailure<&'static str>, _context: &RetryContext) {}

/// Continues rule-chain dispatch without selecting a terminal decision.
fn use_default_rule(_failure: &AttemptFailure<&'static str>, _context: &RetryContext) -> RetryDecision {
    RetryDecision::UseDefault
}

/// Terminates rule-chain dispatch after preceding default decisions.
fn abort_rule(_failure: &AttemptFailure<&'static str>, _context: &RetryContext) -> RetryDecision {
    RetryDecision::Abort
}

/// Measures the lowest-overhead successful synchronous execution path.
fn benchmark_sync_success(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry = RetryConfig::<&'static str>::builder().policy(policy).build().expect("valid config");

    c.bench_function("sync_success", |b| {
        b.iter(|| {
            let result = Retry::new(&retry).run(|| Ok::<u64, &'static str>(black_box(42)));
            let _ = black_box(result);
        });
    });

    let facade = Retry::new(&retry);
    c.bench_function("sync_success_reused_facade", |b| {
        b.iter(|| {
            let result = facade.run(|| Ok::<u64, &'static str>(black_box(42)));
            let _ = black_box(result);
        });
    });
}

/// Measures successful sync execution with and without a live cancellation
/// token.
fn benchmark_sync_cancellation_token(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry = RetryConfig::<&'static str>::builder().policy(policy).build().expect("valid config");
    let without_token = Retry::new(&retry);
    let with_token = Retry::new(&retry).cancellation_token(RetryCancellationToken::new());

    c.bench_function("sync_success_without_token", |b| {
        b.iter(|| {
            let result = without_token.run(|| Ok::<u64, &'static str>(black_box(42)));
            let _ = black_box(result);
        });
    });
    c.bench_function("sync_success_with_token", |b| {
        b.iter(|| {
            let result = with_token.run(|| Ok::<u64, &'static str>(black_box(42)));
            let _ = black_box(result);
        });
    });
}

/// Measures lifecycle observer dispatch for zero, one, and four observers.
fn benchmark_observer_counts(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry_0 = RetryConfig::<&'static str>::builder().policy(policy.clone()).build().expect("valid config");
    let retry_1 = RetryConfig::<&'static str>::builder().policy(policy.clone())
        .observer(NoopObserver)
        .build().expect("valid config");
    let retry_4 = RetryConfig::<&'static str>::builder().policy(policy)
        .observer(NoopObserver)
        .observer(NoopObserver)
        .observer(NoopObserver)
        .observer(NoopObserver)
        .build().expect("valid config");

    for (name, retry) in [
        ("observer_count/0", retry_0),
        ("observer_count/1", retry_1),
        ("observer_count/4", retry_4),
    ] {
        let facade = Retry::new(&retry);
        c.bench_function(name, |b| {
            b.iter(|| {
                let result = facade.run(|| Ok::<u64, &'static str>(black_box(42)));
                let _ = black_box(result);
            });
        });
    }
}

/// Measures rule dispatch for zero, one, and four defaulting rules.
fn benchmark_rule_counts(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry_0 = RetryConfig::<&'static str>::builder().policy(policy.clone()).build().expect("valid config");
    let retry_1 = RetryConfig::<&'static str>::builder().policy(policy.clone())
        .rule(use_default_rule)
        .build().expect("valid config");
    let retry_4 = RetryConfig::<&'static str>::builder().policy(policy)
        .rule(use_default_rule)
        .rule(use_default_rule)
        .rule(use_default_rule)
        .rule(use_default_rule)
        .build().expect("valid config");

    for (name, retry) in [
        ("rule_count/0", retry_0),
        ("rule_count/1", retry_1),
        ("rule_count/4", retry_4),
    ] {
        let facade = Retry::new(&retry);
        c.bench_function(name, |b| {
            b.iter(|| {
                let result = facade.run(|| Err::<u64, &'static str>(black_box("failure")));
                let _ = black_box(result);
            });
        });
    }
}

/// Measures the successful completion callback path without callback panics.
fn benchmark_completion_observer(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry = RetryConfig::<&'static str>::builder().policy(policy).observer(NoopObserver).build().expect("valid config");
    let facade = Retry::new(&retry);

    c.bench_function("completion_observer_success", |b| {
        b.iter(|| {
            let result = facade.run(|| Ok::<u64, &'static str>(black_box(42)));
            let _ = black_box(result);
        });
    });
}

/// Measures one successful no-op worker attempt, including spawn and reaping.
fn benchmark_worker_noop(c: &mut Criterion) {
    #[cfg(feature = "worker")]
    {
        let policy = RetryPolicy::builder()
            .max_attempts(1)
            .backoff(BackoffPolicy::immediate())
            .build()
            .expect("benchmark retry policy should be valid");
        let retry = RetryConfig::<&'static str>::builder().policy(policy).build().expect("valid config");
        let facade = WorkerRetry::new(&retry);

        c.bench_function("worker_noop_success", |b| {
            b.iter(|| {
                let result = facade.run(|_| Ok::<u64, &'static str>(black_box(42)));
                let _ = black_box(result);
            });
        });
    }
    #[cfg(not(feature = "worker"))]
    let _ = c;
}

/// Measures a no-delay flow that retries two operation failures before success.
fn benchmark_sync_no_delay_retries(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(3)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry = RetryConfig::<&'static str>::builder().policy(policy)
        .fallback(qubit_retry::RetryFallback::Retry)
        .build().expect("valid config");

    c.bench_function("sync_no_delay_retries", |b| {
        b.iter(|| {
            let mut attempts = 0;
            let result = Retry::new(&retry).run(|| {
                attempts += 1;
                if attempts < 3 {
                    Err("retry")
                } else {
                    Ok(black_box(42_u64))
                }
            });
            let _ = black_box((result, attempts));
        });
    });
}

/// Measures a failed attempt with one failure listener installed.
fn benchmark_sync_failure_listener(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry = RetryConfig::<&'static str>::builder().policy(policy).observer(observe_failure).build().expect("valid config");

    c.bench_function("sync_failure_listener", |b| {
        b.iter(|| {
            let result = Retry::new(&retry).run(|| Err::<u64, &'static str>(black_box("failure")));
            let _ = black_box(result);
        });
    });
}

/// Measures ordered rule dispatch through several default decisions.
fn benchmark_rule_chain_decision(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry = RetryConfig::<&'static str>::builder().policy(policy)
        .rule(use_default_rule)
        .rule(use_default_rule)
        .rule(use_default_rule)
        .rule(abort_rule)
        .build().expect("valid config");

    c.bench_function("rule_chain_decision", |b| {
        b.iter(|| {
            let result = Retry::new(&retry).run(|| Err::<u64, &'static str>(black_box("failure")));
            let _ = black_box(result);
        });
    });
}

/// Measures one exponential backoff calculation with fresh state.
fn benchmark_backoff_calculation(c: &mut Criterion) {
    let policy = BackoffPolicy::exponential(Duration::from_millis(10), 2.0, Duration::from_secs(1))
        .expect("benchmark backoff policy should be valid");
    let request = BackoffRequest::policy();

    c.bench_function("backoff_calculation", |b| {
        b.iter_batched(
            || policy.start(),
            |mut state| {
                let step = state.next(black_box(request));
                let _ = black_box(step);
            },
            BatchSize::SmallInput,
        );
    });
}

/// Measures one successful Tokio-backed async retry execution.
fn benchmark_async_success(c: &mut Criterion) {
    #[cfg(feature = "tokio")]
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("benchmark Tokio runtime should be available");
        let policy = RetryPolicy::builder()
            .max_attempts(1)
            .backoff(BackoffPolicy::immediate())
            .build()
            .expect("benchmark retry policy should be valid");
        let retry = RetryConfig::<&'static str>::builder().policy(policy).build().expect("valid config");

        c.bench_function("async_success", |b| {
            b.iter(|| {
                let result = runtime.block_on(TokioRetry::new(&retry).run(|| async { Ok::<u64, &'static str>(black_box(42)) }));
                let _ = black_box(result);
            });
        });
    }
    #[cfg(not(feature = "tokio"))]
    let _ = c;
}

criterion_group!(
    retry_benches,
    benchmark_sync_success,
    benchmark_sync_cancellation_token,
    benchmark_observer_counts,
    benchmark_rule_counts,
    benchmark_completion_observer,
    benchmark_worker_noop,
    benchmark_sync_no_delay_retries,
    benchmark_sync_failure_listener,
    benchmark_rule_chain_decision,
    benchmark_backoff_calculation,
    benchmark_async_success,
);
criterion_main!(retry_benches);
