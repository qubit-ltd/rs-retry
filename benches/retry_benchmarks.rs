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
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryPolicy;

/// No-op failure listener used to measure listener dispatch overhead.
fn observe_failure(
    _failure: &AttemptFailure<&'static str>,
    _context: &RetryContext,
) {
}

/// Continues rule-chain dispatch without selecting a terminal decision.
fn use_default_rule(
    _failure: &AttemptFailure<&'static str>,
    _context: &RetryContext,
) -> RetryDecision {
    RetryDecision::UseDefault
}

/// Terminates rule-chain dispatch after preceding default decisions.
fn abort_rule(
    _failure: &AttemptFailure<&'static str>,
    _context: &RetryContext,
) -> RetryDecision {
    RetryDecision::Abort
}

/// Measures the lowest-overhead successful synchronous execution path.
fn benchmark_sync_success(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(1)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry = Retry::<&'static str>::builder(policy).build();

    c.bench_function("sync_success", |b| {
        b.iter(|| {
            let result =
                retry.sync().run(|| Ok::<u64, &'static str>(black_box(42)));
            let _ = black_box(result);
        });
    });
}

/// Measures a no-delay flow that retries two operation failures before success.
fn benchmark_sync_no_delay_retries(c: &mut Criterion) {
    let policy = RetryPolicy::builder()
        .max_attempts(3)
        .backoff(BackoffPolicy::immediate())
        .build()
        .expect("benchmark retry policy should be valid");
    let retry = Retry::<&'static str>::builder(policy).build();

    c.bench_function("sync_no_delay_retries", |b| {
        b.iter(|| {
            let mut attempts = 0;
            let result = retry.sync().run(|| {
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
    let retry = Retry::<&'static str>::builder(policy)
        .observer(observe_failure)
        .build();

    c.bench_function("sync_failure_listener", |b| {
        b.iter(|| {
            let result = retry
                .sync()
                .run(|| Err::<u64, &'static str>(black_box("failure")));
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
    let retry = Retry::<&'static str>::builder(policy)
        .rule(use_default_rule)
        .rule(use_default_rule)
        .rule(use_default_rule)
        .rule(abort_rule)
        .build();

    c.bench_function("rule_chain_decision", |b| {
        b.iter(|| {
            let result = retry
                .sync()
                .run(|| Err::<u64, &'static str>(black_box("failure")));
            let _ = black_box(result);
        });
    });
}

/// Measures one exponential backoff calculation with fresh state.
fn benchmark_backoff_calculation(c: &mut Criterion) {
    let policy = BackoffPolicy::exponential(
        Duration::from_millis(10),
        2.0,
        Duration::from_secs(1),
    )
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
        let retry = Retry::<&'static str>::builder(policy).build();

        c.bench_function("async_success", |b| {
            b.iter(|| {
                let result =
                    runtime.block_on(retry.asynchronous().run(|| async {
                        Ok::<u64, &'static str>(black_box(42))
                    }));
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
    benchmark_sync_no_delay_retries,
    benchmark_sync_failure_listener,
    benchmark_rule_chain_decision,
    benchmark_backoff_calculation,
    benchmark_async_success,
);
criterion_main!(retry_benches);
