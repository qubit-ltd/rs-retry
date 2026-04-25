# Qubit Retry

[![CircleCI](https://circleci.com/gh/qubit-ltd/rs-retry.svg?style=shield)](https://circleci.com/gh/qubit-ltd/rs-retry)
[![Coverage Status](https://coveralls.io/repos/github/qubit-ltd/rs-retry/badge.svg?branch=main)](https://coveralls.io/github/qubit-ltd/rs-retry?branch=main)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Qubit Retry provides type-preserving retry policies for Rust sync and async operations.

The core API is `Retry<E>`. The retry policy is bound only to the operation error type `E`; the success type `T` is introduced by each `run` or `run_async` call.

## Features

- Sync retry works without optional features.
- Async retry and per-attempt timeout are available with the `tokio` feature.
- `qubit-config` integration is available with the `config` feature.
- Retry callbacks are stored with `rs-function` functors, so closures and custom function objects are both supported.
- `AttemptFailure<E>` represents one failed attempt: either `Error(E)` or `Timeout`.
- `RetryError<E>` represents the terminal retry-flow error and carries `reason`, `last_failure`, and `RetryContext`.
- Lifecycle hooks are explicit: `before_attempt`, `on_success`, `on_failure`, and `on_error`.

## Installation

```toml
[dependencies]
qubit-retry = "0.7.0"
```

Enable optional integrations as needed:

```toml
[dependencies]
qubit-retry = { version = "0.7.0", features = ["tokio", "config"] }
```

Optional features:

- `tokio`: enables `Retry::run_async` and per-attempt async timeout support through `tokio::time::timeout`.
- `config`: enables `RetryOptions::from_config` and `RetryConfigValues` for reading retry settings from `qubit-config`.

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

By default, operation errors are retried until configured attempt or elapsed-time limits stop the flow. Use `retry_if_error` for simple error predicates:

```rust
use qubit_retry::{Retry, RetryContext};
use std::time::Duration;

let retry = Retry::<ServiceError>::builder()
    .max_attempts(4)
    .exponential_backoff(Duration::from_millis(100), Duration::from_secs(2))
    .retry_if_error(|error: &ServiceError, _context: &RetryContext| error.is_retryable())
    .build()?;
```

Use `on_failure` when decisions need access to attempt timeout, retry-after hints, or failure kind:

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
            _ => AttemptFailureDecision::UseDefault,
        },
    )
    .build()?;
```

`AttemptFailureDecision::UseDefault` lets the retry policy apply its configured limits, delay strategy, jitter, and optional retry-after hint.

## Async Retry and Timeout

Async execution requires the `tokio` feature. Per-attempt timeout is configured on the builder and is reflected in `AttemptFailure::Timeout` plus `RetryContext::attempt_timeout()`.

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

## Retry-After Hints

If an attempt failure carries retry-after information, register a hint extractor with `retry_after_hint`. The extractor returns `Option<Duration>`: `Some(delay)` means "wait this long before the next retry", while `None` means "no hint is available". The default policy uses `Some(delay)` when all failure listeners return `UseDefault`; otherwise it falls back to the configured delay strategy.

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

When the hint depends only on the operation error, `retry_after_from_error` is a convenience wrapper around `retry_after_hint`:

```rust
let retry = Retry::<ServiceError>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .retry_after_from_error(|error: &ServiceError| error.retry_after())
    .build()?;
```

Listeners can also read the extracted value from `RetryContext::retry_after_hint()`.

## Listeners

Listeners are lifecycle hooks, not separate policy systems:

- `before_attempt`: invoked before every attempt, including the first attempt.
- `on_success`: invoked after each successful attempt.
- `on_failure`: invoked after each `AttemptFailure` and returns `AttemptFailureDecision`.
- `on_error`: invoked once when the retry flow returns a terminal `RetryError`.

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
    .on_error(|error: &RetryError<std::io::Error>, context: &RetryContext| {
        tracing::error!(
            reason = ?error.reason(),
            attempts = context.attempt(),
            elapsed_ms = context.total_elapsed().as_millis(),
            "retry flow failed",
        );
    })
    .build()?;
```

## Configuration

`RetryOptions` is an immutable snapshot. Reading from `qubit-config` requires the `config` feature and happens during construction.

```rust
use qubit_config::Config;
use qubit_retry::{Retry, RetryOptions};

let mut config = Config::new();
config.set("retry.max_attempts", 5u32)?;
config.set("retry.max_elapsed_millis", 30_000u64)?;
config.set("retry.delay", "exponential")?;
config.set("retry.exponential_initial_delay_millis", 200u64)?;
config.set("retry.exponential_max_delay_millis", 5_000u64)?;
config.set("retry.exponential_multiplier", 2.0)?;
config.set("retry.jitter_factor", 0.2)?;

let options = RetryOptions::from_config(&config.prefix_view("retry"))?;
let retry = Retry::<std::io::Error>::from_options(options)?;
```

Supported relative keys:

- `max_attempts`
- `max_elapsed_millis`
- `max_elapsed_unlimited`
- `delay`: `none`, `fixed`, `random`, `exponential`, or `exponential_backoff`
- `fixed_delay_millis`
- `random_min_delay_millis`
- `random_max_delay_millis`
- `exponential_initial_delay_millis`
- `exponential_max_delay_millis`
- `exponential_multiplier`
- `jitter_factor`

## Error Handling

Inspect `RetryError::reason()`, `RetryError::last_failure()`, and `RetryError::context()` to distinguish terminal causes from attempt failures:

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
        eprintln!("elapsed: {:?}", error.context().total_elapsed());

        if error.reason() == RetryErrorReason::AttemptsExceeded {
            if let Some(AttemptFailure::Error(source)) = error.last_failure() {
                eprintln!("last operation error: {source}");
            }
        }
    }
}
```
