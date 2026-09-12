# Qubit Retry User Guide

[简体中文](user_guide.zh_CN.md) · [README](../README.md) · [API reference](https://docs.rs/qubit-retry/0.24.0/qubit_retry/)

This guide covers **qubit-retry 0.24.0** and requires **Rust 1.94+**. It is for
Rust developers adding retries to clients, storage access, or reconnect loops.
You supply the operation and decide which failures are safe to retry.

Start with a simulated storage read: two timeouts followed by success. The
library controls retries; the simulated responses let you run the example
without an external service.
Every Rust block is a complete program that can replace `src/main.rs` in a test app.

## Contents

- [Quick start: read a snapshot](#quick-start-read-a-snapshot)
- [Understand a retry flow](#understand-a-retry-flow)
- [Choose an execution mode](#choose-an-execution-mode)
- [Choose which failures to retry](#choose-which-failures-to-retry)
- [Configure backoff and server hints](#configure-backoff-and-server-hints)
- [Set budgets and timeouts](#set-budgets-and-timeouts)
- [Run async operations](#run-async-operations)
- [Cancel work and shut down](#cancel-work-and-shut-down)
- [Handle results and observe execution](#handle-results-and-observe-execution)
- [Load JSON configuration](#load-json-configuration)
- [Use standalone budgets and test clocks](#use-standalone-budgets-and-test-clocks)
- [Troubleshoot](#troubleshoot)

## Quick start: read a snapshot

Add the dependency to your application's `Cargo.toml`:

<!-- retry-example: kind=cargo features=none -->
```toml
[dependencies]
qubit-retry = "0.24"
```

A storage service is temporarily unavailable. Retry its read and return the
snapshot when it recovers. The separate `read_snapshot` function simulates two
failed calls followed by success. Its counter only generates test responses;
it does not implement retry control. `RetryConfig` sets the attempt limit,
waiting time, and exponential backoff; the rule classifies retryable errors.

<!-- retry-example: kind=run features=none -->
```rust
use std::io;
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;

// Simulate storage: the first and second calls time out; the third succeeds.
// simulated_calls only produces test responses; it does not limit or schedule retries.
// Replace this function with a real storage read, without the simulation counter.
fn read_snapshot(simulated_calls: &mut u32) -> io::Result<&'static str> {
    *simulated_calls += 1;
    if *simulated_calls <= 2 {
        Err(io::Error::new(io::ErrorKind::TimedOut, "storage unavailable"))
    } else {
        Ok("snapshot-v2")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = RetryConfig::builder()
        .max_attempts(5)
        .backoff(BackoffPolicy::exponential(
            Duration::from_millis(100),
            2.0,
            Duration::from_secs(1),
        )?)
        .rule(|failure: &AttemptFailure<io::Error>, _: &RetryContext| {
            match failure {
                AttemptFailure::Error(error) if error.kind() == io::ErrorKind::TimedOut => {
                    RetryDecision::Retry
                }
                _ => RetryDecision::Abort,
            }
        })
        .build()?;

    let mut simulated_calls = 0;
    let success = Retry::new(&config).run(|| read_snapshot(&mut simulated_calls))?;
    assert_eq!(*success.value(), "snapshot-v2");
    assert_eq!(success.context().attempts(), 3);
    assert!(success.completion_callback_failures().is_empty());
    println!("{}, attempts={}", success.value(), success.context().attempts());
    Ok(())
}
```

Add the dependency to your application's `Cargo.toml`, put the complete example
in `src/main.rs`, and run `cargo run`:

```text
snapshot-v2, attempts=3
```

The library calls `read_snapshot`, waits 100 ms after its failure, and calls it
again. After the second failure it waits 200 ms; the third call succeeds.
**The fixture chooses responses; the policy controls further attempts and waits.**
`max_attempts(5)` includes the first call and allows at most four retries.
`context.attempts()` is the library's recorded execution count.

Replace `read_snapshot` with your real client call and remove the simulation
counter; no application retry loop or sleep is needed. Set `max_attempts` to 2
and the library returns `Exhausted { limit: Attempts }` after the second failure,
without making the fixture's third call. Unclassified errors abort by default.

| Scenario to verify | Change | Result |
| --- | --- | --- |
| Recovery after temporary errors | Run the example as written | Success on the third attempt |
| Attempts exhausted | Change `max_attempts(5)` to `max_attempts(2)` | `Exhausted` after two failures; no further operation call |
| Permanent error | Make the simulated read return `PermissionDenied` | `Aborted` after the first failure |

These changes exercise different branches. In an application, retain the full
`RetryError<io::Error>`: inspect `reason()` for the stop reason and `last_error()`
for the original I/O error. Every `run` starts fresh; reusing `Retry` does not
require resetting the library's counters.

## Understand a retry flow

| Concept | Meaning |
| --- | --- |
| Attempt | One admitted operation, including the first call |
| Retry | A subsequent attempt after a failure |
| Backoff | The delay before a retry |
| Admission | The check that permits an attempt to start under the remaining limits |
| `RetryPolicy` | Validated attempt limits, elapsed budgets, and backoff configuration |
| `RetryConfig<E>` | Policy, rules, observers, and fallback for error type `E` |
| `Retry` / `AsyncRetry` / `TokioRetry` / `WorkerRetry` | Executors that run operations with a shared `RetryConfig` |
| `RetryContext` | A snapshot of counts, elapsed time, and the current phase's attempt/delay information |

The normal path is:

```text
check limits → before-attempt callback → recheck → admit and run
                                                   ├─ success → complete
                                                   └─ failure → observe → apply rules
                                                                            ├─ stop → complete
                                                                            └─ retry → check delay budget
                                                                                       → observe schedule → wait → recheck
```

Cancellation, hard timeouts, or infrastructure failures can stop execution at
control boundaries. A scheduled retry is provisional: later checks can still
prevent it from starting. `context.attempts()` counts admissions;
`current_attempt()` may also describe an upcoming attempt during a callback.

Defaults are **three total attempts, immediate backoff, no elapsed budgets, and
abort for unclassified application errors**. A default configuration alone does not
make failures retryable. Each `run` creates fresh state. Cloning `RetryConfig` shares
rules and observers without requiring the application error to implement `Clone`.

## Choose an execution mode


| Entry point | Feature | Operation requirements | Where it runs |
| --- | --- | --- | --- |
| `Retry::new(&config)` | None | `FnMut() -> Result<T, E>`; can borrow local state | Calling thread |
| `AsyncRetry::new(&config)` | `async` | `FnMut() -> Fut`; futures need not be `Send` or `'static` | Any executor polling the future; default timer is `StdTimer` |
| `TokioRetry::new(&config)` | `tokio` | `FnMut() -> Fut`; futures need not be `Send` or `'static` | Tokio runtime |
| `WorkerRetry::new(&config)` | `worker` | `Fn(AttemptCancellationToken) -> Result<T, E> + Send + Sync + 'static`; `T` and `E`: `Send + 'static` | Dedicated worker thread per attempt |

The execution APIs require `E: 'static`, including in sync/async mode; that does
not require their operation closures to own all captured state. Rules and
observers are shared `Send + Sync + 'static` callbacks.

Enable runtime-independent async execution with:

<!-- retry-example: kind=cargo features=async -->
```toml
[dependencies]
qubit-retry = { version = "0.24", features = ["async"] }
```

`AsyncRetry` returns a standard `Future` and uses `StdTimer` by default. The
executor is supplied by the application. To use Tokio's native timer instead,
enable `tokio`:

<!-- retry-example: kind=cargo features=tokio -->
```toml
[dependencies]
qubit-retry = { version = "0.24", features = ["tokio"] }
```

The async examples also need a direct Tokio dependency. Add it with:

```bash
cargo add tokio@1.52 --features rt,macros,time
```

For blocking worker examples use:

<!-- retry-example: kind=cargo features=worker -->
```toml
[dependencies]
qubit-retry = { version = "0.24", features = ["worker"] }
```

Features can be combined in the dependency's `features` array. `WorkerRetry::new(&config).run()`
blocks its caller while coordinating the worker; it is not an async thread-pool API.

## Choose which failures to retry

Rules run in registration order. The first decision other than `UseDefault`
wins; `UseDefault` lets the next rule decide, then uses the fallback if every
rule delegates.

| Decision | Effect |
| --- | --- |
| `RetryDecision::Abort` | Stop and retain the failure |
| `RetryDecision::Retry` | Request another attempt using policy backoff |
| `RetryDecision::RetryWithHint(delay)` | Supply a delay hint protected from jitter, but still subject to the final cap |
| `RetryDecision::RetryWithJitteredHint(delay)` | Supply a hint that allows configured jitter |
| `RetryDecision::UseDefault` | Continue rule evaluation |

If every application error from a particular operation is retryable, opt in
explicitly with `RetryFallback::Retry`:

<!-- retry-example: kind=run features=none -->
```rust
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = RetryConfig::<&str>::builder()
        .max_attempts(2)
        .fallback(RetryFallback::Retry)
        .build()?;
    let error = Retry::new(&config).run(|| Err::<(), _>("temporarily unavailable")).unwrap_err();
    assert!(matches!(error.reason(), RetryErrorReason::Exhausted { .. }));
    assert_eq!(error.context().attempts(), 2);
    Ok(())
}
```

Fallback applies to application errors. A captured attempt timeout defaults to
`TimedOut`, and a captured worker panic defaults to `Aborted`, even with
`RetryFallback::Retry`. A rule can explicitly request a retry for either, within
remaining limits. Flow timeout, cancellation, callback failure, and infrastructure
failure are terminal and cannot be recovered by a retry rule.

An `io::ErrorKind::TimedOut` returned by a client is an application error. It is
different from `AttemptFailure::TimedOut`, which is created by an execution timeout.
The quick start classifies the client error `TimedOut`; it does not classify
an executor timeout.
For writes, establish whether repeating the operation is safe before adding a
retry rule: cancelling a wait cannot undo a committed write.

## Configure backoff and server hints

Set backoff with `RetryConfig::builder().backoff(...)`. Constructors do not sleep;
the executor waits after a failed attempt when another retry is permitted.

| Constructor | Base delay | Suitable use |
| --- | --- | --- |
| `BackoffPolicy::immediate()` | Zero | Fast local retries or examples |
| `BackoffPolicy::fixed(delay)` | Same delay each time | A known polling interval |
| `BackoffPolicy::uniform(min, max)?` | Random delay within inclusive bounds | Distributing retries over a chosen interval |
| `BackoffPolicy::exponential(initial, multiplier, max)?` | Grows from `initial`, capped at `max` | Repeated temporary service failures |

For example, 50 ms with multiplier 2 and a 2 s maximum gives base delays of
50, 100, 200, 400, 800, 1600, 2000, 2000 ms. The first delay is before the
second attempt. `uniform` requires `min <= max`; exponential requires
`initial <= max` and a finite multiplier of at least 1.

`with_full_jitter()` samples from zero to the selected delay.
`with_bounded_jitter(ratio)?` varies it symmetrically by a finite ratio in `[0, 1]`;
for example, 0.2 gives approximately 80%–120% of the delay. Constructors start
without jitter. Use `without_jitter()` to remove it.

An application can parse a server's retry instruction and return
`RetryWithHint(duration)` from its rule. Qubit Retry accepts a `Duration`; it does
not parse HTTP headers. These policies determine how to combine the hint:

| Method | With a hint |
| --- | --- |
| `use_retry_after_as_minimum()` (default) | Take the larger of the hint and policy delay after permitted jitter |
| `prefer_retry_after()` | Use the hint after any permitted hint jitter |
| `ignore_retry_after()` | Use only the policy delay |

### Retry a busy service and log progress

This service supplies a 10 ms retry hint. The rule passes it to the executor;
the observer reports the chosen delay and successful completion. The default
hint policy selects the larger of that hint and the 5 ms fixed delay.

<!-- retry-example: kind=run features=none -->
```rust
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffStep;
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryObserver;

#[derive(Debug)]
enum ReadError {
    Busy { retry_after: Duration },
}

struct ReadLog;
impl RetryObserver<ReadError> for ReadLog {
    fn on_retry_scheduled(&self, step: &BackoffStep, context: &RetryContext) {
        println!("retry after {:?}, attempts={}", step.effective_delay(), context.attempts());
    }

    fn on_success(&self, context: &RetryContext) {
        println!("read complete, attempts={}", context.attempts());
    }
}

fn main() -> Result<(), qubit_retry::RetryPolicyError> {
    let config = RetryConfig::builder()
        .max_attempts(3)
        .backoff(BackoffPolicy::fixed(Duration::from_millis(5)))
        .rule(|failure: &AttemptFailure<ReadError>, _: &RetryContext| {
            match failure {
                AttemptFailure::Error(ReadError::Busy { retry_after }) => {
                    RetryDecision::RetryWithHint(*retry_after)
                }
                _ => RetryDecision::Abort,
            }
        })
        .observer(ReadLog)
        .build()?;
    // Simulated server responses; the operation does not implement retry logic.
    let mut responses = [
        Err(ReadError::Busy { retry_after: Duration::from_millis(10) }),
        Ok("snapshot-v2"),
    ].into_iter();
    let success = Retry::new(&config)
        .run(|| responses.next().expect("fixture has two responses"))
        .expect("read recovers after server hint");
    assert_eq!(*success.value(), "snapshot-v2");
    assert_eq!(success.context().attempts(), 2);
    Ok(())
}
```

It prints `retry after 10ms, attempts=1` and then `read complete, attempts=2`.
Replace the simulated `Busy` response with a delay parsed by your client.

### Calculate delays without running an operation

Here is an exponential policy with jitter and a one-second server minimum:

<!-- retry-example: kind=run features=none -->
```rust
use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let policy = BackoffPolicy::exponential(
        Duration::from_millis(50), 2.0, Duration::from_secs(2),
    )?
        .with_full_jitter()
        .use_retry_after_as_minimum();
    let mut state = policy.start();
    let step = state.next(BackoffRequest::hint(Duration::from_secs(1)));
    assert_eq!(step.effective_delay(), Duration::from_secs(1));
    assert_eq!(step.retry_index(), 1);
    state.reset();
    assert_eq!(state.retry_index(), 0);
    Ok(())
}
```

The first base delay is 50 ms; full jitter cannot raise it above the unjittered
one-second hint, so the effective delay is exactly one second.
`BackoffState::next` advances the retry index. Start a fresh state for a new flow;
in a reconnect loop, reset only when the connection meets your stability criterion.

`maximum_delay()` reports the base strategy's maximum. `limit_delay(cap)` is a
separate final cap applied after hints and jitter. It **can shorten a mandatory
server minimum**: a 500 ms cap would reduce this example's one-second delay to
500 ms. To honor the minimum, keep hints unjittered and avoid a smaller final cap.
If the remaining budget cannot fit the required wait, stop retrying.

## Set budgets and timeouts

| Setting | Configured on | What it limits |
| --- | --- | --- |
| `max_attempts(n)` | Policy builder | Total attempts, including the first; zero is invalid |
| `operation_time_budget(d)` | Policy builder | Cumulative admitted-operation time; excludes backoff and control callbacks |
| `total_time_budget(d)` | Policy builder | Flow elapsed time, including control callbacks and backoff |
| `hard_attempt_timeout(d)` | Async/worker executor | Waiting for one attempt |
| `hard_flow_timeout(d)` | Async/worker executor | Waiting across the flow, including retries and backoff |

Both elapsed budgets are **soft admission limits**. If an operation starts with
time remaining and finishes successfully after the budget, that success is
retained when completion accounting is valid. A zero elapsed budget is valid but
prevents even the first attempt. Omitted budgets mean no elapsed limit.

For a remote read, you might set a 10 s total budget on the policy, then a 2 s
attempt timeout and a 5 s flow timeout on its async executor. These settings
answer different questions: may another attempt start, how long may this attempt
be awaited, and how long may the flow be awaited?

Hard timeouts are cooperative. A blocking future poll or synchronous callback
can delay checks. Worker cleanup adds its cancellation grace period. Completion
observers run after timing is frozen and are outside timeout control. These APIs
do not guarantee that the entire `run` call returns by an exact wall-clock deadline.

## Run async operations

For Tokio-native async execution, enable `tokio` and add the direct Tokio dependency shown above. Supply a closure
that creates a **fresh future for each attempt**, rather than reusing one future.
This example first recovers from a client error, then times out a pending read:

<!-- retry-example: kind=run features=tokio -->
```rust
use std::future;
use std::io;
use std::time::Duration;

use qubit_retry::RetryConfig;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryFallback;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::TokioRetry;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = RetryConfig::<io::Error>::builder()
        .max_attempts(3)
        .fallback(RetryFallback::Retry)
        .build()?;
    // Simulated client responses; retry limits are enforced by the policy.
    let mut responses = [
        Err(io::Error::new(io::ErrorKind::TimedOut, "storage unavailable")),
        Ok("snapshot-v2"),
    ].into_iter();
    let success = TokioRetry::new(&config).run(|| {
        let response = responses.next().expect("fixture has two responses");
        async move { response }
    }).await?;
    assert_eq!(success.context().attempts(), 2);

    let error = TokioRetry::new(&config)
        .hard_attempt_timeout(Duration::from_millis(10))
        .run(|| future::pending::<Result<(), io::Error>>())
        .await.unwrap_err();
    assert!(matches!(error.reason(), RetryErrorReason::TimedOut {
        scope: RetryTimeoutScope::Attempt
    }));
    assert_eq!(error.context().attempts(), 1);
    Ok(())
}
```

The pending read stops after one attempt: retry-all fallback does not retry an
executor timeout. Use selective rules from the snapshot example when the client
can also return permanent errors. The response sequence only simulates a client
that times out and then succeeds; it does not control the retry count. For a real
client, create each request future in the closure and let the library count and
stop attempts.

## Cancel work and shut down

Clone a `RetryCancellationToken` into the component responsible for shutdown,
pass another clone to the executor, and call `cancel()` to request cancellation.
Clones share permanent cancellation state. Create a new token for independent
work; tokens have no reset, parent-child hierarchy, or built-in deadline.

### Synchronous operations

Sync checks cancellation around attempts and during backoff, but cannot interrupt
the closure. This deterministic example requests shutdown during the failed read:

<!-- retry-example: kind=run features=none -->
```rust
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryErrorReason;

fn main() {
    let token = RetryCancellationToken::new();
    let config = RetryConfig::<&str>::builder().build().expect("valid config");
    let error = Retry::new(&config).cancellation_token(token.clone()).run(|| {
        token.cancel();
        Err::<(), _>("temporary read failure")
    }).unwrap_err();
    assert_eq!(error.context().attempts(), 1);
    assert!(matches!(error.reason(), RetryErrorReason::Cancelled {
        phase: RetryCancellationPhase::Backoff
    }));
}
```

The result is cancelled in the `Backoff` phase after one attempt. If the closure
returns `Ok` instead, success wins over cancellation when completion clock
accounting is valid. Bound the underlying blocking I/O separately.

### Pending async operations

This example triggers cancellation as the attempt creates a pending future;
an external shutdown task can cancel the same token in an application:

<!-- retry-example: kind=run features=tokio -->
```rust
use std::future;
use std::time::Duration;

use qubit_retry::RetryConfig;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryErrorReason;
use qubit_retry::TokioRetry;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let config = RetryConfig::<&str>::builder().build().expect("valid config");
    let token = RetryCancellationToken::new();
    let operation_token = token.clone();
    let error = TokioRetry::new(&config)
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

Cancellation drops the pending operation future. When an operation result,
cancellation, and timeout are ready in the same poll, async selection prefers
result, then cancellation, then timer. A selected error still goes through
failure handling. Equal attempt and flow deadlines are attributed to the attempt.
Dropping the future does not roll back a request already sent to a server.

### Blocking operations on a worker

Enable `worker`. The closure receives an `AttemptCancellationToken`, distinct
from the flow token passed to `.cancellation_token(...)`. Check the attempt token
between bounded pieces of work and release resources promptly when cancelled.
The loop below only demonstrates that handshake; it simulates no storage I/O.

<!-- retry-example: kind=run features=worker -->
```rust
use std::time::Duration;

use qubit_retry::RetryConfig;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryErrorReason;
use qubit_retry::WorkerRetry;

fn main() {
    let config = RetryConfig::<&str>::builder().build().expect("valid config");
    let token = RetryCancellationToken::new();
    let operation_token = token.clone();
    let error = WorkerRetry::new(&config)
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

Each attempt creates a worker and a reaper thread; there is no pool. Worker mode
checks cancellation, then timeout, before accepting a result with confirmed
thread exit. Exit includes thread-local storage (TLS) destruction.

`cancellation_grace` defaults to 100 ms; the example sets one second. It uses real
monotonic time even with an injected timer. If cleanup exceeds it, the result is
`Infrastructure { failure: WorkerStillRunning { trigger } }`. The flow starts no
further attempt, but the worker and reaper may remain alive. Do not automatically
start replacement work that could overlap it. Without a timeout or cancellation,
waiting for worker exit can be unbounded. Threads cannot be forcibly killed.

## Handle results and observe execution

### Preserve the final outcome

| Result | Inspect without consuming | Consume without losing information |
| --- | --- | --- |
| `RetrySuccess<T>` | `value()`, `context()`, `completion_callback_failures()` | `into_parts()` → `(value, context, diagnostics)` |
| `RetryError<E>` | `reason()`, `last_failure()`, `last_error()`, `context()`, `completion_callback_failures()` | `into_parts()` → `(reason, last_failure, context, diagnostics)` |

`last_failure` is optional: execution can stop before an operation fails.
`last_error()` is present only for a retained application error, not an executor
timeout or panic. The success tuple contains a `Vec<RetryCallbackFailure>`;
the error tuple contains a `Box<[RetryCallbackFailure]>`.

| `RetryErrorReason` | Meaning |
| --- | --- |
| `Aborted` | A rule or default behavior chose to stop |
| `Exhausted { limit }` | Attempt count, operation budget, or total budget prevented continuation |
| `TimedOut { scope }` | An attempt or flow hard timeout stopped execution |
| `Cancelled { phase }` | Cancellation was observed before an attempt, during an attempt, or during backoff |
| `CallbackFailed { callback }` | A rule or control observer panicked |
| `Infrastructure { failure }` | A clock, timer, or worker runtime failure prevented continuation |

Use a wildcard arm when matching the non-exhaustive reason enum.
`map_error` converts only the retained application error while preserving
context, reason, and diagnostics; its `FnOnce` mapper runs zero or one times.
For domain adapters, `into_metadata_and_error()` separates an optional application
error from `RetryErrorMetadata`. Prefer preserving the full `RetryError<E>` as a
source when no projection is needed. `into_value_discarding_diagnostics()` is an
explicit option for discarding successful context and diagnostics.

### Observe lifecycle events

Register a `RetryObserver<E>` with `.observer(...)` on the retry builder.

| Callback | When it runs | If it panics |
| --- | --- | --- |
| `on_before_attempt` | Before admission; the upcoming attempt is not yet counted | Stops as `CallbackFailed` |
| `on_attempt_failed` | After an attempt failure is recorded, before rules | Stops as `CallbackFailed` |
| `on_retry_scheduled` | When the selected delay currently fits continuation limits | Stops as `CallbackFailed` |
| `on_success` | After the success result is frozen | Adds a diagnostic; preserves success |
| `on_terminal_failure` | After the terminal error is frozen, including zero-attempt errors | Adds a diagnostic; preserves the error |

Keep callbacks short and nonblocking. Count admissions with terminal
`context.attempts()`, not scheduled notifications. Completion callback failures
do not prevent later completion observers from running.

This example deliberately makes an audit callback panic and shows how to retain
its diagnostic while converting the business error to `String`:

<!-- retry-example: kind=run features=none -->
```rust
use qubit_retry::Retry;
use qubit_retry::RetryConfig;
use qubit_retry::RetryCallbackPhase;
use qubit_retry::RetryContext;
use qubit_retry::RetryErrorReason;
use qubit_retry::RetryObserver;

struct Audit;
impl RetryObserver<&'static str> for Audit {
    fn on_terminal_failure(&self, _: &RetryErrorReason, _: &RetryContext) {
        panic!("audit sink unavailable");
    }
}

fn main() {
    let config = RetryConfig::builder().max_attempts(1)
        .observer(Audit).build().expect("valid config");
    let error = Retry::new(&config).run(|| Err::<(), _>("offline")).unwrap_err();
    let mapped = error.map_error(String::from);
    let (reason, failure, context, diagnostics) = mapped.into_parts();
    assert!(matches!(reason, RetryErrorReason::Aborted));
    assert_eq!(failure.and_then(|failure| failure.into_error()).as_deref(), Some("offline"));
    assert_eq!(context.attempts(), 1);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].phase(), RetryCallbackPhase::TerminalFailure);
}
```

A panic hook may print `audit sink unavailable` even though the example completes
successfully. Only worker mode catches **operation** panics; sync/async operation
panics propagate. Unwinding, dropping an async `run` future, or aborting the process
does not guarantee completion notifications. Capturing a panic requires unwinding;
it cannot recover from process abort.

## Load JSON configuration

Enable `serde`:

<!-- retry-example: kind=cargo features=serde -->
```toml
[dependencies]
qubit-retry = { version = "0.24", features = ["serde"] }
```

For this JSON example also run `cargo add serde_json@1`. Policies are validated
when deserialized, just as they are when built in Rust:

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
    assert_eq!(policy.admission_limits().max_attempts().get(), 4);
    let encoded = serde_json::to_string(&policy)?;
    assert_eq!(serde_json::from_str::<RetryPolicy>(&encoded)?, policy);
    Ok(())
}
```

Durations use `seconds` and `nanoseconds`, with the nanosecond part below one
billion. Omitted or `null` optional elapsed budgets mean no limit. Unknown fields,
zero `max_attempts`, invalid duration values, and invalid backoff parameters are
rejected. Bounded jitter requires a numeric ratio; `null` is invalid.

The feature serializes validated policies, admission limits, and backoff
configuration. It does not serialize rules, observers, cancellation tokens,
results, or errors. Hard timeouts remain execution settings; do not insert them
into this policy JSON.

## Use standalone budgets and test clocks

If your application already owns a reconnect loop, use `RetryBudget` for admission
and accounting, and `BackoffState` for delays. These components do not execute
operations, classify errors, or emit retry observer notifications for you.

For this example add `qubit-clock` directly with its test utilities:

```bash
cargo add qubit-clock@0.13 --features test-util
```

Advance a manual clock to verify operation time and total time independently:

<!-- retry-example: kind=run features=none -->
```rust
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_retry::RetryBudget;
use qubit_retry::RetryPolicy;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clock = ManualMonotonicClock::new_shared();
    let policy = RetryPolicy::builder().max_attempts(2).build()?;
    let mut budget = RetryBudget::new(clock.as_ref(), *policy.admission_limits())?;
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

The two-second operation contributes to both counters; the one-second wait only
contributes to total time. Finish each attempt token before beginning the next.
Tokens belong to their original budget; dropping an unfinished token prevents
further admission in that budget. `check_retry_after` validates a proposed wait,
and admission rechecks limits after the actual wait.

Executors also accept `.timer(...)` and `.random_source(...)`. A manual timer
should use the same clock domain throughout the flow. Synchronize a test with
an observable operation or timer registration before advancing time; avoid using
real sleeps to guess when the executor is ready. Clock regression and domain
mismatches produce structured errors. Worker cleanup grace still uses real time.

### Test jitter deterministically

Inject a fixed random source to test jitter. This implementation always selects
the permitted lower bound, so full jitter over 100 ms produces zero:

<!-- retry-example: kind=run features=none -->
```rust
use std::sync::Arc;
use std::time::Duration;

use qubit_retry::BackoffPolicy;
use qubit_retry::BackoffRequest;
use qubit_retry::RetryRandomSource;

struct LowerBound;
impl RetryRandomSource for LowerBound {
    fn random_f64_inclusive(&self, min: f64, _: f64) -> f64 {
        min
    }
}

fn main() {
    let policy = BackoffPolicy::fixed(Duration::from_millis(100)).with_full_jitter();
    let mut state = policy.start_with_random_source(Arc::new(LowerBound));
    let step = state.next(BackoffRequest::policy());
    assert_eq!(step.effective_delay(), Duration::ZERO);
}
```

`RetryRandomSource` must support concurrent calls and return a finite value within
the supplied inclusive bounds. For a complete flow, inject the same source with
`.random_source(Arc::new(LowerBound))` on the executor. This implementation tests
an endpoint; always selecting the lower bound in an application removes the
random distribution of retries.

### Additional configuration entry points

| Need | Methods and constraints |
| --- | --- |
| Set or clear an optional budget | `operation_time_budget_opt(Some(d))` / `total_time_budget_opt(Some(d))`; pass `None` to clear it |
| Explicitly remove elapsed limits | `without_operation_time_budget()` / `without_total_time_budget()` |
| Reuse callbacks already held in `Arc` | `shared_rule(...)` / `shared_observer(...)` accept the corresponding trait objects and preserve registration order |
| Identify workers or request stack space | `thread_name(name)` / `worker_stack_size(bytes)` on the worker executor |
| Wait for cancellation | Poll `is_cancelled()` synchronously or await the flow token's `cancelled()` future |

Cloned `Retry` values share callback objects, including mutable state such as
counters inside them; synchronize that state when needed. An injected worker
timer must progress independently while the calling thread is blocked.
Standalone `RetryBudgetError` distinguishes `Clock`, `Exhausted`,
`AttemptInProgress`, and `InvalidAttempt`: invalid timing, an exhausted limit,
an unfinished prior attempt, or a token that does not identify the active attempt.
After a normal control callback returns, the clock is refreshed before processing
cancellation or a rule decision; an invalid sample becomes an infrastructure
failure. A callback panic keeps `CallbackFailed` as its primary reason with
best-effort timing.

## Troubleshoot

| Symptom | What to check |
| --- | --- |
| Only one call despite `max_attempts(3)` | Unclassified errors abort by default. Check the rule and fallback before increasing limits. |
| `attempts() == 0` | Check pre-cancellation, zero budgets/timeouts, before-attempt callbacks, and infrastructure errors. |
| `Exhausted` instead of `TimedOut` | Soft budgets stop admission; inspect `limit`. Hard timeout errors carry `scope`. |
| Sync runs longer than the budget | Its closure cannot be interrupted. Bound the underlying I/O or choose a suitable async/worker operation. |
| `AsyncRetry`, `TokioRetry`, or `WorkerRetry` is unavailable | Enable the matching `async`, `tokio`, or `worker` feature; `AsyncRetry` only needs an executor supplied by the application. |
| Fewer calls than retry-scheduled notifications | Scheduling does not guarantee admission. Later cancellation, callbacks, or limits can prevent the attempt. |
| `WorkerStillRunning` | Inspect its trigger and the operation/TLS cleanup protocol before starting replacement work. |
| A successful result contains diagnostics | Inspect completion observers; their panic does not invalidate the business result. |
| A server hint became shorter | Check hint jitter permission, hint selection, and the final delay cap. |
| `Infrastructure::Clock` or timer failure | Check the supplied clock/timer and diagnostic message; do not fabricate elapsed values. |

Keep retries bounded for remote workloads, account for any retry behavior already
inside the client, and define how uncertain side effects are reconciled. The
library provides no circuit breaker, cancellation hierarchy, async executor, or
thread pool.

## Further reading and documentation checks

- [English README](../README.md) · [中文 README](../README.zh_CN.md)
- [API reference for 0.24.0](https://docs.rs/qubit-retry/0.24.0/qubit_retry/)
- [Design](design.md) · [中文用户手册](user_guide.zh_CN.md)

From the repository root, run `python3 -B scripts/check_doc_examples.py` to compile
and execute every annotated Rust/Cargo block in both README and guide languages.
The checker uses this checkout as a path dependency, temporary consumer crates,
and offline Cargo resolution; dependencies must already be cached.
