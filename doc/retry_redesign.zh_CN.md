# `qubit-retry` 重新设计文档

本文档前部（§1–§3）说明旧版问题与设计目标；**自 §4 起的公开类型、监听器签名、§7 目录结构、§5.3 示例已与当前 `rust-common/rs-retry` 源码一致**。§5.4 描述的 result-based / `run_outcome` 等 API **尚未在本 crate 实现**，仍标记为可选后续方向。

## 1. 背景

当前 `qubit-retry` 的实现把“执行控制”、“错误判定”、“结果值判定”、“事件监听”和“配置存储”全部绑定在 `RetryBuilder<T, C>` / `RetryExecutor<T, C>` 上。这样做在简单场景下可用，但对多数真实调用方并不友好：

1. 基础错误重试也要求成功值 `T: Clone + PartialEq + Eq + Hash + Send + Sync + 'static`。
2. 这些约束来自 result-based retry 和事件持有 owned result，但被提前加到了所有 API 上。
3. 操作错误被擦除为 `Box<dyn Error>` 或 `RetryError`，调用方很难保留自己的错误类型。
4. `failed_on_error::<E>()` / `abort_on_error::<E>()` 的 TypeId 设计并没有真正对 boxed dynamic error 做可靠类型匹配，容易给调用方错误预期。
5. `DefaultRetryConfig` 在运行时从 `Config` 中反复读取配置，使“策略快照”和“配置存储”混在一起。
6. 同步 `operation_timeout` 只能后置检查，不能真正中断操作，但 API 表达上容易让用户误解。
7. 事件对象持有 owned `T`，直接导致 `Clone` 要求；很多场景只需要 attempt、delay、elapsed、error 等元数据。

由于包还没有对外发布，本设计不考虑旧版 API 兼容性，目标是把 API 调整为更符合 Rust 使用习惯的形态。

## 2. 设计目标

1. 核心 retry API 不再对成功值 `T` 施加 `Clone/Eq/Hash/Send/Sync/'static` 约束。
2. 默认场景面向错误重试：操作返回 `Result<T, E>`，retry executor 基于 `&E` 判断是否重试。
3. 终止错误保留原始错误类型 `E`，不强制转成 `RetryError` 或 `Box<dyn Error>`。
4. result-based retry 作为高级 API 独立存在，只有使用它时才引入与 `T` 相关的复杂性。
5. retry 配置是不可变 value object，运行期间不从外部配置源反复读取。
6. 事件监听不持有 owned `T`，避免为了观测而污染执行 API 的类型约束。
7. async timeout 语义要真实、明确；sync API 不伪装能中断同步操作。
8. `qubit-http` 可以自然基于 `HttpError::retry_hint()` 接入，不需要胶水代码或额外保存最后错误。

## 3. 非目标

1. 不保留 `RetryBuilder<T, C>` / `RetryExecutor<T, C>` 的旧 API 兼容性。
2. 不继续支持“通过 TypeId 配置错误类型集合”的旧错误判定模型。
3. 不让 retry 框架接管业务错误建模。业务是否把某个成功结果视作可重试，应优先由业务转换成显式错误。
4. 不在 sync `run()` 中实现强制中断。Rust 中安全中断任意同步闭包不可行。
5. 不在第一阶段引入复杂 circuit breaker、hedging、bulkhead 等 resilience 能力。

## 4. 核心设计决策

### 4.1 Executor 不再绑定成功值 `T`

当前 crate 中的核心类型：

```rust
use qubit_common::BoxError;

#[derive(Clone)]
pub struct RetryExecutor<E = BoxError> {
    options: RetryOptions,
    classifier: ErrorClassifier<E>,
    listeners: RetryListeners<E>, // pub(crate)，见 `events/listeners.rs`
}
```

`RetryExecutor` 只绑定错误类型 `E`，不绑定成功值 `T`。执行方法再引入 `T`：

```rust
impl<E> RetryExecutor<E> {
    pub fn run<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Result<T, E>;

    pub async fn run_async<T, F, Fut>(&self, operation: F) -> Result<T, RetryError<E>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>;
}
```

