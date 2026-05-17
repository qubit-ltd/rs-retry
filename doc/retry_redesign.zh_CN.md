# `qubit-retry` 重新设计文档

本文记录 `qubit-retry` 从旧版类型绑定模型迁移到当前实现的设计依据。文档中的公开 API、模块结构和执行语义以当前 `rust-common/rs-retry` 源码为准；结果值驱动的 `run_outcome` 仍是可选后续方向，当前 crate 未实现。

## 1. 背景

旧版设计把“执行控制”、“错误判定”、“结果值判定”、“事件监听”和“配置存储”过早绑定在带成功值类型参数的 executor / builder 上。这样会带来几个问题：

1. 普通错误重试也会被成功值 `T` 的 `Clone` / `Eq` / `Hash` / `Send` / `Sync` 等约束污染。
2. 事件对象如果持有 owned `T`，调用方为了观测 retry 元数据就必须让业务返回值可克隆。
3. 操作错误容易被擦除成 boxed error，依赖方不容易保留自己的错误枚举。
4. 基于 `TypeId` 的错误类型匹配对 boxed dynamic error 不直观，容易给调用方错误预期。
5. 配置读取和执行策略混在一起，会让一次 retry 执行是否使用同一份策略快照变得不清楚。
6. 同步函数无法安全中断任意阻塞闭包，但旧 `operation_timeout` 容易让用户误以为 sync API 能强制中断。

当前实现的目标是：把 retry 框架收敛成“错误类型绑定 + 方法级成功值 + 明确执行模式 + 可观测上下文”的模型。

## 2. 设计目标

1. 核心 `Retry<E>` 只绑定操作错误类型 `E`，成功值 `T` 由每次 `run` / `run_async` / `run_in_worker` 调用引入。
2. 默认场景面向错误重试：操作返回 `Result<T, E>`，终止错误通过 `RetryError<E>` 保留原始 `E`。
3. 事件监听只观察 `RetryContext`、`AttemptFailure<E>` 和 `RetryError<E>`，不持有成功值 `T`。
4. retry 配置是不可变 `RetryOptions` 快照，`qubit-config` 读取只发生在构造阶段。
5. sync、async、worker-thread 三种执行模式明确区分 timeout 能力。
6. `max_operation_elapsed` 和 `max_total_elapsed` 语义分开，避免把用户操作耗时和 retry 控制流耗时混在一起。
7. blocking 操作的 timeout 通过 worker thread、合作式取消 token 和 cancellation grace 表达，不假装能强杀 Rust 线程。

## 3. 非目标

1. 不恢复旧版 `RetryBuilder<T, C>` / `RetryExecutor<T, C>` 兼容层。
2. 不继续支持 TypeId 集合式错误匹配。
3. 不让 retry 框架接管业务成功值判定；业务是否把成功结果视为可重试，优先由业务转换成显式错误。
4. 不在普通 sync `run()` 中实现强制中断。
5. 不在当前阶段引入 circuit breaker、hedging、bulkhead 等更高层 resilience 能力。

## 4. 核心设计决策

### 4.1 `Retry<E>` 不绑定成功值 `T`

当前核心类型是 `Retry<E>`：

```rust
pub struct Retry<E = BoxError> {
    options: RetryOptions,
    retry_after_hint: Option<RetryAfterHint<E>>,
    isolate_listener_panics: bool,
    listeners: RetryListeners<E>,
}
```

执行方法再引入成功值：

```rust
impl<E> Retry<E> {
    pub fn run<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>;

    pub async fn run_async<T, F, Fut>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>;

    pub fn run_in_worker<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: Fn(AttemptCancelToken) -> Result<T, E> + Send + Sync + 'static;
}
```

这样普通错误重试不会要求 `T: Clone + Eq + Hash`。

### 4.2 错误判定由 failure listener 表达

当前没有独立的 `ErrorClassifier` / `RetryDecision` 类型。错误判定通过 `on_failure` 监听器返回 `AttemptFailureDecision`：

```rust
pub enum AttemptFailureDecision {
    UseDefault,
    Retry,
    RetryAfter(Duration),
    Abort,
}
```

便捷 API `retry_if_error` 只处理 `AttemptFailure::Error(E)`，返回 `true` 表示 retry，返回 `false` 表示 abort；timeout、panic、executor failure 仍交给默认策略或其他 failure listener。

默认策略是：普通应用错误可重试直到限制耗尽；configured attempt timeout 按 `AttemptTimeoutPolicy`；panic 和 executor failure 默认 abort。

