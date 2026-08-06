// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Benchmarks for the common synchronous retry execution paths.

use criterion::{
    Criterion,
    black_box,
    criterion_group,
    criterion_main,
};
use qubit_retry::{
    AttemptFailure,
    AttemptFailureDecision,
    Retry,
    RetryContext,
};

/// No-op failure listener used to measure listener dispatch overhead.
fn observe_failure(
    _failure: &AttemptFailure<&'static str>,
    _context: &RetryContext,
) -> AttemptFailureDecision {
    AttemptFailureDecision::UseDefault
}

/// Measures the lowest-overhead successful synchronous execution path.
fn benchmark_sync_success(c: &mut Criterion) {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .build()
        .expect("benchmark retry policy should be valid");

    c.bench_function("sync_success", |b| {
        b.iter(|| {
            let result = retry.run(|| Ok::<u64, &'static str>(black_box(42)));
            let _ = black_box(result);
        });
    });
}

/// Measures a no-delay flow that retries two operation failures before success.
fn benchmark_sync_no_delay_retries(c: &mut Criterion) {
    let retry = Retry::<&'static str>::builder()
        .max_attempts(3)
        .no_delay()
        .build()
        .expect("benchmark retry policy should be valid");

    c.bench_function("sync_no_delay_retries", |b| {
        b.iter(|| {
            let mut attempts = 0;
            let result = retry.run(|| {
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
    let retry = Retry::<&'static str>::builder()
        .max_attempts(1)
        .no_delay()
        .on_failure(observe_failure)
        .build()
        .expect("benchmark retry policy should be valid");

    c.bench_function("sync_failure_listener", |b| {
        b.iter(|| {
            let result =
                retry.run(|| Err::<u64, &'static str>(black_box("failure")));
            let _ = black_box(result);
        });
    });
}

criterion_group!(
    retry_benches,
    benchmark_sync_success,
    benchmark_sync_no_delay_retries,
    benchmark_sync_failure_listener,
);
criterion_main!(retry_benches);