这样成功值 `T` 只被闭包返回和最终返回使用，不参与策略存储，也不需要 `Clone/Eq/Hash`。

### 4.2 错误分类改为闭包或 trait，不使用 TypeId

- `RetryDecision` 定义在 `src/events/retry_decision.rs`，由 `qubit_retry::RetryDecision`（crate 根与 `events` 子模块）导出。
- `AttemptContext` 定义在 `src/events/attempt_context.rs`，分类器与重试监听都会用到。
- `ErrorClassifier<E>` 定义在 `src/error/error_classifier.rs`，类型为 `qubit_function::ArcBiFunction<E, AttemptContext, RetryDecision>`（内部用 `Arc` 共享，便于 `RetryExecutor` 克隆），crate 根与 `qubit_retry::error::ErrorClassifier` 均可导入。
- `RetryError<E>`、`AttemptFailure<E>` 仍在 `error` 子模块。

```rust
pub enum RetryDecision {
    Retry,
    Abort,
}

// `ArcBiFunction` 来自依赖 `qubit-function`
pub type ErrorClassifier<E> =
    qubit_function::ArcBiFunction<E, AttemptContext, RetryDecision>;
```

默认行为建议为“重试所有错误”，可用 builder 覆盖：

```rust
let executor = RetryExecutor::<HttpError>::builder()
    .max_attempts(3)
    .retry_if(|error, _ctx| error.retry_hint() == RetryHint::Retryable)
    .build()?;
```

其中 `retry_if` 是便捷接口，接收返回 `bool` 的闭包：`true` 映射为 `RetryDecision::Retry`，`false` 映射为 `RetryDecision::Abort`。如果调用方需要更明确的命名，也可以提供 `classify_error`：

```rust
let executor = RetryExecutor::<HttpError>::builder()
    .classify_error(|error, _ctx| {
        if error.retry_hint() == RetryHint::Retryable {
            RetryDecision::Retry
        } else {
            RetryDecision::Abort
        }
    })
    .build()?;
```

不再提供 `failed_on_error::<E>()` 这种 TypeId API。需要按错误类型判断时，调用方可以在自己的错误枚举或 `downcast_ref` 中显式处理。

### 4.3 终止错误保留原始 `E`

新版错误类型建议：

```rust
pub enum RetryError<E> {
    Aborted {
        attempts: u32,
        elapsed: Duration,
        failure: AttemptFailure<E>,
    },
    AttemptsExceeded {
        attempts: u32,
        max_attempts: u32,
        elapsed: Duration,
        last_failure: AttemptFailure<E>,
    },
    MaxElapsedExceeded {
        attempts: u32,
        elapsed: Duration,
        max_elapsed: Duration,
        last_failure: Option<AttemptFailure<E>>,
    },
}

pub enum AttemptFailure<E> {
    Error(E),
    AttemptTimeout {
        elapsed: Duration,
        timeout: Duration,
    },
}
```

好处：

1. 如果最后一次失败是业务错误，调用方能拿回原始 `E`。
2. 如果最后一次失败是 async attempt timeout，也能表达为 retry 框架生成的失败。
3. `RetryError<E>` 可以在 `E: Error + 'static` 时实现 `std::error::Error`，但不强制所有 `E` 都是 `Error`。

当前实现提供的便捷方法：

```rust
impl<E> RetryError<E> {
    pub fn attempts(&self) -> u32;
    pub fn elapsed(&self) -> Duration;
    pub fn last_failure(&self) -> Option<&AttemptFailure<E>>;
    pub fn last_error(&self) -> Option<&E>;
    pub fn into_last_error(self) -> Option<E>;
}
```

`qubit-http` 可以把 `AttemptsExceeded { last_failure: Error(error), .. }` 映射回 `HttpError`，同时追加 retry 上下文。

### 4.4 监听器：Context 元数据 + 单独传入的失败

监听器不持有成功值 `T`：重试/失败/终止路径使用 **Copy 的 context 结构体** 描述元数据，**`AttemptFailure<E>` 以引用**传给回调（由执行器在调用栈上借出）。成功路径只有 **`SuccessContext`**（`attempts` + `elapsed`）。

