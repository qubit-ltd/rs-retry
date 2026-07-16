# Retry Lifecycle Semantics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修正 `qubit-retry` 的 hard-timeout failure notification 与 attempt 计数语义，集中三种 runner 的公共生命周期，并完成下游兼容、确定性测试和 Rust 风格清理。

**Architecture:** 先用公开行为回归测试锁定“upcoming attempt”和“committed attempt”两个概念，以及 hard elapsed timeout 的 observation/terminal 顺序。随后把 runner 共有的 pre-attempt、commit 和 completion 阶段提取到私有 `executor/internal/attempt_lifecycle.rs`，普通 failure 与 hard terminal failure 在 `RetryFailureHandler` 中共享 observation 阶段。`rs-http` 适配新增可见的 timeout failure，`rs-cas` 通过现有 hook 和 report 验证 timeout 被观察。

**Tech Stack:** Rust 2024、Rust 1.94、Tokio 1.52、`qubit-clock` manual monotonic sleepers、标准库 thread/channel、Cargo integration tests。

## Global Constraints

- 设计规格：`rs-retry/docs/superpowers/specs/2026-07-16-retry-lifecycle-semantics-design.md`。
- 保持 `qubit-retry` 现有公开类型、方法、模块导出路径兼容，不增加公开 listener API。
- 不处理闭包装箱、动态分发、基准测试或其他第 6 项性能议题。
- 所有 Rust 测试放在 `tests/`，不新增 inline `#[cfg(test)]` 模块，不为测试扩大生产可见性。
- 新增 Rust 文件复制仓库现有完整版权头；每个 struct/enum/trait 独立文件，私有辅助类型位于 `internal/`。
- 每个行为修改严格执行 red → green → refactor；每次 red 必须确认是预期断言失败而不是编译错误。
- 三个仓库分别检查和验证；不得跨仓库混合 Git 操作。
- 未经用户明确要求，不执行 `git add`、`git commit`、`git push`。计划中的审查关卡使用 `git --no-pager diff` 和测试结果代替提交。
- 保留当前本地 path dependencies；不得把 `qubit-*` 依赖退回 crates.io 版本。

---

### Task 1: Commit attempt only after pre-attempt checks

**Files:**
- Modify: `rs-retry/tests/executor/retry_run_and_listener_tests.rs`
- Modify: `rs-retry/tests/executor/retry_async_tests.rs`
- Modify: `rs-retry/tests/executor/retry_worker_and_blocking_timeout_tests.rs`
- Modify: `rs-retry/src/executor/retry_flow_state.rs`
- Modify: `rs-retry/src/executor/retry_runner.rs`
- Modify: `rs-retry/src/executor/async_retry_runner.rs`
- Modify: `rs-retry/src/executor/worker_retry_runner.rs`
- Modify: `rs-retry/src/error/retry_error.rs`
- Modify: `rs-retry/src/event/retry_context.rs`

**Interfaces:**
- Consumes: existing `RetryFlowState::context`, `RetryEvents::before_attempt`, and elapsed-budget checks.
- Produces: `RetryFlowState::next_attempt_context(...) -> RetryContext`; terminal `RetryError::attempts()` counts only attempts admitted into execution.

- [ ] **Step 1: Change the sync first-attempt regression test to require committed-attempt semantics**

Replace the wall-clock listener in `test_max_total_elapsed_includes_before_attempt_listener_time` with manual time and assert both counters explicitly:

```rust
#[test]
fn test_max_total_elapsed_includes_before_attempt_listener_time() {
    let clock = Arc::new(ManualMonotonicClock::new());
    let sleeper =
        Arc::new(ManualBlockingSleeper::from_clock(Arc::clone(&clock)));
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let listener_attempts = Arc::clone(&observed_attempts);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .blocking_sleeper(sleeper)
        .before_attempt(move |context: &RetryContext| {
            listener_attempts
                .lock()
                .expect("observed attempts should be lockable")
                .push(context.attempt());
            listener_clock
                .advance(Duration::from_secs(20))
                .expect("manual time should advance");
        })
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { panic!("operation must not run") })
        .expect_err("before-attempt listener should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 0);
    assert_eq!(
        *observed_attempts
            .lock()
            .expect("observed attempts should be lockable"),
        vec![1]
    );
    assert!(error.last_failure().is_none());
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(20));
}
```

- [ ] **Step 2: Run the sync test and verify RED**

Run from `rs-retry`:

```bash
cargo test --all-features --test lib_tests \
  executor::retry_run_and_listener_tests::test_max_total_elapsed_includes_before_attempt_listener_time \
  -- --exact
```

Expected: FAIL because the old implementation reports `error.attempts() == 1`, while the listener still observes attempt 1.

- [ ] **Step 3: Replace the async and worker tests with exact committed-attempt assertions**

Replace the async test body with:

```rust
#[tokio::test]
async fn test_run_async_max_total_elapsed_includes_before_attempt_listener_time()
{
    let clock = Arc::new(ManualMonotonicClock::new());
    let sleeper = Arc::new(ManualAsyncSleeper::from_clock(Arc::clone(&clock)));
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let listener_attempts = Arc::clone(&observed_attempts);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .async_sleeper(sleeper)
        .before_attempt(move |context: &RetryContext| {
            listener_attempts
                .lock()
                .expect("observed attempts should be lockable")
                .push(context.attempt());
            listener_clock
                .advance(Duration::from_secs(20))
                .expect("manual time should advance");
        })
        .build()
        .expect("retry should build");

    let error = retry
        .run_async::<(), _, _>(|| async { panic!("operation must not run") })
        .await
        .expect_err("before-attempt listener should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 0);
    assert_eq!(
        *observed_attempts
            .lock()
            .expect("observed attempts should be lockable"),
        vec![1]
    );
    assert!(error.last_failure().is_none());
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(20));
}
```

Add `Mutex` to the async test module imports. Replace the worker test body with:

