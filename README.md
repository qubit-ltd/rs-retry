# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Qubit Retry keeps retries for Rust clients, storage operations, and reconnect
loops in one typed policy. It preserves the original application error, the
reason execution stopped, and completion callback diagnostics.

## Why use it

A snapshot reader should retry a temporary timeout, stop on a permanent error,
and tell its caller how many operations actually ran. A handwritten loop often
mixes those decisions with shutdown and timing. `RetryPolicy` separates validated
limits and backoff from execution; each `run` owns fresh state, while cloned
`Retry` values share rules and observers without requiring `E: Clone`.

## Installation

Requires Rust 1.94 or newer. The default feature set is empty.

<!-- retry-example: kind=cargo features=none -->
```toml
[dependencies]
qubit-retry = "0.23"
```

Tokio execution and configuration serialization are opt-in:

<!-- retry-example: kind=cargo features=tokio,serde -->
```toml
[dependencies]
qubit-retry = { version = "0.23", features = ["tokio", "serde"] }
```

## Quick start

This deterministic snapshot-reader fixture fails once, then succeeds. Replace
its bounded operation with your storage client's call. The ten-second limit is
a **soft budget for further admission**: it cannot interrupt a synchronous read
or revoke an admitted success. The helper returns the original typed retry error.
Immediate backoff keeps this example fast; use the guide's jittered policy when
coordinating real remote requests.

<!-- retry-example: kind=run features=none -->
```rust
use std::io;
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryError;
use qubit_retry::RetryPolicy;
use qubit_retry::RetrySuccess;

fn fetch_snapshot(retry: &Retry<io::Error>) -> Result<RetrySuccess<Vec<u8>>, RetryError<io::Error>> {
    let mut calls = 0;
    retry.sync().run(|| {
        calls += 1;
        if calls == 1 {
            Err(io::Error::new(io::ErrorKind::TimedOut, "temporary read failure"))
        } else {
            Ok(b"snapshot-v2".to_vec())
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let policy = RetryPolicy::builder()
        .max_attempts(4)
        .total_time_budget(Duration::from_secs(10))
        .backoff(BackoffPolicy::immediate())
        .build()?;
    let retry = Retry::builder(policy)
        .rule(|failure: &AttemptFailure<io::Error>, _: &RetryContext| match failure {
            AttemptFailure::Error(error) if error.kind() == io::ErrorKind::TimedOut => RetryDecision::Retry,
            _ => RetryDecision::Abort,
        })
        .build();
    let (snapshot, context, diagnostics) = fetch_snapshot(&retry)?.into_parts();
    assert_eq!(snapshot, b"snapshot-v2");
    assert_eq!(context.attempts(), 2);
    assert!(diagnostics.is_empty());
    Ok(())
}
```

## Execution modes and limits

| Mode | Operation and resources | Timeout and cancellation boundary |
| --- | --- | --- |
| `sync()` | `FnMut` on the calling thread | Before admission, after failure, and during backoff; no hard operation timeout |
| `asynchronous()` | Non-`Send`, non-static futures supported; requires `tokio` | Cooperative attempt/flow timers drop a pending future; cannot preempt blocking polls |
| `worker()` | Requires the `worker` feature; `Send + 'static` work; one worker and one reaper per attempt | Cancellation requests cooperative exit; waits for join including TLS destruction, within real-time cleanup grace |

The default is **three total attempts**, immediate retries, and no elapsed budget.
Unmatched application errors abort by default; opt into retry-all behavior with
`RetryFallback::Retry`.
Captured attempt timeouts and worker panics are terminal by default, but a rule
may request retry within remaining limits. Rules run in registration order;
the first decision other than `UseDefault` wins. Flow timeout, cancellation,
control callback failure, and infrastructure failure cannot be recovered by a rule.

`operation_time_budget` counts admitted operation time; `total_time_budget`
includes control callbacks and backoff. Neither is a hard timeout. Sync success
wins over cancellation with a valid completion clock. Async selection prefers
ready operation results over cancellation, then timeout. Worker selection checks
cancellation, then timeout, before accepting a result plus confirmed thread exit.
Tokens are permanent, shared across clones, and have no reset or parent tree.