```rust
pub struct RetryContext {
    pub attempt: u32,
    pub max_attempts: u32,
    pub elapsed: Duration,
    pub next_delay: Duration,
}

pub struct SuccessContext {
    pub attempts: u32,
    pub elapsed: Duration,
}

pub struct FailureContext {
    pub attempts: u32,
    pub elapsed: Duration,
}

pub struct AbortContext {
    pub attempts: u32,
    pub elapsed: Duration,
}
```

监听器类型别名（`ArcBiConsumer` / `ArcConsumer` 来自 `qubit-function`；定义见 `src/events/listeners.rs`）：

```rust
pub type RetryListener<E> =
    qubit_function::ArcBiConsumer<RetryContext, AttemptFailure<E>>;
pub type SuccessListener = qubit_function::ArcConsumer<SuccessContext>;
pub type FailureListener<E> =
    qubit_function::ArcBiConsumer<FailureContext, Option<AttemptFailure<E>>>;
pub type AbortListener<E> =
    qubit_function::ArcBiConsumer<AbortContext, AttemptFailure<E>>;
```

这样 listener 不需要持有 `T`，核心 API 也不需要 `T: Clone`。若要观测成功业务值，应在业务 `operation` 内自行记录，而不是由 retry 框架克隆结果。

### 4.5 配置改成 `RetryOptions` 快照

建议删除 `RetryConfig` trait、`DefaultRetryConfig`、`SimpleRetryConfig` 这组三层结构，改为单一 value object：

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RetryOptions {
    pub max_attempts: NonZeroU32,
    pub max_elapsed: Option<Duration>,
    pub delay: Delay,
    pub jitter: Jitter,
}
```

配置读取放在转换层：

```rust
impl RetryOptions {
    pub fn from_config<R: ConfigReader + ?Sized>(config: &R) -> Result<Self, RetryConfigError>;
}
```

原则：

1. `RetryExecutor` 持有 `RetryOptions` 快照。
2. 从 `qubit-config` 读取只发生在构造阶段。
3. `build()` 做完整校验，返回 `Result<RetryExecutor<E>, RetryConfigError>`。
4. `max_attempts` 用 `NonZeroU32`，避免 `0` 的语义歧义。
5. `Duration::ZERO` 只允许用于 `Delay::None`；其他 delay/timeout 为零时直接配置错误。

### 4.6 Delay 和 Jitter 拆清楚

当前 `RetryDelayStrategy::calculate_delay(attempt, jitter_factor)` 同时处理基础 backoff 和 jitter，且 jitter 只向上增加 delay。新版建议拆成：

```rust
pub enum Delay {
    None,
    Fixed(Duration),
    Random { min: Duration, max: Duration },
    Exponential {
        initial: Duration,
        max: Duration,
        multiplier: f64,
    },
}

pub enum Jitter {
    None,
    Factor(f64),
}
```

计算流程：

```rust
let base = options.delay.base_delay(attempt);
let delay = options.jitter.apply(base, rng);
```

`Jitter::Factor(0.2)` 建议语义为对称抖动：`base ± base * factor`，下限 clamp 到 `Duration::ZERO`。这比“只加不减”更符合常见 jitter 直觉。

如果希望可测试性更强，可以把随机源封装在内部 `RandomSource`，测试里用 seeded RNG；第一阶段可以只保证 delay 范围测试。

### 4.7 timeout 语义

建议明确区分：

1. `max_elapsed`：整个 retry 流程的总预算，sync/async 都适用。
2. `attempt_timeout`：单次 attempt 超时，只在 async API 中真正生效，因此不放进基础 `RetryOptions`。

基础 sync API 不支持单次 attempt timeout：

```rust
pub fn run<T, F>(&self, operation: F) -> Result<T, RetryError<E>>
where
    F: FnMut() -> Result<T, E>;