```rust
#[test]
fn test_run_in_worker_max_total_elapsed_includes_before_attempt_listener_time()
{
    let _guard = WORKER_THREAD_ID_LOCK
        .lock()
        .expect("worker probe lock should be available");
    WORKER_THREAD_ID_CALLS.store(0, Ordering::SeqCst);
    let clock = Arc::new(ManualMonotonicClock::new());
    let sleeper =
        Arc::new(ManualBlockingSleeper::from_clock(Arc::clone(&clock)));
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let listener_attempts = Arc::clone(&observed_attempts);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(2)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .blocking_sleeper(sleeper)
        .before_attempt(move |context: &RetryContext| {
            listener_attempts
                .lock()
                .expect("observed attempts should be lockable")
                .push(context.attempt());
            listener_clock
                .advance(Duration::from_secs(20))
                .expect("manual time should advance");
        })
        .build()
        .expect("retry should build");

    let error = retry.run_in_worker(record_worker_thread_id).expect_err(
        "before-attempt listener should exhaust total elapsed",
    );

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 0);
    assert_eq!(WORKER_THREAD_ID_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(
        *observed_attempts
            .lock()
            .expect("observed attempts should be lockable"),
        vec![1]
    );
    assert!(error.last_failure().is_none());
    assert_eq!(error.context().total_elapsed(), Duration::from_secs(20));
}
```

Add `qubit_clock::{ManualBlockingSleeper, ManualMonotonicClock}` to the worker test module imports.

- [ ] **Step 4: Run the async and worker tests and verify RED**

```bash
cargo test --all-features --test lib_tests \
  executor::retry_async_tests::test_run_async_max_total_elapsed_includes_before_attempt_listener_time \
  -- --exact
cargo test --all-features --test lib_tests \
  executor::retry_worker_and_blocking_timeout_tests::test_run_in_worker_max_total_elapsed_includes_before_attempt_listener_time \
  -- --exact
```

Expected: both FAIL only on the old `attempts == 1` behavior.

- [ ] **Step 5: Add a second-attempt regression test**

Add this sync test to prove the previous committed attempt and its failure survive a second pre-attempt stop:

```rust
#[test]
fn test_max_total_elapsed_before_second_operation_preserves_first_attempt() {
    let clock = Arc::new(ManualMonotonicClock::new());
    let sleeper =
        Arc::new(ManualBlockingSleeper::from_clock(Arc::clone(&clock)));
    let observed_attempts = Arc::new(Mutex::new(Vec::new()));
    let listener_attempts = Arc::clone(&observed_attempts);
    let listener_clock = Arc::clone(&clock);
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .max_total_elapsed(Some(Duration::from_secs(20)))
        .no_delay()
        .blocking_sleeper(sleeper)
        .before_attempt(move |context: &RetryContext| {
            listener_attempts
                .lock()
                .expect("observed attempts should be lockable")
                .push(context.attempt());
            if context.attempt() == 2 {
                listener_clock
                    .advance(Duration::from_secs(20))
                    .expect("manual time should advance");
            }
        })
        .build()
        .expect("retry should build");

    let error = retry
        .run(|| -> Result<(), TestError> { Err(TestError("first")) })
        .expect_err("second pre-attempt check should exhaust total elapsed");

    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(
        error.last_failure().and_then(AttemptFailure::as_error),
        Some(&TestError("first"))
    );
    assert_eq!(
        *observed_attempts
            .lock()
            .expect("observed attempts should be lockable"),
        vec![1, 2]
    );
}
```

- [ ] **Step 6: Run the second-attempt test and verify RED**

```bash
cargo test --all-features --test lib_tests \
  executor::retry_run_and_listener_tests::test_max_total_elapsed_before_second_operation_preserves_first_attempt \
  -- --exact
```

Expected: FAIL because the old state reports attempt 2 after the second listener.

- [ ] **Step 7: Implement the minimal state API and move commit points**

In `RetryFlowState`, keep `context` for committed attempts and add an upcoming context wrapper:

```rust
/// Builds a context for the attempt that will run after pre-attempt checks.
///
/// # Arguments
///
/// * `options` - Retry limits copied into the context.
/// * `attempt_elapsed` - Elapsed time for the upcoming attempt, normally zero.
/// * `attempt_timeout` - Effective timeout visible to listeners.
///
/// # Returns
///
/// A context whose attempt is one greater than the committed attempt count.
pub(in crate::executor) fn next_attempt_context(
    &self,
    options: &RetryOptions,
    attempt_elapsed: Duration,
    attempt_timeout: EffectiveAttemptTimeout,
) -> RetryContext {
    self.context_with_attempt(
        self.attempts + 1,
        options,
        attempt_elapsed,
        attempt_timeout,
    )
}
```

Refactor `context` through a private `context_with_attempt` helper that fills `RetryContextParts`. In all three runners, replace the pre-listener `state.context(...)` with `state.next_attempt_context(...)`, move `state.start_next_attempt()` to immediately after the second elapsed-budget check, and leave worker spawn inside the committed phase.

The helper body must be:

```rust
/// Builds a context snapshot for an explicit attempt number.
///
/// # Arguments
///
/// * `attempt` - Attempt number stored in the context.
/// * `options` - Retry limits copied into the context.
/// * `attempt_elapsed` - Elapsed time for the represented attempt.
/// * `attempt_timeout` - Effective timeout for the represented attempt.
///
/// # Returns
///
/// A retry context using the supplied attempt and this state's elapsed values.
fn context_with_attempt(
    &self,
    attempt: u32,
    options: &RetryOptions,
    attempt_elapsed: Duration,
    attempt_timeout: EffectiveAttemptTimeout,
) -> RetryContext {
    RetryContext::from_parts(RetryContextParts {
        attempt,
        max_attempts: options.max_attempts(),
        max_operation_elapsed: options.max_operation_elapsed(),
        max_total_elapsed: options.max_total_elapsed(),
        operation_elapsed: self.operation_elapsed,
        total_elapsed: self.total_elapsed(),
        attempt_elapsed,
        attempt_timeout: attempt_timeout.duration(),
    })
    .with_attempt_timeout_source(attempt_timeout.source())
}
```

- [ ] **Step 8: Run all four focused tests and verify GREEN**

Run the four exact commands from Steps 2, 4, and 6. Expected: PASS with zero warnings.

- [ ] **Step 9: Update public counter documentation**

Use these contracts:

```rust
/// Returns the number of attempts admitted into execution.
///
/// `before_attempt` receives the upcoming one-based attempt number before it
/// is committed. If a pre-attempt listener exhausts a budget, this count does
/// not include that unexecuted attempt.
```

Apply the same definition to `RetryError::attempts`, `RetryContext::attempt`, and the relevant README listener/elapsed-budget sections.

