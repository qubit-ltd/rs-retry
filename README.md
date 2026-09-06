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
qubit-retry = "0.21"
```

Tokio execution and stable configuration serialization are opt-in:

```toml
qubit-retry = { version = "0.21", features = ["tokio", "serde"] }
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

### Synchronous cancellation

The same-thread facade checks cancellation before admission, after a failed
operation, and during backoff. It cannot interrupt a running closure. Keep each
operation bounded; an `Ok` returned by that operation still wins over cancellation.
This example cancels during a failed operation and therefore admits no retry:

```rust
use qubit_retry::{Retry, RetryCancellationToken, RetryFailure, RetryPolicy};

let token = RetryCancellationToken::new();
let policy = RetryPolicy::builder().build().expect("valid policy");
let retry = Retry::<&'static str>::builder(policy).build();
let mut calls = 0;
let error = retry.sync().cancellation_token(token.clone()).run(|| {
    calls += 1;
    token.cancel();
    Err::<(), _>("temporarily unavailable")
}).expect_err("cancellation stops further attempts");
assert_eq!(calls, 1);
assert!(matches!(error.failure(), RetryFailure::Cancelled { .. }));
```

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
Each attempt uses a worker and a reaper thread. The reaper joins the worker;
the calling thread waits for confirmed thread exit, including thread-local
storage (TLS) destructors, under the timeout/cancellation protocol. Returning an
operation result alone does not prove exit. An uncooperative operation or TLS
destructor may leave both threads alive after the grace period. No next attempt
starts in that flow. Without a timeout or cancellation, this wait can be unbounded.

## Why this project exists

Retry loops often spread attempt counting, sleeps, shutdown checks, and error
conversion across call sites. That makes it easy to lose the last application
error or report a timeout, cancellation, and callback panic as the same opaque
failure. Qubit Retry keeps these decisions in one policy-driven flow and
returns a coherent `RetryContext` with every success or terminal error.

## What it provides

- `RetryPolicy` combines validated attempt, operation-time, and total-time
  budgets with reusable immediate, fixed, uniform, or exponential backoff.
- `Retry::sync()` runs same-thread closures with cooperative cancellation at
  safe boundaries. It exposes no hard timeout for a running closure.
- `Retry::asynchronous()` supports attempt/flow timeouts and cancellation when
  the `tokio` feature is enabled; Tokio is the only async runtime integration.
- `Retry::worker()` captures panics, preserves the stop trigger, and never
  starts another worker while an earlier uncooperative worker may still run.
- `RetryFailure` distinguishes aborted, exhausted, timed-out, cancelled,
  callback-failed, and infrastructure terminals. `AttemptFailure` retains an
  application error, timeout scope, or stable panic payload.
- Ordered rules and control observers fail closed on callback panic. Completion
  observers preserve the main result and attach diagnostics. Both retain callback
  kind, registration index, lifecycle phase, and panic payload classification.
- `BackoffState` is reusable for reconnect loops such as SSE and supports
  server hints, jitter, and overflow-safe exponential growth.
- The optional `serde` feature serializes configuration types only; runtime
  results, errors, and callback state are deliberately not wire protocols.

Budgets only control whether another attempt may start. If an admitted
operation succeeds after crossing a budget, the success is returned. A retry
policy never forcefully kills a synchronous operation or an uncooperative
worker thread. Cancellation tokens are not resettable and do not provide parent
trees or built-in deadlines.

### Defaults and failure classification

`RetryPolicy::builder().build()` allows **three attempts in total**: the initial attempt
and at most two immediate retries, with no elapsed budget. It does not mean
three additional retries. Without a rule, any application `Err(E)` is retryable;
classify permanent failures explicitly when that default is too broad.

| Event | Default behavior | Rule override |
| --- | --- | --- |
| Application `Err(E)` | Retry within the budgets; exhaustion retains the last error | `Abort`, `Retry`, or `RetryWithHint` |
| Captured attempt timeout | Stop as `TimedOut` | May request retry while the flow remains eligible |
| Captured operation panic (worker/async) | Stop as `Aborted` | May request retry while the flow remains eligible |
| Same-thread operation panic | Unwind to the caller | Not passed to rules |
| Attempt/elapsed budget exhausted | Stop as `Exhausted` | Cannot bypass admission limits |
| Flow timeout | Stop as `TimedOut` | Cannot extend the flow deadline |
| Cancellation | Stop as `Cancelled` at the facade's cancellation boundaries | Cannot reset the token |
| Rule or control-observer panic | Stop as `CallbackFailed` | Later control callbacks do not recover it |
| Clock, timer, spawn, channel, or active-worker failure | Stop as `Infrastructure` | Not retried |
| Completion-observer panic | Keep the success or terminal error; attach diagnostics | All later completion observers still run |

Rules run in registration order; the first decision other than `UseDefault`
wins. If every rule delegates, the defaults above apply.

### Budgets and timeouts