```

async 单次 timeout 用显式方法表达：

```rust
pub async fn run_async_with_timeout<T, F, Fut>(
    &self,
    attempt_timeout: Duration,
    operation: F,
) -> Result<T, RetryError<E>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>;
```

这样 sync `run()` 不会因为 executor 中存在 timeout 配置而产生“配置错误混入执行错误”的问题，也不会假装能中断同步闭包。

如果以后要支持 sync timeout，应提供单独 API，例如 `run_blocking_with_timeout()`，通过线程或进程隔离实现，但这不是第一阶段目标。

## 5. 推荐公开 API

### 5.1 基础错误重试

```rust
use qubit_retry::{Delay, RetryExecutor};
use std::time::Duration;

let executor = RetryExecutor::<std::io::Error>::builder()
    .max_attempts(3)
    .delay(Delay::fixed(Duration::from_millis(100)))
    .build()?;

let text = executor.run(|| std::fs::read_to_string("config.toml"))?;
```

默认重试所有 `Err(E)`。

### 5.2 自定义错误判定

```rust
let executor = RetryExecutor::<HttpError>::builder()
    .max_attempts(3)
    .delay(Delay::exponential(
        Duration::from_millis(200),
        Duration::from_secs(5),
        2.0,
    ))
    .retry_if(|error, _ctx| error.retry_hint() == RetryHint::Retryable)
    .build()?;

let response = executor
    .run_async(|| async { client.execute_once(request.clone()).await })
    .await?;