- [ ] **Step 10: Inspect Task 1 diff without staging**

```bash
git --no-pager diff -- \
  src/executor/retry_flow_state.rs \
  src/executor/retry_runner.rs \
  src/executor/async_retry_runner.rs \
  src/executor/worker_retry_runner.rs \
  src/error/retry_error.rs \
  src/event/retry_context.rs \
  tests/executor
git diff --check
```

Expected: only attempt-phase semantics, deterministic test setup, and matching docs; `git diff --check` exits 0.

---

### Task 2: Notify hard elapsed timeout failures exactly once

**Files:**
- Modify: `rs-retry/tests/executor/retry_async_tests.rs`
- Modify: `rs-retry/tests/executor/retry_worker_and_blocking_timeout_tests.rs`
- Modify: `rs-retry/src/executor/retry_failure_handler.rs`
- Modify: `rs-retry/src/executor/async_retry_runner.rs`
- Modify: `rs-retry/src/executor/worker_retry_runner.rs`
- Modify: `rs-cas/tests/executor/cas_executor_tests.rs`

**Interfaces:**
- Consumes: `RetryEvents::retry_after_hint`, `RetryEvents::failure_decision`, `EffectiveAttemptTimeout::elapsed_timeout_reason`.
- Produces: `RetryFailureHandler::elapsed_timeout_error(...) -> RetryError<E>`; hard elapsed timeouts run observation but never delay or retry.

- [ ] **Step 1: Add an async hard-timeout listener regression test**

Add a manual-time test that configures `max_operation_elapsed`, returns `Retry` from `on_failure`, and verifies the hard reason wins:

```rust
#[tokio::test]
async fn test_run_async_elapsed_timeout_notifies_failure_without_retrying() {
    let clock = Arc::new(ManualMonotonicClock::new());
    let sleeper = Arc::new(ManualAsyncSleeper::from_clock(Arc::clone(&clock)));
    let failures = Arc::new(AtomicUsize::new(0));
    let retries = Arc::new(AtomicUsize::new(0));
    let sources = Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener_failures = Arc::clone(&failures);
    let listener_sources = Arc::clone(&sources);
    let retry_events = Arc::clone(&retries);
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .max_operation_elapsed(Some(Duration::from_secs(30)))
        .no_delay()
        .async_sleeper(sleeper)
        .on_failure(move |failure: &AttemptFailure<TestError>, context: &RetryContext| {
            assert!(matches!(failure, AttemptFailure::Timeout));
            listener_failures.fetch_add(1, Ordering::SeqCst);
            listener_sources
                .lock()
                .expect("timeout sources should be lockable")
                .push(context.attempt_timeout_source());
            AttemptFailureDecision::Retry
        })
        .on_retry(move |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
            retry_events.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<(), TestError>>);
    tokio::pin!(retry_future);
    let waiter_registration = clock.wait_for_waiters_async(1);
    tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        () = waiter_registration => {}
    }
    clock
        .advance(Duration::from_secs(30))
        .expect("manual time should advance");

    let error = retry_future
        .await
        .expect_err("max-operation elapsed should terminate the attempt");
    assert_eq!(error.reason(), RetryErrorReason::MaxOperationElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(failures.load(Ordering::SeqCst), 1);
    assert_eq!(retries.load(Ordering::SeqCst), 0);
    assert_eq!(
        *sources.lock().expect("timeout sources should be lockable"),
        vec![Some(AttemptTimeoutSource::MaxOperationElapsed)]
    );
}
```

Add the max-total companion as a separate test:

```rust
#[tokio::test]
async fn test_run_async_total_timeout_notifies_failure_without_retrying() {
    let clock = Arc::new(ManualMonotonicClock::new());
    let sleeper = Arc::new(ManualAsyncSleeper::from_clock(Arc::clone(&clock)));
    let failures = Arc::new(AtomicUsize::new(0));
    let retries = Arc::new(AtomicUsize::new(0));
    let sources = Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener_failures = Arc::clone(&failures);
    let listener_sources = Arc::clone(&sources);
    let retry_events = Arc::clone(&retries);
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .max_total_elapsed(Some(Duration::from_secs(30)))
        .no_delay()
        .async_sleeper(sleeper)
        .on_failure(move |failure: &AttemptFailure<TestError>, context: &RetryContext| {
            assert!(matches!(failure, AttemptFailure::Timeout));
            listener_failures.fetch_add(1, Ordering::SeqCst);
            listener_sources
                .lock()
                .expect("timeout sources should be lockable")
                .push(context.attempt_timeout_source());
            AttemptFailureDecision::Abort
        })
        .on_retry(move |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
            retry_events.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("retry should build");

    let retry_future =
        retry.run_async(std::future::pending::<Result<(), TestError>>);
    tokio::pin!(retry_future);
    let waiter_registration = clock.wait_for_waiters_async(1);
    tokio::select! {
        result = &mut retry_future => {
            panic!("attempt completed before manual time advanced: {result:?}");
        }
        () = waiter_registration => {}
    }
    clock
        .advance(Duration::from_secs(30))
        .expect("manual time should advance");

    let error = retry_future
        .await
        .expect_err("max-total elapsed should terminate the attempt");
    assert_eq!(error.reason(), RetryErrorReason::MaxTotalElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(failures.load(Ordering::SeqCst), 1);
    assert_eq!(retries.load(Ordering::SeqCst), 0);
    assert_eq!(
        *sources.lock().expect("timeout sources should be lockable"),
        vec![Some(AttemptTimeoutSource::MaxTotalElapsed)]
    );
}
```

Add `AttemptFailureDecision` and `Mutex` to the async test imports.

- [ ] **Step 2: Add a cooperative worker hard-timeout regression test**

Use a real short deadline but make the worker exit on the cancellation token instead of sleeping for an arbitrary duration:

