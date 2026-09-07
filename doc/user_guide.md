# Qubit Retry User Guide

[简体中文](user_guide.zh_CN.md) · [README](../README.md) · [Design](design.md)

## Audience and scenario

This guide covers **qubit-retry 0.23** for client and storage authors. A snapshot
reader must recover from temporary unavailability while preserving permanent
errors, respecting shutdown, and exposing the final outcome. Success means one
returned snapshot with an accurate admitted-attempt count; retries must not hide
an uncertain side effect. The [README fixture](../README.md#quick-start) provides
the complete minimal implementation and returns `RetryError<io::Error>` intact.

## Concepts and setup

`RetryPolicy` is reusable configuration; `Retry` adds ordered rules and observers.
Each `run` creates a fresh budget, backoff index, and context. A policy's
`max_attempts` includes the first operation. The default is three attempts,
immediate backoff, no elapsed limits, and retryable application errors.
`AttemptFailure` describes one failed operation; `RetryErrorReason` describes why the
whole flow stopped. A context's `attempts()` counts admissions, whereas
`current_attempt()` may describe a pre-admission callback or retained active work.

Install with Rust 1.94 or newer. Default features are empty:

<!-- retry-example: kind=cargo features=none -->
```toml
[dependencies]
qubit-retry = "0.23"
```

Enable `tokio` for async execution and `serde` for configuration wire formats:

<!-- retry-example: kind=cargo features=tokio,serde -->
```toml
[dependencies]
qubit-retry = { version = "0.23", features = ["tokio", "serde"] }
```

Guide examples additionally use `qubit-clock` 0.13 with `test-util` for manual
time, `serde_json` 1 for JSON, and Tokio 1.52 or newer with `rt`, `macros`, `time`
for the async binary. The document checker supplies these fixture dependencies.

## Core workflow: classify, run, retain the result

Use a rule to retry only transient snapshot read failures. The first
non-`UseDefault` decision wins. `Abort` stops after retaining the failure;
`Retry`, `RetryWithHint`, and `RetryWithJitteredHint` still cannot bypass admission
limits. When all rules delegate, application errors retry, attempt timeouts stop
as `TimedOut`, and captured worker panics stop as `Aborted`.

Select the mode that matches the actual operation:

| Operation | Mode | Constraints |
| --- | --- | --- |
| A bounded blocking read on the current thread | `sync()` | Cannot interrupt the closure; borrowed state and `FnMut` are supported |
| A cancellation-safe asynchronous client call | `asynchronous()` | Tokio feature/runtime; operation futures need not be `Send` or static |
| A blocking call that cooperatively exits on a token | `worker()` | Operation: `Fn + Send + Sync + 'static`; result/error: `Send + 'static`; worker and reaper per attempt |

Retain all three parts of success or error. Use `map_error` when only converting
the application error type. Its `FnOnce` mapper runs once if an application error
exists and zero times otherwise; a mapper panic propagates. It preserves terminal
classification, context and completion diagnostics without imposing extra
`Clone`, `Send` or `'static` bounds. Merely cloning `Retry` does not clone `E`.

## Timing and shutdown

`operation_time_budget` sums admitted-operation durations. `total_time_budget`
measures monotonic flow time, including control callbacks and sleeps. Both limit
future admission; an admitted success remains success after crossing them.
Async/worker `hard_attempt_timeout` and `hard_flow_timeout` bound cooperative waits, not
arbitrary synchronous work. A callback or blocking future poll can delay every
check. Completion callbacks are outside both elapsed accounting and timeout control.

For the same-thread reader, cancellation after an error prevents the next read:

<!-- retry-example: kind=run features=none -->
```rust
use qubit_retry::Retry;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryPolicy;

fn main() {
    let token = RetryCancellationToken::new();
    let retry = Retry::<&str>::builder(RetryPolicy::builder().build().unwrap()).build();
    let error = retry.sync().cancellation_token(token.clone()).run(|| {
        token.cancel();
        Err::<(), _>("temporary read failure")
    }).unwrap_err();
    assert_eq!(error.context().attempts(), 1);
    assert!(matches!(error.reason(), RetryErrorReason::Cancelled {
        phase: RetryCancellationPhase::Backoff
    }));
}
```

An `Ok` with a valid completion clock wins over sync cancellation. Tokens share
permanent state across clones; there is no reset, parent tree, or built-in deadline.
At a normal control callback return, clock refresh precedes cancellation; an
invalid sample becomes a clock infrastructure error. Callback panic remains the
primary error with best-effort timing. Backoff cancellation wins over a ready delay.

For a pending async read, cancellation drops the operation future:

<!-- retry-example: kind=run features=tokio -->
```rust
use std::future;
use std::time::Duration;

use qubit_retry::Retry;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryPolicy;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let retry = Retry::<&str>::builder(RetryPolicy::builder().build().unwrap()).build();
    let token = RetryCancellationToken::new();
    let operation_token = token.clone();
    let error = retry.asynchronous()
        .hard_attempt_timeout(Duration::from_secs(2))
        .hard_flow_timeout(Duration::from_secs(5))
        .cancellation_token(token)
        .run(move || {
            operation_token.cancel();
            future::pending::<Result<(), &str>>()
        }).await.unwrap_err();
    assert!(matches!(error.reason(), RetryErrorReason::Cancelled {
        phase: RetryCancellationPhase::Attempt
    }));
}
```

If result, cancellation, and timeout are ready in the same poll, async prefers
result, then cancellation, then timer. A selected error still enters failure
handling; a selected success needs valid completion accounting. Equal attempt
and flow deadlines are attributed to the attempt. Dropping a future does not
roll back an HTTP request or transaction: use idempotency keys, transactional
boundaries, or application-specific reconciliation before retrying side effects.

For blocking work, cooperate with the per-attempt token:

<!-- retry-example: kind=run features=none -->
```rust
use std::time::Duration;

use qubit_retry::Retry;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryPolicy;

fn main() {
    let retry = Retry::<&str>::builder(RetryPolicy::builder().build().unwrap()).build();
    let token = RetryCancellationToken::new();
    let operation_token = token.clone();
    let error = retry.worker()
        .cancellation_token(token)
        .cancellation_grace(Duration::from_secs(1))
        .run(move |attempt| {
            operation_token.cancel();
            while !attempt.is_cancelled() {
                std::thread::yield_now();
            }
            Err::<(), _>("cancelled read")
        }).unwrap_err();
    assert!(matches!(error.reason(), RetryErrorReason::Cancelled { .. }));
}
```

The flow checks cancellation, then timer, before accepting both an operation
result and completed join. Join includes thread-local storage destruction.
`cancellation_grace` uses real monotonic time even with an injected timer. If
worker/TLS cleanup exceeds grace, `Infrastructure::WorkerStillRunning` retains
its `WorkerStopTrigger`; this flow will not start another attempt. The worker
and detached reaper can remain alive. No timeout/cancellation means exit waiting
may be unbounded. There is no force-kill facility or worker pool.

## Server hints, jitter, and reconnects

Exponential backoff avoids repeatedly hitting a busy server; jitter spreads
concurrent clients. Full jitter samples zero through the selected delay;
bounded jitter uses a validated ratio in `[0, 1]`. Uniform sampling preserves
exact endpoints and never exceeds its interval for a valid random source.

`BackoffRequest::hint` avoids jittering the server value; `jittered_hint` permits
it. `prefer_retry_after` selects the hint, `use_retry_after_as_minimum` combines
it with backoff, and `ignore_retry_after` ignores it. `maximum_delay()` is only
the base strategy maximum. `limit_delay()` applies last, after hint resolution
and jitter; this example deliberately truncates a one-second minimum:

<!-- retry-example: kind=run features=none -->
```rust
use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let policy = BackoffPolicy::exponential(Duration::from_millis(50), 2.0, Duration::from_secs(2))?
        .with_full_jitter()
        .use_retry_after_as_minimum()
        .limit_delay(Duration::from_millis(500));
    let mut state = policy.start();
    let step = state.next(BackoffRequest::hint(Duration::from_secs(1)));
    assert_eq!(step.effective_delay(), Duration::from_millis(500));
    assert_eq!(step.retry_index(), 1);
    state.reset();
    assert_eq!(state.retry_index(), 0);
    Ok(())
}
```

If the server's minimum is mandatory, do not configure a smaller final cap.
Stop when the remaining budget cannot accommodate the minimum. `BackoffStep`
retains base/effective delay and source; reset state only for a new flow or after
a connection meets your stability criterion. SSE in rs-http independently
combines `RetryBudget` and `BackoffState`, then enforces its own minimum 1ms and
server-delay cap; it does not create retry completion diagnostics.

## Standalone budgets and deterministic clocks

Custom reconnect loops can account for admitted work without using an executor.
Finish every token before beginning another attempt; tokens are bound to their
original budget. A dropped token leaves that flow closed to further admission.
`check_retry_after` checks the proposed delay, but the next admission rechecks
after actual sleep. This manual-clock example needs no filesystem or network:

<!-- retry-example: kind=run features=none -->
```rust
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_retry::RetryBudget;
use qubit_retry::RetryPolicy;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder().max_attempts(2).build()?;
    let mut budget = RetryBudget::new(clock.as_ref(), *policy.limits())?;
    let attempt = budget.begin_attempt()?;
    clock.advance(Duration::from_secs(2))?;
    let snapshot = budget.finish_attempt(attempt)?;
    assert_eq!(snapshot.attempts(), 1);
    assert_eq!(snapshot.operation_elapsed(), Duration::from_secs(2));
    budget.check_retry_after(Duration::from_secs(1))?;
    clock.advance(Duration::from_secs(1))?;
    assert_eq!(budget.snapshot()?.total_elapsed(), Duration::from_secs(3));
    Ok(())
}
```

Inject a timer from the same manual clock into an executor with `.timer(...)`.
Advance only after the operation or timer-registration boundary under test is
observable; do not synchronize correctness tests by sleeping real time.
Clock domain changes and regression return structured errors. Invalid timing
must not be replaced with fabricated elapsed values. Worker cleanup grace still
uses real time and therefore needs an explicit release/join protocol in tests.

## Configuration JSON

The serde feature covers validated policy/limit/backoff configuration only.
Durations use `seconds` and `nanoseconds` (less than one billion); strategy tags
are stable snake_case. Unknown fields and invalid combinations are rejected.
Omitted optional elapsed limits mean no limit; `delay_limit` is optional and
applies after hints and jitter. A bounded jitter ratio must be numeric and present;
`null` is not absence. Runtime callbacks, results, and errors are not a wire format.

<!-- retry-example: kind=run features=serde -->
```rust
use qubit_retry::RetryPolicy;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = r#"{
        "max_attempts": 4,
        "operation_time_budget": null,
        "total_time_budget": {"seconds": 10, "nanoseconds": 0},
        "backoff": {
            "strategy": {"type": "fixed", "delay": {"seconds": 0, "nanoseconds": 50000000}},
            "jitter": {"type": "none"},
            "retry_after": "at_least_backoff"
        }
    }"#;
    let policy: RetryPolicy = serde_json::from_str(json)?;
    assert_eq!(policy.limits().max_attempts().get(), 4);
    let encoded = serde_json::to_string(&policy)?;
    assert_eq!(serde_json::from_str::<RetryPolicy>(&encoded)?, policy);
    Ok(())
}
```

## Observers and completion diagnostics

Control callbacks execute before admission, after a committed failure, during
rule selection, and after eligible retry scheduling. A panic stops controls as
`CallbackFailed`, retaining kind/index/phase/payload. `on_retry_scheduled` means
only that the delay fits at that point; subsequent callbacks and admission
recheck cancellation, budgets, and timeout. Count real work with terminal attempts.

`on_success` / `on_terminal_failure` run once for each returned result, including
zero-attempt failures. Their synchronous work must be short and nonblocking.
They see a frozen outcome; their panics attach diagnostics and later completion
observers still run. This example intentionally panics in the audit sink; a panic
hook may print a message even though the retry result retains the diagnostic:

<!-- retry-example: kind=run features=none -->
```rust
use qubit_retry::Retry;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;

struct Audit;
impl RetryObserver<&'static str> for Audit {
    fn on_terminal_failure(&self, _: &RetryErrorReason, _: &RetryContext) {
        panic!("audit sink unavailable");
    }
}

fn main() {
    let retry = Retry::builder(RetryPolicy::builder().max_attempts(1).build().unwrap())
        .observer(Audit).build();
    let error = retry.sync().run(|| Err::<(), _>("offline")).unwrap_err();
    let mapped = error.map_error(String::from);
    let (reason, failure, context, diagnostics) = mapped.into_parts();
    assert!(matches!(reason, RetryErrorReason::Aborted));
    assert_eq!(failure.and_then(|failure| failure.error()).map(String::as_str), Some("offline"));
    assert_eq!(context.attempts(), 1);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].phase(), RetryCallbackPhase::TerminalFailure);
}
```

Sync/async operation panic, dropping an async run future, and process abort do
not guarantee completion. Only worker mode captures operation panics. Captured
static strings, owned strings, and non-string payloads are distinguished. If
non-string payload destruction panics, classification stays `NonString`; only
that secondary payload is leaked to avoid recursive destruction. This protection
does not cover arbitrary business-value destructors, panic hooks, or aborts.

## Adapter boundaries and migration

For 0.23, `RetrySuccess::into_parts` returns three values and `RetryError::into_parts` returns four values
including diagnostics. Remove `into_parts_with_diagnostics`; it has no alias.
The old `into_value` / `into_failure` names become explicit
`into_value_discarding_diagnostics` / `into_failure_discarding_diagnostics`, which
also discard context. Keep defaults and mode-specific priorities as described above.
Normal callback-return clock validation now precedes returned Abort decisions as
well as cancellation; callback panic preserves its own primary classification.

HTTP wraps every retry terminal with its complete `RetryError<HttpError>` source
and preserves request/status/preview/hint/redaction fields; the original HTTP
error and backend source remain reachable. CAS projects timeout state and domain
failure while retaining completion diagnostics in `CasError`. EventBus preserves
its old mapping for empty diagnostics; a nonempty set creates the terminal
`RetryCompletionDiagnostics` wrapper with a domain source and shared context.
These adapters currently install no completion observers on successful flows
and explicitly discard empty success diagnostics; future observer registration
must provide a success diagnostic outlet at the same time.

## Troubleshooting and limits

| Symptom | Check and response |
| --- | --- |
| No operation ran | Inspect `attempts`, callback phase, zero budgets, cancellation and timer registration failures |
| Fewer calls than scheduled notifications | Scheduling is provisional; inspect the terminal limit/timeout and callback elapsed time |
| A synchronous call exceeds the budget | Budgets are soft; bound the operation or select a suitable async/worker API |
| A cancelled operation still has effects | Cancellation stops waiting, not committed external effects; reconcile/idempotently retry |
| `Infrastructure::Clock` | Check timer/clock domain and monotonicity; do not reuse tokens or invent replacement timestamps |
| `WorkerStillRunning` | Inspect the trigger and operation/TLS exit protocol; do not automatically start a replacement flow |
| Successful value has diagnostics | Inspect completion observers; their failures do not negate the value |
| Retry-After is shorter than expected | Inspect final cap, hint strategy, jitter permission and downstream SSE constraints |

There is no built-in circuit breaker, cancellation hierarchy, alternative async
runtime, or thread-pool execution. Choose a bounded number of attempts and budgets
for remote workloads; evaluate benchmark results for your actual operation costs.
Run `./align-ci.sh` then `./ci-check.sh` for repository changes. The project CI
executes every annotated Rust/Cargo block in both languages from temporary path
consumer crates; rustdoc examples are checked separately.

## Further reading

[README](../README.md) · [Design](design.md) · [API](https://docs.rs/qubit-retry) · [简体中文](user_guide.zh_CN.md)
