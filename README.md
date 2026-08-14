# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Qubit Retry is a small, typed retry engine for fallible Rust work. It is for
services, clients, and storage code that need bounded retries without losing
the original application error or hiding why execution stopped.

## A real use case

An HTTP client can retry a transient response and return a server `Retry-After`
value as `RetryWithHint`, then report a stable terminal category to its caller:

```rust
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
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

let response = retry.sync().run(|| std::fs::read("Cargo.toml"))?.into_value();
# Ok::<(), Box<dyn std::error::Error>>(())
```

The same policy can be used by an async client through `retry.asynchronous()`
or by blocking code through `retry.worker()`.

When a domain error carries a server delay, the rule can return
`RetryDecision::RetryWithHint(delay)`. The configured backoff policy then decides
whether that hint is preferred, used as a minimum, or ignored.

## Installation

```toml
[dependencies]
qubit-retry = "0.18"
```

Enable Tokio execution when needed:

```toml
qubit-retry = { version = "0.18", features = ["tokio"] }
```

## What it provides

- `RetryPolicy` combines validated attempt and elapsed budgets with an opaque,
  reusable `BackoffPolicy`.
- `Retry::sync()` runs same-thread closures and deliberately exposes no
  timeout, because arbitrary Rust code cannot be interrupted safely.
- `Retry::asynchronous()` supports per-attempt and whole-flow timeouts when
  the `tokio` feature is enabled.
- `Retry::worker()` isolates blocking attempts, captures panics, and waits for
  cooperative cancellation before allowing another worker to start.
- Ordered `RetryRule` values choose `Retry`, `RetryWithHint`,
  `RetryWithJitteredHint`, `Abort`, or
  `UseDefault`; `RetryObserver` values receive isolated lifecycle callbacks.
- `AttemptFailureKind` and `RetryErrorKind` are stable categories. Detailed
  `AttemptFailure` and `RetryErrorReason` values are marked non-exhaustive so
  new runtime details can be added without changing category handling.
- `BackoffState` is reusable for reconnect loops such as SSE and supports
  server hints, jitter, and overflow-safe exponential growth.

Budgets only control whether another attempt may start. If an admitted
operation succeeds after crossing a budget, the success is returned. A retry
policy never forcefully kills a synchronous operation or an uncooperative
worker thread.

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