```rust
#[test]
fn test_run_in_worker_elapsed_timeout_notifies_failure_without_retrying() {
    let failures = Arc::new(AtomicUsize::new(0));
    let retries = Arc::new(AtomicUsize::new(0));
    let listener_failures = Arc::clone(&failures);
    let retry_events = Arc::clone(&retries);
    let retry = Retry::<TestError>::builder()
        .max_attempts(3)
        .max_operation_elapsed(Some(Duration::from_millis(20)))
        .worker_cancel_grace(Duration::from_secs(1))
        .no_delay()
        .on_failure(move |failure: &AttemptFailure<TestError>, _context: &RetryContext| {
            assert!(matches!(failure, AttemptFailure::Timeout));
            listener_failures.fetch_add(1, Ordering::SeqCst);
            AttemptFailureDecision::Retry
        })
        .on_retry(move |_failure: &AttemptFailure<TestError>, _context: &RetryContext| {
            retry_events.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("retry should build");

    let error = retry
        .run_in_worker(|token: AttemptCancelToken| {
            while !token.is_cancelled() {
                thread::yield_now();
            }
            Ok::<(), TestError>(())
        })
        .expect_err("max-operation elapsed should terminate the worker attempt");

    assert_eq!(error.reason(), RetryErrorReason::MaxOperationElapsedExceeded);
    assert_eq!(error.attempts(), 1);
    assert_eq!(failures.load(Ordering::SeqCst), 1);
    assert_eq!(retries.load(Ordering::SeqCst), 0);
}
```

- [ ] **Step 3: Add the `rs-cas` report/event regression before changing production code**

Add under Tokio in `cas_executor_tests.rs`:

```rust
#[tokio::test(start_paused = true)]
async fn test_execute_async_elapsed_timeout_updates_report_and_events() {
    let state = AtomicRef::from_value(5usize);
    let attempt_failures = Arc::new(AtomicUsize::new(0));
    let retry_requests = Arc::new(AtomicUsize::new(0));
    let listener_failures = Arc::clone(&attempt_failures);
    let listener_retries = Arc::clone(&retry_requests);
    let hooks = CasHooks::new().on_event(move |event: &CasEvent| match event {
        CasEvent::AttemptFailed { kind, .. }
            if *kind == CasAttemptFailureKind::Timeout =>
        {
            listener_failures.fetch_add(1, Ordering::SeqCst);
        }
        CasEvent::RetryRequested { .. } => {
            listener_retries.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    });
    let executor = CasExecutor::<usize, TestError>::builder()
        .max_attempts(3)
        .max_operation_elapsed(Some(Duration::from_secs(30)))
        .no_delay()
        .observability(CasObservabilityConfig::event_stream())
        .build()
        .expect("executor should build");

    let outcome = executor
        .execute_async_with_hooks(
            &state,
            |_current: Arc<usize>| async move {
                std::future::pending::<CasDecision<usize, (), TestError>>().await
            },
            hooks,
        )
        .await;

    assert_eq!(outcome.report().timeouts(), 1);
    assert_eq!(attempt_failures.load(Ordering::SeqCst), 1);
    assert_eq!(retry_requests.load(Ordering::SeqCst), 0);
    let error = outcome.expect_err("elapsed timeout should terminate CAS");
    assert_eq!(error.kind(), CasErrorKind::MaxOperationElapsedExceeded);
    assert_eq!(error.attempts(), 1);
}
```

- [ ] **Step 4: Run the three regression groups and verify RED**

From `rs-retry`:

```bash
cargo test --all-features --test lib_tests \
  executor::retry_async_tests::test_run_async_elapsed_timeout_notifies_failure_without_retrying \
  -- --exact
cargo test --all-features --test lib_tests \
  executor::retry_worker_and_blocking_timeout_tests::test_run_in_worker_elapsed_timeout_notifies_failure_without_retrying \
  -- --exact
```

From `rs-cas`:

```bash
cargo test --all-features --test lib_tests \
  executor::cas_executor_tests::test_execute_async_elapsed_timeout_updates_report_and_events \
  -- --exact
```

Expected: listener/report assertions show zero rather than one; terminal reason itself remains correct.

- [ ] **Step 5: Extract failure observation in `RetryFailureHandler`**

Add `use std::time::Duration;`, a restricted hard-timeout constructor, and a
private observation helper. In the actual inherent impl, keep restricted
methods (`handle`, then `elapsed_timeout_error`) before the final private
`observe_failure`; the helper appears first in this excerpt only so its return
values are defined before use:

```rust
/// Observes one failed attempt before retry policy is applied.
///
/// # Arguments
///
/// * `state` - Retry-flow state used to refresh total elapsed time.
/// * `failure` - Failure produced by the admitted attempt.
/// * `context` - Context captured immediately after the attempt.
///
/// # Returns
///
/// The extracted hint, raw listener decision, and refreshed context.
fn observe_failure(
    &self,
    state: &RetryFlowState<'_, E>,
    failure: &AttemptFailure<E>,
    context: RetryContext,
) -> (Option<Duration>, AttemptFailureDecision, RetryContext) {
    let hint = self.events.retry_after_hint(failure, &context);
    let context = context
        .with_retry_after_hint(hint)
        .with_total_elapsed(state.total_elapsed());
    let listener_decision = self.events.failure_decision(failure, &context);
    let context = context.with_total_elapsed(state.total_elapsed());
    (hint, listener_decision, context)
}

/// Builds a terminal error for an elapsed-budget timeout after observation.
///
/// # Arguments
///
/// * `state` - Retry-flow state used to refresh total elapsed time.
/// * `failure` - Timeout failure produced by the admitted attempt.
/// * `context` - Context captured immediately after the attempt.
/// * `reason` - Hard elapsed-budget terminal reason.
///
/// # Returns
///
/// A terminal error that preserves the timeout and ignores listener policy.
pub(in crate::executor) fn elapsed_timeout_error(
    &self,
    state: &RetryFlowState<'_, E>,
    failure: AttemptFailure<E>,
    context: RetryContext,
    reason: RetryErrorReason,
) -> RetryError<E> {
    let (_hint, _listener_decision, context) =
        self.observe_failure(state, &failure, context);
    RetryError::new(reason, Some(failure), context)
}
```

Change normal `handle` to destructure `observe_failure`, resolve `listener_decision` through `RetryFailurePolicy`, and preserve all existing ordering after that point.

- [ ] **Step 6: Route async and worker elapsed timeouts through the handler**

Replace both direct `RetryError::new(reason, ...)` branches with:

```rust
if let Some(reason) = attempt_timeout.elapsed_timeout_reason(&failure) {
    let error = handler.elapsed_timeout_error(
        &state,
        failure,
        context,
        reason,
    );
    return Err(events.error(error));
}
```

Do not call `handler.handle` afterward and do not schedule a delay.

- [ ] **Step 7: Run all Task 2 tests and verify GREEN**