### 4.3 终止错误保留原始错误类型

`RetryError<E>` 是结构体而不是多变体枚举。终止原因、最后一次 attempt failure 和终止时的上下文分开保存：

```rust
pub struct RetryError<E> {
    reason: RetryErrorReason,
    last_failure: Option<AttemptFailure<E>>,
    context: RetryContext,
}
```

`AttemptFailure<E>` 表示一次 attempt 的失败：

```rust
pub enum AttemptFailure<E> {
    Error(E),
    Timeout,
    Panic(AttemptPanic),
    Executor(AttemptExecutorError),
}
```

调用方可以通过 `reason()`、`last_failure()`、`last_error()`、`into_last_error()`、`context()`、`attempt_timeout_source()` 和 `unreaped_worker_count()` 读取终止信息。若最后一次失败是业务错误，原始 `E` 不会被擦除。

### 4.4 监听器只传上下文和失败引用

当前 listener 是生命周期 hook，不是单独的策略系统：

1. `before_attempt(&RetryContext)`：每次 attempt 开始前触发。
2. `on_success(&RetryContext)`：attempt 成功后触发。
3. `on_failure(&AttemptFailure<E>, &RetryContext) -> AttemptFailureDecision`：attempt 失败后触发，可影响 retry / abort / retry-after。
4. `on_retry(&AttemptFailure<E>, &RetryContext)`：已确定会 retry 且 delay 已选定后触发，只观察，不改策略。
5. `on_error(&RetryError<E>, &RetryContext)`：整个 retry flow 终止失败时触发。

listener 不持有成功值 `T`。如果业务需要记录成功值，应在 operation 内自行记录，而不是让 retry 框架克隆返回值。

### 4.5 配置是 `RetryOptions` 快照

`RetryOptions` 持有与错误类型无关的执行策略：attempt 次数、elapsed budget、delay、jitter、attempt timeout、worker cancellation grace。配置从 `qubit-config` 读取时先进入 `RetryConfigValues`，再合并为 `RetryOptions`。

重要约束：

1. `max_attempts` 内部使用 `NonZeroU32`，构造时拒绝 0。
2. `RetryDelay::None` 允许零 delay；其他固定、随机、指数 delay 中不接受无意义的零值。
3. `RetryJitter::Factor` 必须是有限的 `[0.0, 1.0]`。
4. `AttemptTimeoutOption` 的 timeout 必须大于 0。
5. `RetryDelay::Random` 的采样边界必须能无损表示为 `u64` 纳秒，避免极大 `Duration` 被饱和后破坏 min/max 语义。

### 4.6 Delay 和 Jitter 拆分

`RetryDelay` 负责基础 delay：

```rust
pub enum RetryDelay {
    None,
    Fixed(Duration),
    Random { min: Duration, max: Duration },
    Exponential { initial: Duration, max: Duration, multiplier: f64 },
}
```

`RetryJitter` 在基础 delay 之后应用：

```rust
pub enum RetryJitter {
    None,
    Factor(f64),
}
```

`Factor(0.2)` 表示围绕 base delay 做对称抖动：`base +/- 20%`，下限 clamp 到 0。当前实现使用 `rand::rng()` 采样；测试覆盖范围和边界，不依赖固定随机种子。

### 4.7 timeout 与 elapsed budget

当前实现明确区分三类时间约束：

1. `max_operation_elapsed`：累计用户 operation attempt 的执行时间。retry sleep、Retry-After sleep、hint 提取和 listener 时间不计入。
2. `max_total_elapsed`：整个 retry flow 的单调时间。operation、retry sleep、Retry-After sleep、hint 提取、`before_attempt`、`on_failure` 和 `on_retry` 时间都计入。
3. `attempt_timeout`：单次 attempt timeout。async 通过 `tokio::time::timeout` 生效；worker-thread 通过 `recv_timeout`、`AttemptCancelToken` 和 `worker_cancel_grace` 生效。

async / worker attempt 的有效 timeout 是 configured attempt timeout、剩余 operation budget、剩余 total budget 三者中最短的一个。平局时 configured timeout 优先，以保留 `AttemptTimeoutPolicy` 的可观察语义；operation budget 与 total budget 平局时 operation budget 优先。

普通 sync `run()` 不支持 configured attempt timeout。如果配置了 attempt timeout，`run()` 返回 `RetryErrorReason::UnsupportedOperation`，提示使用 `run_async()` 或 `run_in_worker()`。

