# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Qubit Retry is a typed retry engine for Rust services, clients, and storage
code. It centralizes attempt budgets, backoff, callback handling, timeouts, and
cancellation while preserving both the application error and the exact reason
the flow stopped.

## Installation

```toml
[dependencies]
qubit-retry = "0.19"
```

Tokio execution and stable configuration serialization are opt-in:

```toml
qubit-retry = { version = "0.19", features = ["tokio", "serde"] }
```

## Quick start

A storage client can retry transient I/O failures, cap the complete flow, and
retain structured terminal information instead of flattening it into a string:

```rust
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryPolicy;

fn retry_io(
    failure: &AttemptFailure<std::io::Error>,
    _context: &RetryContext,
) -> RetryDecision {
    match failure {
        AttemptFailure::Error(error)
            if error.kind() == std::io::ErrorKind::TimedOut => RetryDecision::Retry,
        _ => RetryDecision::Abort,
    }
}

let policy = RetryPolicy::builder()
    .max_attempts(4)
    .max_total_elapsed(Duration::from_secs(10))
    .backoff(
        BackoffPolicy::exponential(
            Duration::from_millis(50),
            2.0,
            Duration::from_secs(2),
        )?
        .prefer_retry_after(),
    )
    .build()?;

let retry = Retry::<std::io::Error>::builder(policy)
    .rule(retry_io)
    .build();

let response = match retry.sync().run(|| std::fs::read("Cargo.toml")) {
    Ok(success) => success.into_value(),
    Err(error) => match error.failure() {
        RetryFailure::Exhausted {
            last_failure: Some(AttemptFailure::Error(source)),
            ..
        } => return Err(source.to_string().into()),
        failure => return Err(failure.to_string().into()),
    },
};
assert!(!response.is_empty());
# Ok::<(), Box<dyn std::error::Error>>(())
```

`RetryDecision::RetryWithHint(delay)` can carry a server `Retry-After` value.
The configured backoff policy decides whether that hint is preferred, used as
a minimum, or ignored.

### Async cancellation

With the `tokio` feature, the runtime-independent token interrupts an active
attempt or backoff and reports the phase through `RetryFailure::Cancelled`:

```rust
use std::future;

use qubit_retry::{Retry, RetryCancellationPhase, RetryCancellationToken};
use qubit_retry::{RetryFailure, RetryPolicy};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let policy = RetryPolicy::builder().build()?;
let retry = Retry::<std::io::Error>::builder(policy).build();
let cancellation = RetryCancellationToken::new();
let operation_cancellation = cancellation.clone();

let result = retry
    .asynchronous()
    .cancellation_token(cancellation)
    .run(move || {
        operation_cancellation.cancel();
        future::pending::<Result<(), std::io::Error>>()
    })
    .await;

let error = result.expect_err("the pending attempt should be cancelled");
assert!(matches!(
    error.failure(),
    RetryFailure::Cancelled {
        phase: RetryCancellationPhase::Attempt,
        ..
    }
));
# Ok(())
# }
```

Pass a clone to the application's shutdown path and call `cancel()` there.
Cancellation is permanent and visible to every clone.

### Blocking worker cancellation

Worker mode isolates blocking code on a thread. The flow token stops the retry,
while the per-attempt token asks the current operation to exit cooperatively:

```rust
use std::io;

use qubit_retry::{Retry, RetryCancellationPhase, RetryCancellationToken};
use qubit_retry::{RetryFailure, RetryPolicy};

let policy = RetryPolicy::builder()
    .build()
    .expect("retry policy should be valid");
let retry = Retry::<io::Error>::builder(policy).build();
let cancellation = RetryCancellationToken::new();
let operation_cancellation = cancellation.clone();
let result = retry
    .worker()
    .cancellation_token(cancellation)
    .run(move |attempt| {
        operation_cancellation.cancel();
        loop {
            if attempt.is_cancelled() {
                break Err::<(), io::Error>(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "cancelled",
                ));
            }
            // A real operation would process one bounded work unit here.
            std::thread::yield_now();
        }
    });

let error = result.expect_err("the active worker attempt should be cancelled");
assert!(matches!(
    error.failure(),
    RetryFailure::Cancelled {
        phase: RetryCancellationPhase::Attempt,
        ..
    }
));
```

Rust cannot forcibly kill an uncooperative thread. If it does not exit within
the configured grace period, the flow fails closed with
`WorkerStillRunning` and the precise timeout or cancellation trigger.

## Why this project exists

Retry loops often spread attempt counting, sleeps, shutdown checks, and error
conversion across call sites. That makes it easy to lose the last application
error or report a timeout, cancellation, and callback panic as the same opaque
failure. Qubit Retry keeps these decisions in one policy-driven flow and
returns a coherent `RetryContext` with every success or terminal error.

## What it provides

- `RetryPolicy` combines validated attempt, operation-time, and total-time
  budgets with reusable immediate, fixed, uniform, or exponential backoff.
- `Retry::sync()` runs same-thread closures and deliberately exposes no
  timeout or cancellation promise, because arbitrary synchronous code cannot
  be interrupted safely.
- `Retry::asynchronous()` supports attempt/flow timeouts and cancellation when
  the `tokio` feature is enabled; Tokio is the only async runtime integration.
- `Retry::worker()` captures panics, preserves the stop trigger, and never
  starts another worker while an earlier uncooperative worker may still run.
- `RetryFailure` distinguishes aborted, exhausted, timed-out, cancelled,
  callback-failed, and infrastructure terminals. `AttemptFailure` retains an
  application error, timeout scope, or stable panic payload.
- Ordered rules and observers fail closed on callback panic and retain callback
  kind, registration index, lifecycle phase, and string panic payloads.
- `BackoffState` is reusable for reconnect loops such as SSE and supports
  server hints, jitter, and overflow-safe exponential growth.
- The optional `serde` feature serializes configuration types only; runtime
  results, errors, and callback state are deliberately not wire protocols.

Budgets only control whether another attempt may start. If an admitted
operation succeeds after crossing a budget, the success is returned. A retry
policy never forcefully kills a synchronous operation or an uncooperative
worker thread. Cancellation tokens are not resettable and do not provide parent
trees or built-in deadlines.

## Learn more

- [Rust API documentation](https://docs.rs/qubit-retry)
- [中文 README](README.zh_CN.md)
- [Repository](https://github.com/qubit-ltd/rs-retry)

## Testing

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
./ci-check.sh

# Check code coverage
./coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./align-ci.sh` to format code and
`./ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-retry](https://github.com/qubit-ltd/rs-retry)