Repeat Step 4, plus the async max-total companion. Expected: all PASS, one failure notification, zero retry notifications, correct hard reasons, and CAS report timeout count 1.

- [ ] **Step 8: Verify configured timeout policy remains unchanged**

From `rs-retry`:

```bash
cargo test --all-features --test lib_tests \
  executor::retry_async_tests::test_run_async_configured_timeout_policy_wins_when_equal_to_remaining_elapsed \
  -- --exact
cargo test --all-features --test lib_tests \
  executor::retry_worker_and_blocking_timeout_tests::test_run_in_worker_configured_timeout_policy_wins_when_equal_to_remaining_elapsed \
  -- --exact
```

Expected: PASS; configured timeout still enters normal policy rather than hard-stop handling.

---

### Task 3: Extract shared attempt lifecycle and reduce runner complexity

**Files:**
- Create: `rs-retry/src/executor/internal/mod.rs`
- Create: `rs-retry/src/executor/internal/attempt_lifecycle.rs`
- Modify: `rs-retry/src/executor/mod.rs`
- Modify: `rs-retry/src/executor/retry_flow_state.rs`
- Modify: `rs-retry/src/executor/retry_runner.rs`
- Modify: `rs-retry/src/executor/async_retry_runner.rs`
- Modify: `rs-retry/src/executor/worker_retry_runner.rs`

**Interfaces:**
- Consumes: green behavior tests from Tasks 1–2.
- Produces: `prepare_same_thread_attempt`, `prepare_timed_attempt`, and `complete_attempt` in a private internal module; runner public behavior unchanged.

- [ ] **Step 1: Record the green characterization baseline**

```bash
cargo test --all-features --test lib_tests executor::retry_run_and_listener_tests
cargo test --all-features --test lib_tests executor::retry_async_tests
cargo test --all-features --test lib_tests executor::retry_worker_and_blocking_timeout_tests
```

Expected: PASS. This is the refactor phase of the red-green cycles from Tasks 1–2; no new behavior is introduced here.

- [ ] **Step 2: Add the internal module declaration**

`src/executor/internal/mod.rs`:

```rust
// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal retry lifecycle helpers.

mod attempt_lifecycle;

pub(in crate::executor) use attempt_lifecycle::{
    complete_attempt,
    prepare_same_thread_attempt,
    prepare_timed_attempt,
};
```

Add `mod internal;` to `src/executor/mod.rs`.

- [ ] **Step 3: Implement the lifecycle helper module**

`src/executor/internal/attempt_lifecycle.rs` must contain:

```rust
// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared attempt lifecycle transitions for retry runners.

use std::time::Duration;

use qubit_clock::MonotonicInstant;

use crate::event::RetryEvents;
use crate::executor::retry_flow_state::RetryFlowState;
use crate::options::EffectiveAttemptTimeout;
use crate::{RetryContext, RetryError, RetryOptions};

/// Prepares an attempt whose running operation cannot be interrupted.
///
/// # Arguments
///
/// * `state` - Mutable retry-flow state.
/// * `options` - Retry limits used for checks and context construction.
/// * `events` - Listener dispatcher invoked before the attempt is committed.
///
/// # Returns
///
/// A timeout-free attempt descriptor after the attempt is committed.
///
/// # Errors
///
/// Returns a terminal elapsed-budget error when the budget is exhausted before
/// the operation enters execution.
pub(in crate::executor) fn prepare_same_thread_attempt<E>(
    state: &mut RetryFlowState<'_, E>,
    options: &RetryOptions,
    events: &RetryEvents<E>,
) -> Result<EffectiveAttemptTimeout, RetryError<E>> {
    prepare_attempt(state, options, events, |_| EffectiveAttemptTimeout::none())
}

/// Prepares an async or worker attempt with the shortest effective timeout.
///
/// # Arguments
///
/// * `state` - Mutable retry-flow state.
/// * `options` - Retry limits and configured timeout.
/// * `events` - Listener dispatcher invoked before the attempt is committed.
///
/// # Returns
///
/// The effective timeout recomputed after pre-attempt listeners.
///
/// # Errors
///
/// Returns a terminal elapsed-budget error when no budget remains before the
/// operation enters execution.
pub(in crate::executor) fn prepare_timed_attempt<E>(
    state: &mut RetryFlowState<'_, E>,
    options: &RetryOptions,
    events: &RetryEvents<E>,
) -> Result<EffectiveAttemptTimeout, RetryError<E>> {
    prepare_attempt(state, options, events, |state| {
        options.effective_attempt_timeout(
            state.operation_elapsed(),
            state.total_elapsed(),
        )
    })
}

/// Runs pre-attempt checks and commits the next attempt.
///
/// # Arguments
///
/// * `state` - Mutable retry-flow state.
/// * `options` - Retry limits used for checks and context construction.
/// * `events` - Listener dispatcher invoked before commitment.
/// * `effective_timeout` - Resolver evaluated at each budget control point.
///
/// # Returns
///
/// The final effective timeout after the listener and budget recheck.
///
/// # Errors
///
/// Returns a terminal elapsed-budget error before committing the attempt when
/// either pre-attempt check finds an exhausted budget.
fn prepare_attempt<E, F>(
    state: &mut RetryFlowState<'_, E>,
    options: &RetryOptions,
    events: &RetryEvents<E>,
    effective_timeout: F,
) -> Result<EffectiveAttemptTimeout, RetryError<E>>
where
    F: Fn(&RetryFlowState<'_, E>) -> EffectiveAttemptTimeout,
{
    let attempt_timeout = effective_timeout(state);
    if let Some(error) = state.take_elapsed_error(options, attempt_timeout) {
        return Err(error);
    }

    let attempt_timeout = effective_timeout(state);
    let context = state.next_attempt_context(
        options,
        Duration::ZERO,
        attempt_timeout,
    );
    events.before_attempt(&context);

    let attempt_timeout = effective_timeout(state);
    if let Some(error) = state.take_elapsed_error(options, attempt_timeout) {
        return Err(error);
    }
    state.start_next_attempt();
    Ok(attempt_timeout)
}

/// Records one completed attempt and builds its context.
///
/// # Arguments
///
/// * `state` - Mutable retry-flow state.
/// * `options` - Retry limits copied into the context.
/// * `attempt_start` - Monotonic instant captured immediately before execution.
/// * `attempt_timeout` - Effective timeout used for this attempt.
///
/// # Returns
///
/// The completed-attempt context with updated elapsed durations.
pub(in crate::executor) fn complete_attempt<E>(
    state: &mut RetryFlowState<'_, E>,
    options: &RetryOptions,
    attempt_start: MonotonicInstant,
    attempt_timeout: EffectiveAttemptTimeout,
) -> RetryContext {
    let attempt_elapsed = state.elapsed_since(attempt_start);
    state.add_operation_elapsed(attempt_elapsed);
    state.context(options, attempt_elapsed, attempt_timeout)
}
```

