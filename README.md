# Qubit Retry

[![CircleCI](https://circleci.com/gh/qubit-ltd/rs-retry.svg?style=shield)](https://circleci.com/gh/qubit-ltd/rs-retry)
[![Coverage Status](https://coveralls.io/repos/github/qubit-ltd/rs-retry/badge.svg?branch=main)](https://coveralls.io/github/qubit-ltd/rs-retry?branch=main)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Qubit Retry provides type-preserving retry executors for Rust sync and async operations.

The core API is `RetryExecutor<E>`. An executor is bound only to the operation error type `E`; the success type `T` is introduced by `run` or `run_async`. This means normal error retry does not require `T: Clone + Eq + Hash`.

## Features

- Type-preserving `RetryError<E>` that keeps the original operation error.
- Sync retry via `RetryExecutor::run`.
- Async retry via `RetryExecutor::run_async`.
- Real async per-attempt timeout via `RetryExecutor::run_async_with_timeout`.
- Delay strategies: `Delay::none`, `Delay::fixed`, `Delay::random`, `Delay::exponential`.
- Symmetric jitter through `Jitter::factor`.
- Explicit retry classification with `retry_if` or `retry_decide`.
- Listener contexts for retry/failure/abort plus borrowed failure payloads.
- Listener callback storage based on `qubit-function` functors (`ArcConsumer` / `ArcBiConsumer`).
- Immutable `RetryOptions` snapshots with `qubit-config` integration.

## Core Concepts

`qubit-retry` is designed around a type-preserving retry executor with clear boundaries between retry policy, error classification, and operation execution:

- `RetryExecutor<E>` stores retry behavior and error classification.
- `run<T, _>` and `run_async<T, _, _>` introduce the success type only at execution time.
- Listener callbacks observe context metadata and borrowed failures instead of owned success values.
- `RetryOptions` provides a validated immutable snapshot for retry configuration.

## Installation

```toml
[dependencies]
qubit-retry = "0.4.0"
```

## Basic Sync Retry

```rust
use qubit_retry::{Delay, RetryExecutor};
use std::time::Duration;

fn read_config() -> Result<String, Box<dyn std::error::Error>> {
    let executor = RetryExecutor::<std::io::Error>::builder()
        .max_attempts(3)
        .delay(Delay::fixed(Duration::from_millis(100)))
        .build()?;

    let text = executor.run(|| std::fs::read_to_string("config.toml"))?;
    Ok(text)
}
```

## Error Classification

By default, all operation errors are retryable until the attempt or elapsed-time limit is reached. Use `retry_if` when only some errors should be retried:

```rust
use qubit_retry::{Delay, RetryExecutor};
use std::time::Duration;

#[derive(Debug)]
enum ServiceError {
    RateLimited,
    TemporaryUnavailable,
    InvalidRequest,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ServiceError {}

fn is_retryable(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::RateLimited | ServiceError::TemporaryUnavailable
    )
}

let executor = RetryExecutor::<ServiceError>::builder()
    .max_attempts(4)
    .delay(Delay::exponential(
        Duration::from_millis(100),
        Duration::from_secs(2),
        2.0,
    ))
    .retry_if(|error, _context| is_retryable(error))
    .build()?;
```

Use `retry_decide` when the classifier needs to return a named decision:

```rust
use qubit_retry::{RetryDecision, RetryExecutor};

let executor = RetryExecutor::<ServiceError>::builder()
    .max_attempts(3)
    .retry_decide(|error, context| {
        if context.attempt == 1 && is_retryable(error) {
            RetryDecision::Retry
        } else {
            RetryDecision::Abort
        }
    })
    .build()?;
```

## Async Retry and Attempt Timeout

`run_async_with_timeout` uses `tokio::time::timeout`, so timed-out attempts are actually cancelled at the future boundary.

```rust
use qubit_retry::{Delay, RetryExecutor};
use std::time::Duration;

async fn fetch_once() -> Result<String, std::io::Error> {
    Ok("response".to_string())
}

async fn fetch_with_retry() -> Result<String, Box<dyn std::error::Error>> {
    let executor = RetryExecutor::<std::io::Error>::builder()
        .max_attempts(3)
        .delay(Delay::fixed(Duration::from_millis(50)))
        .build()?;

    let response = executor
        .run_async_with_timeout(Duration::from_secs(2), || async {
            fetch_once().await
        })
        .await?;

    Ok(response)
}
```

Use `run_async` when you do not need a per-attempt timeout:

```rust
let response = executor
    .run_async(|| async {
        fetch_once().await
    })
    .await?;
```

## Listeners

Retry/failure/abort listeners receive a context object plus a borrowed failure payload. Success listeners still receive only `SuccessContext`.

```rust
pub type RetryListener<E> = ArcBiConsumer<RetryContext, AttemptFailure<E>>;
pub type FailureListener<E> = ArcBiConsumer<FailureContext, Option<AttemptFailure<E>>>;
pub type AbortListener<E> = ArcBiConsumer<AbortContext, AttemptFailure<E>>;
pub type SuccessListener = ArcConsumer<SuccessContext>;
```

```rust
use qubit_retry::{AttemptFailure, Delay, RetryExecutor};
use std::time::Duration;

let executor = RetryExecutor::<std::io::Error>::builder()
    .max_attempts(3)
    .delay(Delay::fixed(Duration::from_millis(100)))
    .on_retry(|context, failure| {
        if let AttemptFailure::Error(error) = failure {
            tracing::warn!(
                attempt = context.attempt,
                delay_ms = context.next_delay.as_millis(),
                error = %error,
                "retrying operation",
            );
        }
    })
    .on_failure(|context, last_failure| {
        tracing::error!(
            attempts = context.attempts,
            has_last_failure = last_failure.is_some(),
            "operation failed after retry",
        );
    })
    .on_abort(|context, failure| {
        tracing::warn!(
            attempts = context.attempts,
            failure = ?failure,
            "classifier aborted retry",
        );
    })
    .on_success(|context| {
        tracing::info!(attempts = context.attempts, "operation succeeded");
    })
    .build()?;
```

## Configuration

`RetryOptions` is an immutable snapshot. Reading from `qubit-config` happens once during construction.

```rust
use qubit_config::Config;
use qubit_retry::{RetryOptions, RetryExecutor};

let mut config = Config::new();
config.set("retry.max_attempts", 5u32)?;
config.set("retry.max_elapsed_millis", 30_000u64)?;
config.set("retry.delay", "exponential")?;
config.set("retry.exponential_initial_delay_millis", 200u64)?;
config.set("retry.exponential_max_delay_millis", 5_000u64)?;
config.set("retry.exponential_multiplier", 2.0)?;
config.set("retry.jitter_factor", 0.2)?;

let options = RetryOptions::from_config(&config.prefix_view("retry"))?;
let executor = RetryExecutor::<std::io::Error>::from_options(options)?;
```

Supported relative keys:

- `max_attempts`
- `max_elapsed_millis`
- `delay`: `none`, `fixed`, `random`, `exponential`, or `exponential_backoff`
- `fixed_delay_millis`
- `random_min_delay_millis`
- `random_max_delay_millis`
- `exponential_initial_delay_millis`
- `exponential_max_delay_millis`
- `exponential_multiplier`
- `jitter_factor`

## Error Handling

`RetryError<E>` preserves the original operation error when the terminal failure is an application error:

```rust
use qubit_retry::{RetryError, RetryExecutor};

let executor = RetryExecutor::<std::io::Error>::builder()
    .max_attempts(2)
    .build()?;

match executor.run(|| std::fs::read_to_string("missing.toml")) {
    Ok(text) => println!("{text}"),
    Err(error) => {
        eprintln!("attempts: {}", error.attempts());
        if let Some(source) = error.last_error() {
            eprintln!("last error: {source}");
        }

        if let RetryError::AttemptsExceeded { max_attempts, .. } = error {
            eprintln!("max attempts: {max_attempts}");
        }
    }
}
```