```

### 5.3 注册监听器

```rust
let executor = RetryExecutor::<HttpError>::builder()
    .max_attempts(3)
    .on_retry(|context, failure| {
        tracing::warn!(
            attempt = context.attempt,
            delay_ms = context.next_delay.as_millis(),
            failure = ?failure,
            "retrying failed operation",
        );
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

监听器只依赖错误类型 `E`，不依赖成功值 `T`。

### 5.4 高级：结果值 retry（**当前 crate 未实现**）

以下类型与 `run_outcome` 仍属设计草案：若未来引入，建议把 result-based retry 从核心 `RetryExecutor<E>` 中拆成显式 API：

```rust
pub enum OutcomeDecision {
    Succeed,
    Retry,
    Abort,
}

pub enum OutcomeRef<'a, T, E> {
    Success(&'a T),
    Error(&'a E),
    AttemptTimeout { elapsed: Duration, timeout: Duration },
}

pub enum Outcome<T, E> {
    Success(T),
    Error(E),
    AttemptTimeout { elapsed: Duration, timeout: Duration },
}

pub enum OutcomeRetryError<T, E> {
    Aborted {
        attempts: u32,
        elapsed: Duration,
        outcome: Outcome<T, E>,
    },
    AttemptsExceeded {
        attempts: u32,
        max_attempts: u32,
        elapsed: Duration,
        last_outcome: Outcome<T, E>,
    },
    MaxElapsedExceeded {
        attempts: u32,
        elapsed: Duration,
        max_elapsed: Duration,
        last_outcome: Option<Outcome<T, E>>,
    },
}
```

API：

```rust
let value = executor
    .run_outcome(
        || fetch_page(),
        |outcome, _ctx| match outcome {
            OutcomeRef::Success(page) if page.is_empty() => OutcomeDecision::Retry,
            OutcomeRef::Success(_) => OutcomeDecision::Succeed,
            OutcomeRef::Error(error) if error.is_transient() => OutcomeDecision::Retry,
            OutcomeRef::Error(_) => OutcomeDecision::Abort,
            OutcomeRef::AttemptTimeout { .. } => OutcomeDecision::Retry,
        },
    )?;
```

若将来实现 `run_outcome`，终止错误才可能携带成功值 `T`；当前 crate 提供的 `run` / `run_async` / `run_async_with_timeout` 不受影响。

## 6. 执行流程

错误重试流程：

```text
attempt = 1
last_failure = None

loop:
  if max_elapsed exceeded:
    调用 failure 监听（FailureContext, last_failure）
    return RetryError::MaxElapsedExceeded { last_failure, .. }

  result = run attempt
    - async + run_async_with_timeout: tokio::time::timeout
    - sync: direct call

  if Ok(value):
    调用 success 监听（SuccessContext）
    return Ok(value)

  failure = Error(error) or AttemptTimeout

  if classifier says Abort:
    调用 abort 监听（AbortContext, failure）
    return RetryError::Aborted { failure, .. }

  if attempt >= max_attempts:
    调用 failure 监听（FailureContext, Some(failure)）
    return RetryError::AttemptsExceeded { last_failure: failure, .. }

  delay = calculate_delay(attempt)
  if max_elapsed would be exceeded by sleeping delay:
    调用 failure 监听（FailureContext, Some(failure)）
    return RetryError::MaxElapsedExceeded { last_failure: Some(failure), .. }

  调用 retry 监听（RetryContext, failure）
  sleep delay
  last_failure = failure
  attempt += 1
```

注意：`max_elapsed` 是否允许“截断 sleep 后再试一次”需要明确。推荐第一阶段不截断，预算不足以完成下一次 delay 时直接失败，行为更可预测。

## 7. 当前模块结构（与仓库一致）

```text
src/
  lib.rs                      # 对外 re-export：RetryExecutor、RetryOptions、Delay、Jitter、
                              # error 类型、events 中的 Context / RetryDecision / Listener 别名
  retry_executor.rs           # RetryExecutor<E>：run / run_async / run_async_with_timeout
  retry_executor_builder.rs   # RetryExecutorBuilder<E>
  retry_options.rs            # RetryOptions、校验、RetryOptions::from_config
  failure_action.rs           # 内部：同步路径失败后的下一步（重试 sleep 或终止）
  delay.rs                    # Delay
  jitter.rs                   # Jitter
  events.rs                   # events 子模块入口
  events/
    abort_context.rs
    attempt_context.rs
    failure_context.rs
    listeners.rs              # RetryListeners<E>（crate 内）、*Listener 类型别名
    retry_context.rs
    retry_decision.rs         # RetryDecision
    success_context.rs
  error.rs                    # 聚合 error 子模块
  error/
    attempt_failure.rs
    error_classifier.rs       # ErrorClassifier<E>
    retry_config_error.rs
    retry_error.rs
```

其中 `RetryError<E>`、`AttemptFailure<E>`、`RetryConfigError`、`ErrorClassifier<E>` 由 `error.rs` re-export；`RetryContext`、`SuccessContext`、`FailureContext`、`AbortContext`、`RetryDecision` 与各 `*Listener` 由 `events.rs` 汇总，并在 `lib.rs` 根再导出，便于 `use qubit_retry::{RetryExecutor, RetryContext, …}`。

旧版中的 `RetryConfig` trait、`DefaultRetryConfig` / `SimpleRetryConfig`、`event/retry_reason` 等已不再存在；**`outcome.rs` / `run_outcome` 尚未添加**（见 §5.4）。

## 8. 对 `qubit-http` 的影响

`qubit-http` 的 retry 接入会更简单：

```rust
fn build_retry_executor(&self) -> RetryExecutor<HttpError> {
    RetryExecutor::<HttpError>::builder()
        .max_attempts(self.options.retry.max_attempts)
        .max_elapsed(self.options.retry.max_duration)
        .delay(self.options.retry.delay.clone())
        .jitter(self.options.retry.jitter)
        .retry_if(|error, _ctx| error.retry_hint() == RetryHint::Retryable)
        .build()
        .expect("validated retry options")
}
```

执行：

```rust
let result = executor
    .run_async(|| async {
        let request = request.clone();
        self.execute_once(request).await
    })
    .await;
```

错误映射：

```rust
match result {
    Ok(response) => Ok(response),
    Err(error) => map_retry_error_to_http_error(error),
}
```

`HttpResponse` / `HttpStreamResponse` 不需要 `Clone/Eq/Hash`，因为 `T` 只存在于执行方法的返回值中。

## 9. 兼容性策略（历史决策与现状）

重构目标按破坏性变更推进；**当前 `qubit-retry` 已落地**的主要项包括：

1. 以 `RetryExecutor<E>` + `RetryExecutorBuilder<E>` 替代旧 `RetryBuilder<T, C>` / `RetryExecutor<T, C>` 形态。
2. 以不可变 `RetryOptions` + `RetryOptions::from_config` 替代 `RetryConfig` trait 与多层默认配置类型。
3. 以 `classify_error` / `retry_if` + `ErrorClassifier` 闭包替代 TypeId 集合式错误匹配。
4. 以 `Delay` + `Jitter` 替代一体的 `RetryDelayStrategy` 语义。
5. 文档与集成测试已按当前 API 维护。

**仍未纳入本 crate 的项**：§5.4 所述 result-based / `run_outcome` 等（若需要再单独立项实现）。

## 10. 测试计划

### 10.1 executor 与执行流程

1. 默认策略重试所有错误直到成功。
2. `retry_if` 返回 `Abort` 时立即返回 `RetryError::Aborted`，并保留原始错误。
3. 超过 `max_attempts` 时返回 `AttemptsExceeded`，并保留最后一次错误。
4. `max_elapsed` 在首次 attempt 前超限时返回 `MaxElapsedExceeded { last_failure: None }`。
5. `max_elapsed` 在一次失败后超限时返回 `MaxElapsedExceeded { last_failure: Some(...) }`。
6. delay 不足以进入下一次 attempt 时直接失败。
7. `Delay::None` 不 sleep。
8. `Delay::Fixed`、`Delay::Random`、`Delay::Exponential` 计算结果正确。
9. `Jitter::Factor` 输出在合法区间内。

### 10.2 async timeout

1. async operation 在 timeout 内成功。
2. async operation 超过 `attempt_timeout` 后生成 `AttemptFailure::AttemptTimeout`。
3. timeout 可按策略重试并最终成功。
4. timeout 重试耗尽后返回 `AttemptsExceeded`，last failure 是 timeout。
5. 基础 sync `run()` 没有 attempt timeout API，避免表达不可实现的中断能力。

### 10.3 监听器（Context + 失败引用）

1. `on_retry`：`RetryContext`（含 attempt、max_attempts、elapsed、next_delay）与 `AttemptFailure<E>` 引用。
2. `on_success`：`SuccessContext`（attempts、elapsed）。
3. `on_failure`：`FailureContext` 与 `Option<AttemptFailure<E>>`（在 attempts 用尽、max_elapsed 在 sleep 前/后触发、或首次 attempt 前预算耗尽等终止场景）。
4. `on_abort`：`AbortContext` 与 `AttemptFailure<E>`（分类器判定 Abort）。
5. 监听器不要求成功值 `T: Clone`。

### 10.4 类型约束

用编译测试或普通测试覆盖：

1. `T` 不实现 `Clone` 也可以使用基础 `run_async`。
2. `T` 不实现 `Eq` / `Hash` 也可以使用基础 `run_async`。
3. `E` 可以是业务错误枚举，终止错误中能取回原始 `E`。
4. 若引入 result-based `run_outcome`（§5.4），才需要把 `T` 放入终止错误类型；当前 crate 无此 API。

## 11. 实施步骤（回顾）

仓库内已完成等价于原「第 1–2 步 + 文档/测试」的主体工作：`RetryOptions` / `Delay` / `Jitter` / `RetryError<E>` / `AttemptFailure<E>`、`RetryExecutor<E>` 与 `run` / `run_async` / `run_async_with_timeout`、监听器与 Context 模型、集成测试与 README。

依赖方（如 `qubit-http`）的迁移与 §5.4 高级 API 属后续可选任务，不在本文强制范围。

## 12. 推荐结论

推荐采用“`RetryExecutor<E>` + 方法级 `T` + typed `RetryError<E>`”的设计。

这能解决当前最核心的问题：

1. 成功值 `T` 不再被 result retry 和事件系统绑架。
2. HTTP 等库可以保留自己的错误类型。
3. 错误判定由显式闭包完成，不再依赖不可靠的 TypeId 集合。
4. 配置是不可变快照，行为更容易测试和推理。
5. result-based retry 仍可作为高级 API 存在，但不会污染 90% 的错误重试场景。