- [ ] **Step 4: Replace duplicated runner phases**

Each runner must use `prepare_*_attempt(...).map_err(|error| events.error(error))?` before capturing `attempt_start`, then use `complete_attempt` after the operation/outcome returns. Keep execution-specific timeout select, worker unreaped accounting, sleep type, and failure handling in their existing runners.

Sync shape:

```rust
let attempt_timeout = prepare_same_thread_attempt(
    &mut state,
    options,
    events,
)
.map_err(|error| events.error(error))?;
let attempt_start = sleeper.clock().now();
let result = operation.call();
let context = complete_attempt(
    &mut state,
    options,
    attempt_start,
    attempt_timeout,
);
```

Async shape, retaining the existing select body:

```rust
let attempt_timeout = prepare_timed_attempt(&mut state, options, events)
    .map_err(|error| events.error(error))?;
let attempt_start = sleeper.clock().now();
let result = if let Some(timeout) = attempt_timeout.duration() {
    tokio::select! {
        biased;
        timeout_result = sleeper.sleep_for_async(timeout) => {
            match timeout_result {
                Ok(()) => Err(AttemptFailure::Timeout),
                Err(error) => {
                    return Err(events.error(
                        state.sleeper_error(options, error),
                    ));
                }
            }
        }
        result = operation.call() => result,
    }
} else {
    operation.call().await
};
let context = complete_attempt(
    &mut state,
    options,
    attempt_start,
    attempt_timeout,
);
```

Worker shape:

```rust
let attempt_timeout = prepare_timed_attempt(&mut state, options, events)
    .map_err(|error| events.error(error))?;
let attempt_start = sleeper.clock().now();
let outcome = WorkerAttemptExecutor::run(
    Arc::clone(&operation),
    attempt_timeout.duration(),
    options.worker_cancel_grace(),
);
let context = complete_attempt(
    &mut state,
    options,
    attempt_start,
    attempt_timeout,
)
.with_unreaped_worker_count(outcome.unreaped_worker_count);
```

- [ ] **Step 5: Run characterization tests after extraction**

Repeat Step 1 plus every Task 1–2 focused test. Expected: PASS.

- [ ] **Step 6: Re-query runner complexity**

Use the code graph for the three `run_operation` methods and record cyclomatic/cognitive values. Confirm common lifecycle branches moved into `attempt_lifecycle.rs` and no runner gained a new public dependency. If a runner remains cognitively high because execution-specific branches are still nested, extract only a clearly named execution-specific private method; do not introduce macros or async traits.

---

### Task 4: Adapt `rs-http` to non-HTTP attempt failures

**Files:**
- Modify: `rs-http/tests/client/http_client_tests.rs`
- Modify: `rs-http/src/client/http_client.rs`

**Interfaces:**
- Consumes: Task 2 behavior where elapsed timeout invokes `on_failure`.
- Produces: `HttpClient::retry_failure_decision` returns `UseDefault` for `Timeout`, `Panic`, and `Executor` instead of panicking.

- [ ] **Step 1: Add an in-flight max-duration integration test**

```rust
#[tokio::test]
async fn test_execute_retry_in_flight_max_duration_does_not_panic() {
    let server = spawn_multi_shot_server(vec![ResponsePlan::DelayedStart {
        delay: Duration::from_millis(100),
        status: 200,
        headers: vec![],
        body: b"late".to_vec(),
    }])
    .await;

    let mut options = HttpClientOptions::default();
    options.base_url = Some(server.base_url());
    options.retry.enabled = true;
    options.retry.max_attempts = 3;
    options.retry.max_duration = Some(Duration::from_millis(10));
    options.retry.delay_strategy = RetryDelay::None;
    let client = HttpClientFactory::new()
        .create(options)
        .expect("HTTP client should build");

    let request = client.request(Method::GET, "/in-flight-timeout").build();
    let error = timeout(Duration::from_secs(3), client.execute(request))
        .await
        .expect("execute timed out")
        .expect_err("max duration should terminate the in-flight request");

    assert_eq!(error.kind, HttpErrorKind::RetryMaxElapsedExceeded);
    assert!(error.message.contains("retry max duration exceeded"));

    let captured = timeout(Duration::from_secs(3), server.finish())
        .await
        .expect("server finish timed out");
    assert_eq!(captured.len(), 1);
}
```

- [ ] **Step 2: Run the HTTP test and verify RED**

From `rs-http` after Task 2 is present locally:

```bash
cargo test --all-features --test lib_tests \
  client::http_client_tests::test_execute_retry_in_flight_max_duration_does_not_panic \
  -- --exact
```

Expected: FAIL with the current `expect("HTTP retry attempts do not configure non-HTTP attempt failures")` panic.

- [ ] **Step 3: Make the HTTP policy exhaustive and minimal**

Replace the `as_error().expect(...)` block with:

```rust
let AttemptFailure::Error(error) = failure else {
    return AttemptFailureDecision::UseDefault;
};
```

Keep all HTTP retryability and delay calculations unchanged for `Error`.

- [ ] **Step 4: Run focused and adjacent HTTP retry tests**

```bash
cargo test --all-features --test lib_tests \
  client::http_client_tests::test_execute_retry_in_flight_max_duration_does_not_panic \
  -- --exact
cargo test --all-features --test lib_tests \
  client::http_client_tests::test_execute_retry_max_duration_returns_last_error_after_retry_delay \
  -- --exact
cargo test --all-features --test lib_tests \
  client::http_client_tests::test_execute_retries_retryable_status_until_success \
  -- --exact
```

Expected: PASS; application error policy and retry-after behavior remain unchanged.

---

### Task 5: Reorganize runner tests and remove avoidable wall-clock sleeps