| Setting | What it measures | Effect |
| --- | --- | --- |
| `max_operation_elapsed` | Cumulative admitted-operation time | Soft continuation budget; cannot interrupt an admitted operation |
| `max_total_elapsed` | Monotonic flow time, including backoff and control callbacks | Soft continuation budget; a completed success remains successful |
| `attempt_timeout` (async/worker) | One admitted attempt | Stops waiting/drops the async attempt or asks the worker to exit |
| `flow_timeout` (async/worker) | The whole execution flow | Bounds active-attempt and backoff waits; worker cleanup may add `cancellation_grace` |

Timeouts cannot preempt synchronous callback work or an async future that blocks
its polling thread. Worker cleanup uses real time even with an injected timer.
Completion callbacks run after the terminal context is frozen; their time is
excluded from that context and is not bounded by retry timeouts.

### Completion diagnostics and error mapping

Implement `RetryObserver::on_success` and `on_terminal_failure` for one completion
notification per `run` result, including failures before the first admission.
Observers run synchronously in registration order and must remain short and
nonblocking. Their panics do not change the frozen result, schedule a retry, or
cause a second terminal notification. Read `completion_callback_failures()` on
either `RetrySuccess` or `RetryError` before consuming it:

```rust
use qubit_retry::{Retry, RetryCallbackPhase, RetryContext, RetryFailure};
use qubit_retry::{RetryObserver, RetryPolicy};

struct CompletionAudit;
impl RetryObserver<&'static str> for CompletionAudit {
    fn on_terminal_failure(&self, _: &RetryFailure<&'static str>, _: &RetryContext) {
        panic!("audit sink unavailable");
    }
}

let policy = RetryPolicy::builder().build().expect("valid policy");
let retry = Retry::<&'static str>::builder(policy)
    .observer(CompletionAudit)
    .build();
let error = retry.sync().run(|| Err::<(), _>("offline")).unwrap_err();
let mapped = error.map_error(String::from);
assert_eq!(mapped.last_error().map(String::as_str), Some("offline"));
assert_eq!(mapped.completion_callback_failures().len(), 1);
assert_eq!(mapped.completion_callback_failures()[0].phase(), RetryCallbackPhase::TerminalFailure);
let (_failure, context, diagnostics) = mapped.into_parts_with_diagnostics();
assert_eq!(context.attempts(), 3);
assert_eq!(diagnostics.len(), 1);
```

`AttemptFailure::map_error`, `RetryFailure::map_error`, and `RetryError::map_error`
consume only the retained application error. An `FnOnce` mapper runs once when
that error exists, otherwise zero times; it needs no `Clone`, `Send`, or `'static`
bound. Classification and terminal data are preserved; `RetryError::map_error`
also preserves context and completion diagnostics. A mapper panic propagates.
`into_parts()` discards completion diagnostics; `into_parts_with_diagnostics()`
retains them. `RetrySuccess::into_value()` and `RetryError::into_failure()` also
discard the extra diagnostics. Operation unwinding, dropping an async `run`
future, and process abort do not guarantee a completion notification.

## Learn more

- [Rust API documentation](https://docs.rs/qubit-retry)
- [中文 README](README.zh_CN.md)
- [Repository](https://github.com/qubit-ltd/rs-retry)

## Migrating from 0.20 to 0.21

Update direct `qubit-retry` dependencies to `0.21` and regenerate the affected
lockfile entries together with the HTTP, CAS, and EventBus adapters. Existing
observer implementations compile unchanged because completion methods default
to no-ops. Exhaustive matches on `RetryCallbackPhase` must now handle `Success`
and `TerminalFailure`. Keep a fallback when matching non-exhaustive error types
from adapters. Tokio and serde remain opt-in; the default feature set is empty.

Review result consumption where diagnostics matter: replace `into_parts()` with
`into_parts_with_diagnostics()`, or inspect `completion_callback_failures()`
first. Prefer `map_error` for pure application-error conversion; adapters that
also change terminal semantics still need their domain conversion.

### Contracts retained from 0.20

`RetryBudget` and all execution facades now share admission accounting. Its
`begin_attempt`, `finish_attempt`, `snapshot`, and `check_retry_after` methods
return `RetryBudgetError`; exhaustion is `RetryBudgetError::Exhausted(kind)`.
`check_retry_after` requires a mutable budget. Finish each token before starting
another attempt, and never pass a token to another budget. Invalid clocks return
errors. Soft elapsed budgets no longer require a representable absolute deadline.

`on_retry_scheduled` runs only when the selected delay currently fits the budgets.
Callbacks and the eventual admission still recheck limits and cancellation; this
event does not guarantee another operation. `on_attempt_started` remains a
pre-admission notification, while terminal `context.attempts()` counts admissions.

`BackoffPolicy::maximum_delay()` describes the base strategy before jitter and
hints. Use `.limit_delay(duration)` to cap the final delay after both. The optional
serde field `delay_limit` uses the existing `{seconds, nanoseconds}` duration
representation; older configurations without that field remain accepted.

Worker attempt and flow timeouts now use the injected timer, including manual
time. `cancellation_grace` always uses real time to bound OS-thread cleanup. A
failed active timer requests cancellation; an uncooperative worker reports
`WorkerStillRunning` with `WorkerStopTrigger::TimerFailure` and is never retried
within that flow.

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
