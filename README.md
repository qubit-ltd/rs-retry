# Qubit Retry

[![CircleCI](https://circleci.com/gh/qubit-ltd/rs-retry.svg?style=shield)](https://circleci.com/gh/qubit-ltd/rs-retry)
[![Coverage Status](https://coveralls.io/repos/github/qubit-ltd/rs-retry/badge.svg?branch=main)](https://coveralls.io/github/qubit-ltd/rs-retry?branch=main)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Qubit Retry is a type-preserving retry toolkit for Rust synchronous and asynchronous operations.

The core API is `Retry<E>`. A retry policy is bound only to the operation error type `E`; each `run` or `run_async` call introduces its own success type `T`.

## Overview

Qubit Retry is designed for applications that need explicit, observable retry behavior around fallible work. It supports synchronous operations, Tokio-based async operations, and blocking work isolated on worker threads. Policies are configured through a builder or optional `qubit-config` integration, while lifecycle hooks expose each attempt, failure, retry decision, terminal error, and successful completion.

Use this crate when you need typed retry errors, bounded elapsed-time budgets, retry-after hints, panic-aware worker execution, or retry callbacks that can be implemented as closures or reusable function objects.

## Features

- Synchronous retry works without optional features.
- Tokio-backed async retry supports true per-attempt timeouts.
- Blocking operations can use `run_in_worker` for thread-isolated execution, panic capture, timeout waiting, and cooperative cancellation.
- Optional `qubit-config` integration reads retry settings from configuration.
- Retry callbacks are stored as `rs-function` functors, so both closures and custom function objects are supported.
- `AttemptFailure<E>` represents one failed attempt: `Error(E)`, `Timeout`, `Panic(AttemptPanic)`, or `Executor(AttemptExecutorError)`.
- `RetryError<E>` represents the terminal retry-flow error and carries `reason`, `last_failure`, and `RetryContext`.
- Separate elapsed budgets distinguish user operation time from total retry-flow time.
- Lifecycle hooks are explicit: `before_attempt`, `on_success`, `on_failure`, `on_retry`, and `on_error`.

## Installation

```toml
[dependencies]
qubit-retry = "0.10"
```

Enable optional integrations as needed:

```toml
[dependencies]
qubit-retry = { version = "0.10", features = ["tokio", "config"] }
```

Optional features:

- `tokio`: enables `Retry::run_async` and per-attempt async timeouts through `tokio::time::timeout`.
- `config`: enables `RetryOptions::from_config` and `RetryConfigValues` for reading settings from `qubit-config`.

The default feature set is empty, so synchronous retry does not pull in `tokio` or `qubit-config`.

## Basic Sync Retry

```rust
use qubit_retry::Retry;
use std::time::Duration;

fn read_config() -> Result<String, Box<dyn std::error::Error>> {
    let retry = Retry::<std::io::Error>::builder()
        .max_attempts(3)
        .fixed_delay(Duration::from_millis(100))
        .build()?;

    let text = retry.run(|| std::fs::read_to_string("config.toml"))?;
    Ok(text)
}
```

## Failure Decisions

By default, operation errors are retried until the configured attempt or elapsed-time limits stop the flow. Use `retry_if_error` for simple error predicates:

```rust
use qubit_retry::{Retry, RetryContext};
use std::time::Duration;

let retry = Retry::<ServiceError>::builder()
    .max_attempts(4)
    .exponential_backoff(Duration::from_millis(100), Duration::from_secs(2))
    .retry_if_error(|error: &ServiceError, _context: &RetryContext| error.is_retryable())
    .build()?;
```

Use `on_failure` when a decision needs the failure kind, attempt timeout, retry-after hint, or any other `RetryContext` value:

```rust
use qubit_retry::{Retry, RetryContext, AttemptFailure, AttemptFailureDecision};
use std::time::Duration;

let retry = Retry::<ServiceError>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .on_failure(
        |failure: &AttemptFailure<ServiceError>, context: &RetryContext| match failure {
            AttemptFailure::Error(error) if error.is_rate_limited() => {
                AttemptFailureDecision::RetryAfter(Duration::from_secs(1))
            }
            AttemptFailure::Error(error) if error.is_retryable() => AttemptFailureDecision::Retry,
            AttemptFailure::Timeout if context.attempt_timeout().is_some() => {
                AttemptFailureDecision::Abort
            }
            AttemptFailure::Panic(_) => AttemptFailureDecision::Abort,
            AttemptFailure::Executor(_) => AttemptFailureDecision::Abort,
            _ => AttemptFailureDecision::UseDefault,
        },
    )
    .build()?;
```

`AttemptFailureDecision::UseDefault` hands control back to the retry policy, which then applies the configured limits, delay strategy, jitter, and optional retry-after hint.

## Async Retry and Timeout

Async execution requires the `tokio` feature. Per-attempt timeouts are stored in `RetryOptions` through the builder. When an attempt times out, the executor reports `AttemptFailure::Timeout`, and listeners can inspect the configured timeout through `RetryContext::attempt_timeout()`. Operation panics still unwind through the current async task; `run_async()` does not convert them to `AttemptFailure::Panic`.

```rust
use qubit_retry::Retry;
use std::time::Duration;

async fn fetch_once() -> Result<String, std::io::Error> {
    Ok("response".to_string())
}

async fn fetch_with_retry() -> Result<String, Box<dyn std::error::Error>> {
    let retry = Retry::<std::io::Error>::builder()
        .max_attempts(3)
        .fixed_delay(Duration::from_millis(50))
        .attempt_timeout(Some(Duration::from_secs(2)))
        .retry_on_timeout()
        .build()?;

    let response = retry
        .run_async(|| async {
            fetch_once().await
        })
        .await?;

    Ok(response)
}
```

Plain `run()` keeps normal same-thread synchronous execution. It is the lowest-overhead path and works well for short, high-frequency operations such as CAS loops. `run()` does not support configured per-attempt timeouts: it returns `RetryErrorReason::UnsupportedOperation` when `attempt_timeout` is set. Use `run_async()` for cancellable async futures, or `run_in_worker()` when blocking work must run on a worker thread.

## Elapsed Budgets

Retry elapsed budgets are measured with monotonic `Instant` time, not wall-clock time:

- `max_operation_elapsed`: cumulative time spent executing user operation attempts. Retry sleeps, retry-after sleeps, and listener time are excluded.
- `max_total_elapsed`: total retry-flow time. Operation attempts, retry sleeps, retry-after sleeps, retry hint extraction, `on_before_attempt`, `on_failure`, and `on_retry` time are included.

Terminal listeners keep notification semantics. `on_success` and `on_error` can add caller-visible latency, but they do not turn an already successful operation into a retry failure.

Async and worker-thread attempts use the shortest of configured `attempt_timeout`, remaining `max_operation_elapsed`, and remaining `max_total_elapsed` as the effective attempt timeout. If the selected retry or retry-after delay would consume the remaining `max_total_elapsed` budget, the flow fails with `RetryErrorReason::MaxTotalElapsedExceeded` before sleeping. Retry sleeps are not truncated.

## Worker-Thread Retry

`run_in_worker()` runs every attempt on a worker thread. Without an attempt timeout, the caller waits for the worker result and worker panics are captured as `AttemptFailure::Panic`. Worker-spawn failures are reported as `AttemptFailure::Executor`. With an attempt timeout, the retry executor stops waiting when the timeout expires, marks the attempt token as cancelled, and waits up to `worker_cancel_grace` (default `100ms`) for the worker to exit before applying the configured `AttemptTimeoutPolicy`.

Rust cannot safely kill a running thread, so a timed-out worker may keep running unless the operation checks the token and returns. If the worker is still running after the cancellation grace period, the retry flow stops with `RetryErrorReason::WorkerStillRunning` instead of starting another worker; `RetryContext::unreaped_worker_count()` reports the unreaped worker count. Use this path for blocking IO, third-party calls, code that may panic, or work that needs per-attempt timeout isolation. Prefer plain `run()` for low-latency in-memory work.

```rust
use qubit_retry::{AttemptCancelToken, Retry};
use std::time::Duration;

fn blocking_fetch(token: AttemptCancelToken) -> Result<String, std::io::Error> {
    for _ in 0..20 {
        if token.is_cancelled() {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::read_to_string("payload.txt")
}

let retry = Retry::<std::io::Error>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(50))
    .attempt_timeout(Some(Duration::from_secs(2)))
    .worker_cancel_grace(Duration::from_millis(25))
    .abort_on_timeout()
    .build()?;

let response = retry.run_in_worker(blocking_fetch)?;
```

`run_blocking_with_timeout()` remains available as a compatibility alias for `run_in_worker()`.

## Retry-After Hints

If an attempt failure carries retry-after information, register a hint extractor with `retry_after_hint`. The extractor returns `Option<Duration>`: `Some(delay)` means "wait this long before the next retry", while `None` means "no hint is available". When all failure listeners return `UseDefault`, the default policy uses `Some(delay)`; otherwise it falls back to the configured delay strategy.

```rust
use qubit_retry::{AttemptFailure, Retry, RetryContext};
use std::time::Duration;

let retry = Retry::<ServiceError>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .retry_after_hint(
        |failure: &AttemptFailure<ServiceError>, _context: &RetryContext| {
            failure.as_error().and_then(ServiceError::retry_after)
        },
    )
    .build()?;
```

When the hint depends only on the operation error, `retry_after_from_error` provides a shorter wrapper around `retry_after_hint`:

```rust
let retry = Retry::<ServiceError>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .retry_after_from_error(|error: &ServiceError| error.retry_after())
    .build()?;
```

Listeners can also read the extracted value from `RetryContext::retry_after_hint()`.

## Listeners

Listeners are lifecycle hooks, not a separate policy system:

- `before_attempt`: invoked **before** the operation runs for **each** attempt (including the first). Use it to mark the start of attempt *N*; the current attempt has not started yet, so this is not the “we failed and are about to back off” moment.
- `on_success`: invoked after each successful attempt.
- `on_failure`: invoked after each `AttemptFailure` and returns `AttemptFailureDecision`. Runs **before** the inter-attempt delay is chosen and **before** `on_retry`, and can influence abort vs retry and how the policy picks the next delay.
- `on_retry`: invoked only after a failed attempt will be retried **and** the **delay before the next** `before_attempt` has been **selected** (after `on_failure` / merged decisions); **before** the executor sleeps and **before** the next `before_attempt`. It is **observational** (cannot change backoff/retry); `RetryContext::next_delay()` is the sleep duration. If the flow will not retry (attempts or time budget exhausted, listener abort, etc.), `on_retry` is **not** called.
- `on_error`: invoked once when the retry flow returns a terminal `RetryError`.

`before_attempt` vs `on_retry` in one line: `before_attempt` fires at the **start of an attempt**; `on_retry` fires **right after a failure** once a **retry is scheduled and the next delay is known**, but **before** the sleep and the next attempt.

```rust
use qubit_retry::{
    AttemptFailure, AttemptFailureDecision, Retry, RetryContext, RetryError,
};

let retry = Retry::<std::io::Error>::builder()
    .max_attempts(3)
    .before_attempt(|context: &RetryContext| {
        tracing::debug!(attempt = context.attempt(), "starting attempt");
    })
    .on_success(|context: &RetryContext| {
        tracing::debug!(attempt = context.attempt(), "attempt succeeded");
    })
    .on_failure(
        |failure: &AttemptFailure<std::io::Error>, context: &RetryContext| {
            tracing::warn!(
                failure = %failure,
                attempt = context.attempt(),
                retry_after_hint = ?context.retry_after_hint(),
                "attempt failed",
            );
            AttemptFailureDecision::UseDefault
        },
    )
    .on_retry(
        |failure: &AttemptFailure<std::io::Error>, context: &RetryContext| {
            tracing::info!(
                failure = %failure,
                attempt = context.attempt(),
                next_delay = ?context.next_delay(),
                "will sleep before next attempt",
            );
        },
    )
    .on_error(|error: &RetryError<std::io::Error>, context: &RetryContext| {
        tracing::error!(
            reason = ?error.reason(),
            attempts = context.attempt(),
            operation_elapsed_ms = context.operation_elapsed().as_millis(),
            total_elapsed_ms = context.total_elapsed().as_millis(),
            "retry flow failed",
        );
    })
    .build()?;
```

## Configuration

`RetryOptions` is an immutable configuration snapshot. Reading from `qubit-config` requires the `config` feature and happens during construction.

```rust
use qubit_config::Config;
use qubit_retry::{Retry, RetryOptions};

let mut config = Config::new();
config.set("retry.max_attempts", 5u32)?;
config.set("retry.max_operation_elapsed_millis", 30_000u64)?;
config.set("retry.max_total_elapsed_millis", 60_000u64)?;
config.set("retry.delay", "exponential")?;
config.set("retry.exponential_initial_delay_millis", 200u64)?;
config.set("retry.exponential_max_delay_millis", 5_000u64)?;
config.set("retry.exponential_multiplier", 2.0)?;
config.set("retry.jitter_factor", 0.2)?;
config.set("retry.attempt_timeout_millis", 2_000u64)?;
config.set("retry.attempt_timeout_policy", "retry")?;
config.set("retry.worker_cancel_grace_millis", 25u64)?;

let options = RetryOptions::from_config(&config.prefix_view("retry"))?;
let retry = Retry::<std::io::Error>::from_options(options)?;
```

Supported relative keys:

- `max_attempts`
- `max_operation_elapsed_millis`
- `max_operation_elapsed_unlimited`
- `max_total_elapsed_millis`
- `max_total_elapsed_unlimited`
- `attempt_timeout_millis`
- `attempt_timeout_policy`: `retry` or `abort`
- `worker_cancel_grace_millis`
- `delay`: `none`, `fixed`, `random`, `exponential`, or `exponential_backoff`
- `fixed_delay_millis`
- `random_min_delay_millis`
- `random_max_delay_millis`
- `exponential_initial_delay_millis`
- `exponential_max_delay_millis`
- `exponential_multiplier`
- `jitter_factor`

## Error Handling

Use `RetryError::reason()`, `RetryError::last_failure()`, and `RetryError::context()` to distinguish the terminal cause from the last failed attempt:

```rust
use qubit_retry::{Retry, RetryErrorReason, AttemptFailure};

let retry = Retry::<std::io::Error>::builder()
    .max_attempts(2)
    .build()?;

match retry.run(|| std::fs::read_to_string("missing.toml")) {
    Ok(text) => println!("{text}"),
    Err(error) => {
        eprintln!("reason: {:?}", error.reason());
        eprintln!("attempts: {}", error.context().attempt());
        eprintln!("operation elapsed: {:?}", error.context().operation_elapsed());
        eprintln!("total elapsed: {:?}", error.context().total_elapsed());

        match error.last_failure() {
            Some(AttemptFailure::Error(source)) => {
                eprintln!("last operation error: {source}");
            }
            Some(AttemptFailure::Timeout) => {
                eprintln!("last attempt timed out");
            }
            Some(AttemptFailure::Panic(panic)) => {
                eprintln!("last attempt panicked: {}", panic.message());
            }
            Some(AttemptFailure::Executor(executor)) => {
                eprintln!("retry executor failed: {}", executor.message());
            }
            None => {}
        }
    }
}
```

## Documentation

- API documentation: [docs.rs/qubit-retry](https://docs.rs/qubit-retry)
- Crate package: [crates.io/crates/qubit-retry](https://crates.io/crates/qubit-retry)
- Source repository: [github.com/qubit-ltd/rs-retry](https://github.com/qubit-ltd/rs-retry)
- Coverage guide: [COVERAGE.md](COVERAGE.md)

## Testing

A minimal local run:

```bash
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

To mirror what continuous integration enforces, run the repository scripts from the project root:

```bash
./align-ci.sh
./ci-check.sh
./coverage.sh
```

`./align-ci.sh` formats code and applies local Clippy fixes so the branch follows CI rules. `./ci-check.sh` runs the CI-equivalent pipeline, including formatting checks, Clippy with warnings denied, debug and release builds, all-feature tests, rustdoc with warnings denied, JSON coverage threshold checks, and the security audit. `./coverage.sh` generates coverage reports; use `./coverage.sh help` for output formats such as HTML, text, LCOV, JSON, Cobertura, or all formats.

## Contributing

Issues and pull requests are welcome.

- Open an issue for bug reports, design questions, or larger feature proposals when it helps align on direction.
- Keep pull requests scoped to one behavior change, fix, or documentation update when practical.
- Code contributions must run `./align-ci.sh`, pass `./ci-check.sh`, and review coverage with `./coverage.sh` before submission.
- Add or update tests when you change runtime behavior.
- Update this README or public rustdoc when user-visible API behavior changes.

By contributing, you agree to license your contributions under the [Apache License, Version 2.0](LICENSE), the same license as this project.

## License

Copyright © 2026 Haixing Hu, Qubit Co. Ltd.

This project is licensed under the [Apache License, Version 2.0](LICENSE). See the `LICENSE` file in the repository for the full text.

## Author

**Haixing Hu** — Qubit Co. Ltd.

| | |
| --- | --- |
| **Repository** | [github.com/qubit-ltd/rs-retry](https://github.com/qubit-ltd/rs-retry) |
| **Documentation** | [docs.rs/qubit-retry](https://docs.rs/qubit-retry) |
| **Crate** | [crates.io/crates/qubit-retry](https://crates.io/crates/qubit-retry) |