Worker grace expiry returns `WorkerStillRunning` with its trigger and starts no
next attempt. An uncooperative operation or TLS destructor may leave both threads
alive. Without timeout/cancellation, waiting for exit can be unbounded.

## Errors, hints, and completion

`RetryErrorReason` distinguishes `Aborted`, `Exhausted`, `TimedOut`, `Cancelled`,
`CallbackFailed`, and `Infrastructure`; `AttemptFailure` retains the application
error, timeout scope, or captured panic, while `RetryErrorMetadata` carries the
non-generic terminal details. Only worker mode catches operation
panics. Sync/async operation panics unwind to the caller or polling task.

Completion observers run synchronously after the result is frozen, once per
returned result, including failure before admission. Their panics attach ordered
diagnostics and do not replace the result; later completion observers still run.
Completion time is excluded from context and timeout control. Unwinding, dropping
an async run future, or process abort does not guarantee notification.

`into_parts()` preserves value/failure, context, and diagnostics. `map_error`
converts a retained application error with an `FnOnce` mapper called zero or one
times, without additional `Clone`, `Send`, or `'static` bounds. Explicit
`into_value_discarding_diagnostics()` and `into_failure_discarding_diagnostics()`
also discard context. Use them only at a boundary that intentionally ignores both.

`BackoffState` can be used independently for SSE or reconnect loops. Immediate,
fixed, uniform, and exponential policies support full/bounded jitter and server
hints. `maximum_delay()` describes the base strategy; `limit_delay()` caps the
final delay after hints and jitter and **can truncate a server minimum**. If that
minimum is mandatory, do not configure a smaller cap; stop or revise the budget.
The `serde` feature covers configuration, not runtime errors, results or callbacks.

## Migrating to 0.23

- Update direct dependencies and adapter lockfiles together. `into_parts()` now
  returns `(reason, last_failure, context, diagnostics)` and diagnostics are
  stored in `Box<[RetryCallbackFailure]>`.
- Replace `RetryFailure<E>` with `RetryErrorReason` plus the independent
  `last_failure`; use `into_metadata_and_error()` when a domain adapter needs
  to separate the application error from terminal metadata.
- `RetryLimits`/`limits()` are now `RetryAdmissionLimits`/`admission_limits()`;
  `max_operation_elapsed`/`max_total_elapsed` are now
  `operation_time_budget`/`total_time_budget`.
- `attempt_timeout` and `flow_timeout` are now `hard_attempt_timeout` and
  `hard_flow_timeout`. Worker execution is opt-in through the `worker` feature.
- Replace `into_value()` / `into_failure()` with the explicit diagnostic-discarding
  names only when loss is intended. Prefer the full triple for conversions.
- Normal control callback return now refreshes the clock before cancellation or
  a returned decision. An invalid clock becomes `Infrastructure::Clock`; callback
  panic remains `CallbackFailed` with best-effort timing. Callback time is included
  in cancellation snapshots.
- Uniform backoff has exact interval endpoints. Worker and callback non-string
  panic payloads retain `NonString` even if payload destruction panics; only the
  exceptional secondary payload is leaked to prevent recursive destruction.
- HTTP now retains a full `RetryError<HttpError>` as the immediate retry source.
  CAS retains completion diagnostics in its getter and consuming parts. EventBus
  wraps domain errors only when completion diagnostics are nonempty; this wrapper
  is terminal under its default rule. Built-in adapter success paths currently
  register no completion observers and explicitly discard empty diagnostics.

Existing admission accounting remains: finish each `RetryBudget` token before
starting the next attempt; tokens cannot cross budgets. `on_before_attempt` is
pre-admission and `on_retry_scheduled` does not guarantee another operation.
For earlier upgrades, use `on_before_attempt` / `BeforeAttempt` and handle the
`Success` / `TerminalFailure` completion phases.

## Learn more

- [User guide](doc/user_guide.md): cancellation, hard timeouts, hints, serde, clocks, diagnostics, and troubleshooting
- [Design](doc/design.md): admission, priorities, ownership, and worker protocol
- [Rust API documentation](https://docs.rs/qubit-retry)
- [中文 README](README.zh_CN.md)

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