**Files:**
- Create: `rs-retry/tests/executor/retry_runner_tests.rs`
- Create: `rs-retry/tests/executor/async_retry_runner_tests.rs`
- Create: `rs-retry/tests/executor/retry_failure_handler_tests.rs`
- Modify: `rs-retry/tests/executor/worker_retry_runner_tests.rs`
- Modify: `rs-retry/tests/executor/worker_attempt_executor_tests.rs`
- Modify: `rs-retry/tests/executor/mod.rs`
- Delete after migration: `rs-retry/tests/executor/retry_run_and_listener_tests.rs`
- Delete after migration: `rs-retry/tests/executor/retry_async_tests.rs`
- Delete after migration: `rs-retry/tests/executor/retry_worker_and_blocking_timeout_tests.rs`

**Interfaces:**
- Consumes: green public behavior tests and internal source names from Tasks 1–3.
- Produces: test paths mirrored to production owners; no production visibility changes.

- [ ] **Step 1: Move whole-mode test files to mirrored names**

Use authorized file moves, preserving history:

```bash
command mv tests/executor/retry_async_tests.rs \
  tests/executor/async_retry_runner_tests.rs
command mv tests/executor/retry_run_and_listener_tests.rs \
  tests/executor/retry_runner_tests.rs
```

Update `tests/executor/mod.rs` names immediately so the suite continues compiling.

- [ ] **Step 2: Extract failure-handler behavior from sync runner tests**

Move these functions, with their imports and documentation, into `retry_failure_handler_tests.rs`:

- `test_on_failure_can_abort_retry_flow`
- `test_retry_after_decision_selects_next_delay`
- `test_retry_after_hint_is_available_to_failure_listener`
- `test_retry_after_hint_panic_is_isolated_when_enabled`
- `test_retry_after_hint_panic_propagates_by_default`
- `test_max_total_elapsed_includes_failure_listener_time`
- `test_max_total_elapsed_includes_on_retry_listener_time`
- `test_max_total_elapsed_rechecks_retry_sleep_after_on_retry_listener`
- `test_on_retry_listener_time_does_not_count_against_elapsed_budget`
- `test_run_default_boxed_error_type_observes_listeners_and_hints`

Keep runner execution, budget prechecks, blocking sleeper, unsupported timeout, and basic retry success tests in `retry_runner_tests.rs`.

- [ ] **Step 3: Consolidate worker runner tests**

Move the single existing `test_worker_retry_runner_paths_are_observable_through_timeout_and_success` into the large worker runner suite, then replace `worker_retry_runner_tests.rs` with the consolidated content and remove the old long filename. Keep `WorkerAttemptExecutor` channel/join-specific tests in `worker_attempt_executor_tests.rs`.

- [ ] **Step 4: Replace avoidable listener/operation sleeps with manual clock advancement**

For every sync/async elapsed-budget test, inject the matching manual sleeper and advance its clock inside the operation/listener. Use this pattern instead of `thread::sleep`:

```rust
let operation_clock = Arc::clone(&clock);
let result = retry.run(move || {
    operation_clock
        .advance(Duration::from_secs(1))
        .expect("manual time should advance");
    Err::<(), _>(TestError("temporary"))
});
```

Use `ManualAsyncSleeper` plus `wait_for_waiters_async`/`advance` for async attempt deadlines. Do not replace the one test proving default Tokio sleeper first-poll binding; its pre-runtime `thread::sleep` is not a correctness wait and may remain if still needed to distinguish construction time.

- [ ] **Step 5: Replace worker long sleeps with cancellation/channel coordination**

- Cooperative timeout tests loop on `token.is_cancelled()` and `thread::yield_now()`, then return.
- Uncooperative timeout tests block on a channel or barrier owned by the test, assert the terminal result, then release the worker so no detached 120 ms thread remains.
- Keep the real `recv_timeout` deadline small but non-zero because that is the production mechanism under test.

- [ ] **Step 6: Run renamed test modules**

```bash
cargo test --all-features --test lib_tests executor::retry_runner_tests
cargo test --all-features --test lib_tests executor::async_retry_runner_tests
cargo test --all-features --test lib_tests executor::retry_failure_handler_tests
cargo test --all-features --test lib_tests executor::worker_retry_runner_tests
cargo test --all-features --test lib_tests executor::worker_attempt_executor_tests
```

Expected: PASS and no duplicate test names or unregistered files.

- [ ] **Step 7: Audit remaining wall-clock waits**

```bash
rg -n '\b(thread::sleep|std::thread::sleep|tokio::time::sleep)\b' tests
```

For each remaining occurrence, document in the code why real time is the subject under test or replace it using Steps 4–5. No elapsed-budget correctness assertion may depend on `elapsed < N ms` scheduling luck.

---

### Task 6: Complete the authorized Rust style and readability correction

**Files:**
- Create: `rs-retry/src/options/internal/mod.rs`
- Create: `rs-retry/src/options/internal/retry_jitter_factor_format.rs`
- Modify: `rs-retry/src/options/mod.rs`
- Modify: `rs-retry/src/options/retry_jitter.rs`
- Modify: all in-scope `rs-retry/src/**/*.rs`
- Modify: all in-scope `rs-retry/tests/**/*.rs`

**Interfaces:**
- Consumes: green suite after Tasks 1–5.
- Produces: one-type-per-file compliance, complete useful Rustdoc, correct inherent method order, and inline classification without public API changes.

- [ ] **Step 1: Re-inventory types, methods, docs, and inline attributes**

```bash
rg -n '^(pub(?:\([^)]*\))?\s+)?(struct|enum|trait|type)\s+' src
rg -n '^\s*impl(?:<[^>]*>)?\s+[^ ]+' src
rg -n '^\s*#\[inline(?:\(always\))?\]' src
rg -n '^\s*///\s*# (Parameters|Arguments|Returns|Errors|Panics|Safety)' src tests
rg -n 'This (method|function|test) (has no parameters|does not return errors|returns nothing)' src tests
```

Capture the inventory before editing so every source item is rechecked, not sampled.

- [ ] **Step 2: Move `RetryJitterFactorFormat` into an internal file**

Create `src/options/internal/mod.rs` with a restricted re-export:

```rust
// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal retry-option helpers.

mod retry_jitter_factor_format;

pub(in crate::options) use retry_jitter_factor_format::RetryJitterFactorFormat;
```