## 5. 推荐公开 API

### 5.1 基础错误重试

```rust
use qubit_retry::Retry;
use std::time::Duration;

let retry = Retry::<std::io::Error>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .build()?;

let text = retry.run(|| std::fs::read_to_string("config.toml"))?;
```

### 5.2 自定义错误判定

```rust
let retry = Retry::<HttpError>::builder()
    .max_attempts(3)
    .exponential_backoff(Duration::from_millis(200), Duration::from_secs(5))
    .retry_if_error(|error: &HttpError, _context: &RetryContext| {
        error.retry_hint() == RetryHint::Retryable
    })
    .build()?;

let response = retry
    .run_async(|| async { client.execute_once(request.clone()).await })
    .await?;
```

更复杂的判定可以使用 `on_failure`，例如针对 rate limit 返回 `AttemptFailureDecision::RetryAfter(delay)`。

### 5.3 注册监听器

```rust
let retry = Retry::<HttpError>::builder()
    .max_attempts(3)
    .on_failure(|failure: &AttemptFailure<HttpError>, context: &RetryContext| {
        tracing::warn!(
            attempt = context.attempt(),
            retry_after_hint = ?context.retry_after_hint(),
            failure = ?failure,
            "attempt failed",
        );
        AttemptFailureDecision::UseDefault
    })
    .on_retry(|failure: &AttemptFailure<HttpError>, context: &RetryContext| {
        tracing::info!(
            attempt = context.attempt(),
            next_delay = ?context.next_delay(),
            failure = ?failure,
            "retry scheduled",
        );
    })
    .on_error(|error: &RetryError<HttpError>, context: &RetryContext| {
        tracing::error!(
            reason = ?error.reason(),
            attempts = context.attempt(),
            total_elapsed_ms = context.total_elapsed().as_millis(),
            "retry flow failed",
        );
    })
    .build()?;
```

## 6. 执行流程

错误重试流程可以概括为：

```text
state = { attempts: 0, operation_elapsed: 0, last_failure: None }

loop:
  if operation/total elapsed budget 已耗尽:
    return RetryError(reason, last_failure, context)

  attempts += 1
  emit before_attempt(context)

  if listener 时间导致 elapsed budget 耗尽:
    return RetryError(reason, last_failure, context)

  result = run attempt
    - sync: 直接调用 operation
    - async: 必要时用 tokio::time::timeout 包住 future
    - worker: spawn worker thread + recv / recv_timeout

  if Ok(value):
    emit on_success(context)
    return Ok(value)

  failure = Error(E) | Timeout | Panic | Executor

  if timeout 来自 elapsed budget:
    return MaxOperationElapsedExceeded 或 MaxTotalElapsedExceeded

  hint = retry_after_hint(failure, context)
  decision = merge on_failure decisions with default policy

  if decision == Abort:
    return Aborted

  if worker timeout 后未在 grace 内退出:
    return WorkerStillRunning

  if attempts >= max_attempts:
    return AttemptsExceeded

  delay = RetryAfter / hint / configured delay + jitter
  if delay 会耗尽 max_total_elapsed:
    return MaxTotalElapsedExceeded

  emit on_retry(context with next_delay)
  if on_retry listener 时间耗尽 max_total_elapsed:
    return MaxTotalElapsedExceeded

  sleep delay
  last_failure = failure
```

retry sleep 不截断：如果剩余 total budget 不足以睡完整 delay 并进入下一次 attempt，流程在 sleep 前失败。

## 7. 当前模块结构

```text
src/
  lib.rs
  constants.rs
  error/
    attempt_executor_error.rs
    attempt_failure.rs
    attempt_panic.rs
    retry_config_error.rs
    retry_error.rs
    retry_error_reason.rs
    mod.rs
  event/
    attempt_failure_decision.rs
    attempt_failure_listener.rs
    attempt_success_listener.rs
    attempt_timeout_source.rs
    before_attempt_listener.rs
    retry_after_hint.rs
    retry_context.rs
    retry_context_parts.rs
    retry_error_listener.rs
    retry_events.rs
    retry_listeners.rs
    mod.rs
  executor/
    async_attempt.rs
    async_attempt_future.rs
    async_retry_runner.rs
    async_value_operation.rs
    attempt_cancel_token.rs
    blocking_attempt.rs
    blocking_attempt_outcome.rs
    blocking_value_operation.rs
    retry.rs
    retry_builder.rs
    retry_failure_handler.rs
    retry_failure_policy.rs
    retry_flow_action.rs
    retry_flow_state.rs
    attempt.rs
    retry_runner.rs
    value_operation.rs
    worker_attempt_executor.rs
    worker_retry_runner.rs
    mod.rs
  options/
    attempt_timeout_option.rs
    attempt_timeout_policy.rs
    effective_attempt_timeout.rs
    parse_retry_jitter_error.rs
    retry_config_values.rs
    retry_delay.rs
    retry_delay_duration_format.rs
    retry_jitter.rs
    retry_options.rs
    mod.rs
```