Move the private struct and its `DisplayWith`/`ParseWith` impls into `retry_jitter_factor_format.rs`, add `mod internal;` in `options/mod.rs`, and import it explicitly in `retry_jitter.rs`. Preserve serialized text and range-validation errors exactly.

- [ ] **Step 3: Normalize Rustdoc without retaining empty templates**

For every function/method/type/field/variant field:

- use `# Arguments` only when non-receiver parameters exist;
- use `# Returns` for non-unit results and explain both `Option` states;
- use `# Errors` only for observable `Err` conditions;
- use `# Panics` for real panic paths such as lock poisoning assumptions or runtime requirements;
- state blocking, sleeping, callback, thread, or async-runtime side effects where applicable;
- remove sections whose only content says there are no parameters, no errors, or no return value.

Mechanical heading conversion is authorized, but inspect each changed block so blank sections are not left behind.

- [ ] **Step 4: Reorder every inherent impl**

Within each inherent impl block:

1. all constructors/factories first (`new`, `builder`, `from_*`, `parse`, `with_*` only when it creates `Self`);
2. constructors ordered `pub`, restricted, private;
3. remaining methods ordered `pub`, restricted, private;
4. functional adjacency only inside one visibility group, getter before setter.

Move complete Rustdoc + attributes + method bodies together. Do not reorder trait impl methods.

- [ ] **Step 5: Apply inline classification to the full source inventory**

Use:

```rust
#[inline(always)] // getters, setters, pure forwarding, and extremely thin wrappers
#[inline]         // other short branch-light functions
// no attribute   // loops, long bodies, or complex control flow
```

Review all existing plain `#[inline]` entries. In particular, trivial `RetryContext`, `RetryOptions`, error accessors, builder forwarders, and listener alias wrappers become `#[inline(always)]`; runner loops and failure-policy control flow remain without inline attributes.

- [ ] **Step 6: Re-run the style inventory and source tests**

```bash
rg -n '^\s*///\s*# Parameters|This (method|function|test) (has no parameters|does not return errors|returns nothing)' src tests
cargo test --all-features --test lib_tests options::retry_jitter_tests
cargo test --all-features --test lib_tests
```

Expected: the obsolete documentation patterns return no matches; tests PASS.

---

### Task 7: Align public documentation with the finalized semantics

**Files:**
- Modify: `rs-retry/README.md`
- Modify: `rs-retry/README.zh_CN.md`
- Modify: `rs-retry/src/executor/retry_builder.rs`
- Modify: `rs-retry/src/event/retry_context.rs`
- Modify: `rs-retry/src/error/retry_error.rs`

**Interfaces:**
- Consumes: final behavior from Tasks 1–4.
- Produces: user-facing contract matching runtime behavior.

- [ ] **Step 1: Document listener order and hard-stop precedence**

State in both READMEs and `RetryBuilder::on_failure` Rustdoc:

```text
on_failure observes every failure produced by an admitted attempt. For a
timeout caused by an exhausted max-operation or max-total budget, all failure
listeners still run exactly once, but their decisions cannot override the hard
budget; on_retry is not emitted.
```

Clarify that sleeper/configuration terminal diagnostics outside an admitted attempt go directly to `on_error`.

- [ ] **Step 2: Document upcoming versus committed attempts**

State that `before_attempt` receives the upcoming one-based ordinal, while terminal `RetryError::attempts()` counts only attempts admitted into execution after the post-listener budget check. Include the first-listener exhaustion example: `before_attempt` sees 1, operation calls are 0, terminal attempts are 0.

- [ ] **Step 3: Run doctests and documentation checks**

```bash
cargo test --all-features --doc
cargo doc --all-features --no-deps
```

Expected: both commands exit 0 without broken intra-doc links.

---

### Task 8: Run repository-prescribed verification and audit every diff

**Files:**
- Inspect: all modified files in `rs-retry`, `rs-http`, and `rs-cas`

**Interfaces:**
- Consumes: completed changes from Tasks 1–7.
- Produces: fresh evidence for formatting, lint, feature combinations, tests, and unchanged public paths.

- [ ] **Step 1: Check all three worktrees before write-capable alignment**

```bash
(cd ../rs-retry && git status --short --branch && git --no-pager diff --stat)
(cd ../rs-http && git status --short --branch && git --no-pager diff --stat)
(cd ../rs-cas && git status --short --branch && git --no-pager diff --stat)
```

Expected: only planned files plus the uncommitted design/plan documents; no staged changes.

- [ ] **Step 2: Run `rs-retry` verification in required order**

```bash
cd ../rs-retry
./align-ci.sh
./ci-check.sh
```

Only if `./ci-check.sh` reports coverage below its threshold:

```bash
./coverage.sh json
```

Record exit status and key output. If alignment edits files, inspect its diff before rerunning failed checks.

- [ ] **Step 3: Run `rs-http` verification in required order**

```bash
cd ../rs-http
./align-ci.sh
./ci-check.sh
```

Run `./coverage.sh json` only on an explicit below-threshold report.

- [ ] **Step 4: Run `rs-cas` verification in required order**

```bash
cd ../rs-cas
./align-ci.sh
./ci-check.sh
```

Run `./coverage.sh json` only on an explicit below-threshold report.

- [ ] **Step 5: Re-run focused cross-crate regressions after all alignment edits**

Run the final renamed `rs-retry` attempt-count and hard-timeout tests, the HTTP in-flight max-duration test, and the CAS timeout report/event test using their exact filters from the preceding tasks. Expected: all PASS.

- [ ] **Step 6: Re-audit public paths, file organization, and diffs**

```bash
(cd ../rs-retry && git diff --check && git --no-pager diff)
(cd ../rs-http && git diff --check && git --no-pager diff)
(cd ../rs-cas && git diff --check && git --no-pager diff)
```

Confirm:

- no public re-export disappeared;
- no production visibility was widened for tests;
- each new Rust file has the canonical header and complete Rustdoc;
- no elapsed-budget test relies on arbitrary real sleep;
- no changes address the deferred boxing/performance topic;
- no `git add`, `git commit`, or `git push` was executed.

- [ ] **Step 7: Prepare the final handoff**

Report per repository: modified files, semantic changes, file moves/re-exports, exact verification commands and exit results, coverage status, unresolved risks, and unchecked scope. Link the design and plan documents and do not claim success for any command not freshly run.