`lib.rs` 对外 re-export `Retry`、`RetryBuilder`、`RetryOptions`、`RetryDelay`、`RetryJitter`、timeout 类型、context/listener 相关类型和错误类型。`RetryListeners`、`RetryContextParts`、attempt adapter、worker message、flow action 等保持 crate 内部可见。

## 8. 对 `qubit-http` 的影响

`qubit-http` 可用 `Retry<HttpError>` 保留 HTTP 层错误类型：

```rust
fn build_retry(&self) -> Retry<HttpError> {
    Retry::<HttpError>::builder()
        .max_attempts(self.options.retry.max_attempts)
        .max_total_elapsed(self.options.retry.max_duration)
        .delay(self.options.retry.delay.clone())
        .jitter(self.options.retry.jitter)
        .retry_if_error(|error, _context| error.retry_hint() == RetryHint::Retryable)
        .build()
        .expect("validated retry options")
}
```

执行失败后，`RetryError<HttpError>` 仍可通过 `last_error()` 或 `into_last_error()` 取回原始 `HttpError`，也可以读取 retry context 后再映射成 HTTP 层错误。

## 9. 当前状态与兼容性

当前 crate 已完成的主体工作：

1. `Retry<E>` 只绑定错误类型，成功值由执行方法引入。
2. `RetryBuilder<E>` 提供 max attempts、delay、jitter、elapsed budget、attempt timeout、listener、retry-after hint 等配置入口。
3. `RetryOptions` 是不可变策略快照，支持可选 `config` feature。
4. `RetryError<E>` / `AttemptFailure<E>` 保留 typed error、timeout、panic、executor failure 和 context。
5. `run_async()` 和 `run_in_worker()` 支持真实 per-attempt timeout；`run()` 明确拒绝 configured attempt timeout。
6. worker timeout 采用合作式取消；未在 grace 内退出时 fail-closed 为 `WorkerStillRunning`，避免叠加不可回收 worker。
7. README、英文/中文文档和集成测试按当前 API 维护。

仍未实现的可选方向：

1. result-based retry / `run_outcome`。
2. circuit breaker、hedging、bulkhead 等高层 resilience 能力。
3. 可注入随机源的 deterministic jitter 测试接口。

## 10. 测试覆盖

当前测试重点覆盖：

1. 默认错误重试、显式 abort、attempts exhausted、typed error 取回。
2. sync `run()` 对 configured attempt timeout 的 unsupported 行为。
3. async timeout、elapsed budget 截断、listener 时间计入 total budget。
4. worker thread 执行、panic 捕获、timeout、合作式取消、未回收 worker fail-closed。
5. retry-after hint、`RetryAfter` decision、`on_retry` delay 观测。
6. `RetryDelay` / `RetryJitter` 的解析、序列化、边界校验和 delay 计算。
7. `RetryOptions::from_config` 的显式/隐式 delay、timeout policy、unlimited budget 合并。
8. 错误类型 display/source/accessor 行为。

CI 还会运行格式检查、Clippy、style-check、debug/release build、all-feature tests、rustdoc、coverage 和 security audit。

## 11. 推荐结论

当前推荐继续沿用“`Retry<E>` + 方法级 `T` + typed `RetryError<E>` + explicit execution mode”的设计。

这个模型解决了 redesign 的核心问题：

1. 成功值 `T` 不再被 result retry 和事件系统绑架。
2. HTTP 等依赖方可以保留自己的错误类型。
3. 错误判定由显式 listener / predicate 完成，不依赖 TypeId 集合。
4. 配置是一次构造出的快照，执行行为容易测试和推理。
5. timeout 能力与执行模式匹配，不向 sync API 承诺无法安全实现的中断能力。
6. result-based retry 仍可作为高级 API 单独设计，不污染主要错误重试路径。
