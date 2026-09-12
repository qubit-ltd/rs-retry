# Qubit Retry 用户手册

[English](user_guide.md) · [README](../README.zh_CN.md) · [API 文档](https://docs.rs/qubit-retry/0.25.0/qubit_retry/)

本手册适用于 **qubit-retry 0.25.0**，需要 **Rust 1.94 或更新版本**，
面向需要为客户端、存储访问或重连循环添加重试能力的 Rust 开发者。
业务操作由你提供，哪些错误可以安全重试也由业务决定。

先从一个模拟存储读取开始：前两次调用超时，第三次成功。
重试控制交给库，模拟响应只用于让示例不依赖外部服务。
手册中的每个 Rust 代码块都是完整程序，
可以单独替换测试应用的 `src/main.rs` 运行。

## 目录

- [快速上手：读取存储快照](#快速上手读取存储快照)
- [理解一次重试流程](#理解一次重试流程)
- [选择执行模式](#选择执行模式)
- [决定哪些失败可以重试](#决定哪些失败可以重试)
- [配置退避与服务端等待提示](#配置退避与服务端等待提示)
- [设置预算与超时](#设置预算与超时)
- [执行异步操作](#执行异步操作)
- [取消操作与应用关闭](#取消操作与应用关闭)
- [处理结果与观察执行过程](#处理结果与观察执行过程)
- [从 JSON 加载配置](#从-json-加载配置)
- [独立使用预算与测试时钟](#独立使用预算与测试时钟)
- [排障](#排障)

## 快速上手：读取存储快照

在应用的 `Cargo.toml` 中添加依赖：

<!-- retry-example: kind=cargo features=none -->
```toml
[dependencies]
qubit-retry = "0.25"
```

存储服务暂时不可用时，我们希望自动重试，恢复后返回快照。
下面把模拟读取单独写成 `read_snapshot` 函数：它前两次调用失败，第三次成功。
函数内的计数只用于制造这组测试响应，不是业务需要实现的重试逻辑。
最大尝试次数、等待时间与指数退避都由 `RetryConfig` 配置，规则只负责判断错误是否值得重试。

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

// 模拟存储服务：第一次和第二次调用返回超时，第三次调用成功。
// simulated_calls 只用于制造故障响应，不负责重试次数限制或重试调度。
// 实际项目中，用真实的存储读取函数替换此函数，无需保留这个模拟计数器。
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

把依赖加入应用的 `Cargo.toml`，将完整示例保存到 `src/main.rs`，运行 `cargo run`：

```text
snapshot-v2, attempts=3
```

库先调用一次 `read_snapshot`，失败后等待 100 ms 再调用；第二次失败后等待 200 ms，
第三次成功便结束。**模拟函数决定返回什么响应，policy 决定是否还能尝试以及等待多久。**
`max_attempts(5)` 包含首次调用，最多允许四次重试；`context.attempts()` 是库记录的实际执行次数。

接入真实客户端时，替换 `read_snapshot` 并删除模拟计数器即可，不需要自己写重试循环或 sleep。
如果把 `max_attempts` 改为 2，库会在第二次失败后返回 `Exhausted { limit: Attempts }`，
不会执行模拟函数的第三次调用。未分类错误默认不重试。

| 验证场景 | 修改方式 | 结果 |
| --- | --- | --- |
| 临时错误后恢复 | 直接运行示例 | 第三次尝试成功 |
| 次数耗尽 | 将 `max_attempts(5)` 改为 `max_attempts(2)` | 两次失败后 `Exhausted`，不再调用操作 |
| 永久性错误 | 模拟函数改为返回 `PermissionDenied` | 第一次失败后 `Aborted` |

这些修改仅用于验证不同分支。真实调用方可以保留完整的 `RetryError<io::Error>`，
通过 `reason()` 查看终止原因，通过 `last_error()` 检查原始 I/O 错误。
每次 `run` 都从新的状态开始，复用 `RetryConfig` 不需要重置库内部计数。

## 理解一次重试流程

| 概念 | 含义 |
| --- | --- |
| 尝试（attempt） | 一次获准执行的操作，首次调用也算一次 |
| 重试（retry） | 失败后再次尝试 |
| 退避（backoff） | 重试前的等待时间 |
| 准入（admission） | 检查剩余次数与预算，决定是否允许操作开始 |
| `RetryPolicy` | 经过校验的次数限制、耗时预算与退避配置 |
| `RetryConfig<E>` | 针对错误类型 `E` 的策略、规则、观察者与 fallback |
| `Retry` / `AsyncRetry` / `TokioRetry` / `WorkerRetry` | 基于共享 `RetryConfig` 的执行器 |
| `RetryContext` | 次数、耗时，以及当前阶段的尝试或延迟信息快照 |

正常执行顺序如下：

```text
检查限制 → 尝试前回调 → 再次检查 → 允许执行并调用操作
                                      ├─ 成功 → 完成通知
                                      └─ 失败 → 失败通知 → 判断规则
                                                            ├─ 停止 → 完成通知
                                                            └─ 重试 → 检查等待预算
                                                                       → 调度通知 → 等待 → 再次检查
```

取消、硬超时或基础设施错误也可能在控制边界终止流程。发出调度通知后，
仍可能因后续检查不通过而取消下一次尝试。`context.attempts()` 统计已经准入的次数；
`current_attempt()` 在回调中也可能指即将开始、尚未计数的尝试。

默认配置为：**总共最多三次尝试、立即退避、不设耗时预算，未分类业务错误直接终止**。
仅创建默认配置，并不会让失败自动重试。每次 `run` 都创建独立状态。
克隆 `RetryConfig` 会共享规则与观察者，不要求业务错误实现 `Clone`。

## 选择执行模式

| 入口 | 所需 feature | 操作要求 | 执行位置 |
| --- | --- | --- | --- |
| `Retry::new(&config)` | 无 | `FnMut() -> Result<T, E>`，可借用局部状态 | 调用线程 |
| `AsyncRetry::new(&config)` | `async` | `FnMut() -> Fut`，future 无需满足 `Send` 或 `'static` | 由任意 executor poll；默认使用 `StdTimer` |
| `TokioRetry::new(&config)` | `tokio` | `FnMut() -> Fut`，future 无需满足 `Send` 或 `'static` | Tokio 运行时 |
| `WorkerRetry::new(&config)` | `worker` | `Fn(AttemptCancellationToken) -> Result<T, E> + Send + Sync + 'static`；`T`、`E` 须为 `Send + 'static` | 每次尝试创建独立工作线程 |

各执行接口都要求 `E: 'static`，同步与异步模式也不例外；但这不意味着它们的操作闭包必须拥有所有捕获状态。
规则与观察者会被共享，须满足 `Send + Sync + 'static`。

不绑定具体运行时的异步执行需要开启 `async`：

<!-- retry-example: kind=cargo features=async -->
```toml
[dependencies]
qubit-retry = { version = "0.25", features = ["async"] }
```

`AsyncRetry` 返回标准库 `Future`，默认使用 `StdTimer`；调用方负责提供
executor。若希望使用 Tokio 原生计时器，请开启 `tokio`：

<!-- retry-example: kind=cargo features=tokio -->
```toml
[dependencies]
qubit-retry = { version = "0.25", features = ["tokio"] }
```

运行手册中的异步示例，还需为应用添加直接 Tokio 依赖：

```bash
cargo add tokio@1.52 --features rt,macros,time
```

工作线程示例需要开启 `worker`：

<!-- retry-example: kind=cargo features=worker -->
```toml
[dependencies]
qubit-retry = { version = "0.25", features = ["worker"] }
```

多个 feature 可以放入同一依赖的 `features` 数组。`WorkerRetry::new(&config).run()` 在协调工作线程期间会阻塞调用方，
它不是异步线程池接口。

## 决定哪些失败可以重试

规则按注册顺序执行，第一个非 `UseDefault` 决策生效。`UseDefault` 会继续询问下一条规则；
所有规则均未作决定时，才采用默认处理方式。

| 决策 | 行为 |
| --- | --- |
| `RetryDecision::Abort` | 停止流程，保留本次失败 |
| `RetryDecision::Retry` | 请求按策略退避后再次尝试 |
| `RetryDecision::RetryWithHint(delay)` | 提供等待提示，不对提示施加抖动，但仍受最终延迟上限约束 |
| `RetryDecision::RetryWithJitteredHint(delay)` | 提供允许施加抖动的等待提示 |
| `RetryDecision::UseDefault` | 交给后续规则处理 |

如果某个操作返回的所有业务错误都可以重试，可以显式设置 `RetryFallback::Retry`：

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

Fallback 只决定业务错误的默认行为。即使设置了 `RetryFallback::Retry`，
执行器捕获的单次超时仍默认以 `TimedOut` 结束，worker 捕获的 panic 仍默认以 `Aborted` 结束。
规则可以显式要求重试这两类失败，但不能绕过剩余限制。
流程超时、取消、回调失败和基础设施错误均为终止事件，无法通过重试规则恢复。

客户端返回的 `io::ErrorKind::TimedOut` 属于业务错误，
与执行器超时产生的 `AttemptFailure::TimedOut` 是两回事。
快速上手的规则判断的是客户端业务错误 `TimedOut`，没有处理执行器产生的超时。
为写操作增加规则前，应先明确重复执行是否安全：停止等待无法撤销已经提交的写入。

## 配置退避与服务端等待提示

通过 `RetryConfig::builder().backoff(...)` 设置退避策略。
创建策略本身不会等待；操作失败且允许重试时，执行器才会按策略等待。

| 构造方法 | 基础延迟 | 适用场景 |
| --- | --- | --- |
| `BackoffPolicy::immediate()` | 零 | 快速本地重试或演示 |
| `BackoffPolicy::fixed(delay)` | 每次相同 | 已知轮询间隔 |
| `BackoffPolicy::uniform(min, max)?` | 在闭区间内均匀随机取值 | 将重试分散到指定时间区间 |
| `BackoffPolicy::exponential(initial, multiplier, max)?` | 从 `initial` 递增，不超过 `max` | 服务持续暂时不可用 |

例如，初始延迟 50 ms、倍率 2、上限 2 s，会依次得到
50、100、200、400、800、1600、2000、2000 ms 的基础延迟。
第一个延迟发生在第二次尝试之前。均匀随机策略要求 `min <= max`；
指数策略要求 `initial <= max`，且倍率为不小于 1 的有限数值。

`with_full_jitter()` 在零到所选延迟之间随机取值。
`with_bounded_jitter(ratio)?` 按指定比例在延迟两侧波动，比例须为 `[0, 1]` 内的有限数值；
例如 0.2 对应约 80%–120% 的延迟范围。各构造方法默认不添加抖动，
也可以调用 `without_jitter()` 取消已配置的抖动。

应用可以解析服务端的重试指令，在规则中返回 `RetryWithHint(duration)`。
Qubit Retry 接收的是 `Duration`，不会解析 HTTP 响应头。提示与策略延迟的组合方式如下：

| 方法 | 收到提示时的行为 |
| --- | --- |
| `use_retry_after_as_minimum()`，默认方式 | 按允许的范围施加抖动后，取提示与策略延迟中的较大值 |
| `prefer_retry_after()` | 优先使用提示；仅在明确允许时对提示施加抖动 |
| `ignore_retry_after()` | 仅使用策略延迟 |

### 按服务端提示重试，并记录执行日志

下面的服务返回 10 ms 等待提示。规则将提示交给执行器，观察者记录选中的延迟与最终成功。
默认组合方式会在该提示与 5 ms 固定延迟中取较大值。

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

程序依次输出 `retry after 10ms, attempts=1` 和 `read complete, attempts=2`。
接入实际客户端时，将模拟的 `Busy` 响应替换为客户端解析得到的等待时间。

### 只计算延迟，不执行操作

下面单独计算带抖动的指数退避，并保留服务端要求的一秒最小等待：

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

第一次基础延迟为 50 ms，全抖动后的值不会高于未施加抖动的一秒提示，因此有效延迟恰好为一秒。
每次 `BackoffState::next` 都推进重试序号。新流程使用新状态；
重连循环中，应在连接达到业务定义的稳定条件后再重置状态。

`maximum_delay()` 返回基础策略的最大延迟。`limit_delay(cap)` 是另一层最终上限，
在提示与抖动处理完毕后生效，**可能缩短服务端要求的最小等待**：
若上例配置 500 ms 的最终上限，一秒提示就会被截短到 500 ms。
必须遵守最小等待时，不要对提示施加抖动，也不要设置更小的最终上限。
剩余预算不足以容纳这段等待时，应停止重试。

## 设置预算与超时

| 配置 | 设置位置 | 限制内容 |
| --- | --- | --- |
| `max_attempts(n)` | 策略构造器 | 包含首次调用的总尝试次数，不能为零 |
| `operation_time_budget(d)` | 策略构造器 | 累计已准入操作耗时，不含退避与控制回调 |
| `total_time_budget(d)` | 策略构造器 | 整个流程耗时，包含控制回调与退避 |
| `hard_attempt_timeout(d)` | async/worker 执行器 | 等待单次尝试的时间 |
| `hard_flow_timeout(d)` | async/worker 执行器 | 等待整个流程的时间，包含重试与退避 |

两种耗时预算都是**控制能否开始尝试的软限制**。操作开始时还有剩余预算，
完成时即使已经超出预算，只要完成计量有效，成功结果仍然保留。
零耗时预算是合法配置，但会阻止首次尝试；不设置预算则不限制相应耗时。

例如，远程读取可以在策略上设置 10 s 总预算，在异步执行器上设置 2 s 单次超时和 5 s 流程超时。
它们分别回答三个问题：是否还允许开始下一次尝试、本次操作最多等多久、整个重试流程最多等多久。

硬超时仍依赖协作执行。future 的 poll 或同步回调如果阻塞，超时检查就会推迟；
worker 还需要额外的取消清理宽限期。完成观察者在计量冻结后执行，不受超时控制。
因此，这些配置不保证整个 `run` 调用一定在某个精确的墙上时钟截止时间前返回。

## 执行异步操作

开启 `tokio`，并按前文添加直接 Tokio 依赖。传入的闭包应当**为每次尝试创建新的 future**，
不要重复使用同一个 future。下面先演示客户端错误后的恢复，再演示读取一直未完成时的超时：

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

第二个流程只尝试一次便超时：重试全部业务错误的 fallback 不会重试执行器超时。
实际客户端还可能返回永久性错误时，应使用快速上手中的选择性规则。
示例的响应序列只用于模拟客户端先超时、后成功；它不控制重试次数。
真实调用只需在闭包中为每次请求创建 future，计数与终止判断仍由库完成。

## 取消操作与应用关闭

创建 `RetryCancellationToken`，将一个克隆交给负责关闭的组件，另一个传入执行器。
调用 `cancel()` 即可请求取消，所有克隆共享永久取消状态。
独立任务应使用新的令牌；令牌不支持重置、父子层级或内置截止时间。

### 同步操作

同步模式会在尝试前后及退避期间检查取消，无法打断操作闭包。
下面在一次失败的读取中主动发出关闭信号，便于确定性地演示取消行为：

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

操作执行一次后，流程在 `Backoff` 阶段响应取消。
如果闭包返回的是 `Ok`，且完成时钟计量有效，则优先保留成功结果。
底层阻塞 I/O 的最长耗时需要单独约束。

### 尚未完成的异步操作

下面在创建待完成 future 时触发取消。实际应用中，可以由外部关闭任务取消同一个令牌：

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

取消会丢弃尚未完成的操作 future。如果一次 poll 中操作结果、取消和超时同时就绪，
异步模式依次优先选择结果、取消、计时器；选中的错误仍需进入失败处理流程。
单次与流程截止时间相同时，归因为单次超时。丢弃 future 不会回滚已经发送给服务端的请求。

### 工作线程中的阻塞操作

开启 `worker` feature。操作闭包收到 `AttemptCancellationToken`，
它与传入 `.cancellation_token(...)` 的流程令牌不同。
将工作拆成耗时可控的小段，在各段之间检查尝试令牌，收到取消后及时释放资源。
下面的循环只演示取消与退出的配合过程，没有模拟存储 I/O。

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

每次尝试都会创建工作线程和回收线程（reaper），不使用线程池。
Worker 模式先检查取消，再检查超时，最后才接受操作结果与线程已退出的确认。
线程退出包含线程局部存储（TLS）的析构过程。

`cancellation_grace` 默认为 100 ms，上例显式设为一秒。即使注入了自定义计时器，
宽限期仍使用真实单调时间。清理超过宽限期时，返回
`Infrastructure { failure: WorkerStillRunning { trigger } }`。
当前流程不会开始下一次尝试，但工作线程与回收线程可能仍然存活。
不要自动启动可能与其重叠的替代任务。没有超时或取消时，等待线程退出可能没有时间上限；本库不能强杀线程。

## 处理结果与观察执行过程

### 保留完整结果

| 结果 | 借用检查 | 完整取出所有信息 |
| --- | --- | --- |
| `RetrySuccess<T>` | `value()`、`context()`、`completion_callback_failures()` | `into_parts()` → `(value, context, diagnostics)` |
| `RetryError<E>` | `reason()`、`last_failure()`、`last_error()`、`context()`、`completion_callback_failures()` | `into_parts()` → `(reason, last_failure, context, diagnostics)` |

`last_failure` 是可选值，因为流程可能在操作产生失败前就已停止。
只有保留了业务错误，`last_error()` 才有值；执行器超时或 panic 不会提供业务错误。
成功拆解结果中的诊断类型为 `Vec<RetryCallbackFailure>`，失败拆解结果中则为 `Box<[RetryCallbackFailure]>`。

| `RetryErrorReason` | 含义 |
| --- | --- |
| `Aborted` | 规则或默认行为决定停止 |
| `Exhausted { limit }` | 次数、操作耗时或总耗时预算阻止继续执行 |
| `TimedOut { scope }` | 单次或流程硬超时终止执行 |
| `Cancelled { phase }` | 在尝试前、尝试中或退避期间响应了取消 |
| `CallbackFailed { callback }` | 规则或控制观察者发生 panic |
| `Infrastructure { failure }` | 时钟、计时器或工作线程运行错误导致无法继续 |

该终止原因枚举标记为非穷尽，使用 `match` 时需要保留通配分支。
`map_error` 只转换已保留的业务错误，其他原因、上下文和诊断保持不变；
其 `FnOnce` 转换函数调用零次或一次。
编写领域适配器时，可以用 `into_metadata_and_error()` 将可选业务错误与 `RetryErrorMetadata` 分开。
不需要拆分时，优先把完整 `RetryError<E>` 保留为错误来源。
如果明确只需要成功值，可调用 `into_value_discarding_diagnostics()` 丢弃上下文和诊断。

### 观察生命周期

在重试构造器上调用 `.observer(...)` 注册 `RetryObserver<E>`。

| 回调 | 调用时机 | 发生 panic 时 |
| --- | --- | --- |
| `on_before_attempt` | 准入前，即将开始的尝试尚未计数 | 以 `CallbackFailed` 停止 |
| `on_attempt_failed` | 记录本次失败后、判断规则前 | 以 `CallbackFailed` 停止 |
| `on_retry_scheduled` | 所选延迟在当时符合继续执行的预算 | 以 `CallbackFailed` 停止 |
| `on_success` | 成功结果已经冻结 | 附加诊断，保留成功 |
| `on_terminal_failure` | 终止错误已经冻结，包括零尝试错误 | 附加诊断，保留原错误 |

回调应保持简短、不阻塞。统计实际准入次数应读取终态 `context.attempts()`，
不能用调度通知数代替。一个完成回调失败后，后续完成观察者仍会继续执行。

下面故意让审计回调 panic，展示如何在把业务错误转换为 `String` 的同时保留完成诊断：

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

Panic hook 可能打印 `audit sink unavailable`，但示例本身仍能成功运行。
只有 worker 模式会捕获**操作本身**的 panic；sync/async 操作 panic 会向外传播。
发生栈展开、异步 `run` future 被丢弃或进程中止时，不保证完成通知。
Panic 捕获依赖栈展开，无法从进程 abort 中恢复。

## 从 JSON 加载配置

开启 `serde`：

<!-- retry-example: kind=cargo features=serde -->
```toml
[dependencies]
qubit-retry = { version = "0.25", features = ["serde"] }
```

运行下面的 JSON 示例还需执行 `cargo add serde_json@1`。
反序列化策略时会执行与 Rust 构造器相同的合法性校验：

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

时间长度使用 `seconds` 和 `nanoseconds` 表示，纳秒部分必须小于十亿。
可选耗时预算省略或设为 `null` 表示不限制。
未知字段、为零的 `max_attempts`、非法时间值以及非法退避参数都会被拒绝。
有界抖动必须提供数值比例，不能使用 `null`。

该 feature 序列化的是经过校验的策略、准入限制与退避配置，
不序列化规则、观察者、取消令牌、结果或错误。硬超时属于执行器配置，不应加入这份策略 JSON。

## 独立使用预算与测试时钟

如果应用已经有重连循环，可以用 `RetryBudget` 管理准入与耗时，用 `BackoffState` 计算延迟。
这两个组件不会替你执行操作、分类错误或发送重试观察者通知。

下面的示例需要直接依赖 `qubit-clock` 并开启测试工具：

```bash
cargo add qubit-clock@0.13 --features test-util
```

通过推进手动时钟，可以分别验证操作耗时与总耗时，无需真实等待：

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

两秒操作计入两个耗时值，一秒等待只计入总耗时。
开始下一次尝试前，必须完成当前尝试令牌；令牌只属于创建它的预算。
丢弃尚未完成的令牌，会阻止该预算继续准入。
`check_retry_after` 检查拟等待时间，真正等待后，准入时还会再次检查限制。

执行器也支持 `.timer(...)` 和 `.random_source(...)` 注入。
手动计时器应在整个流程中保持同一时钟域。测试应观察到操作开始或计时器注册等明确事件后再推进时间，
不要通过真实 sleep 猜测执行器是否就绪。时钟回退或时钟域不匹配会产生结构化错误。
Worker 清理宽限期仍然使用真实时间。

### 确定性地验证抖动

测试抖动时，可以注入固定随机源。下面始终返回允许区间的下界，
因此 100 ms 延迟的全抖动结果固定为零：

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

`RetryRandomSource` 须支持并发调用，返回值必须是指定闭区间内的有限数值。
执行完整流程时，用执行器的 `.random_source(Arc::new(LowerBound))` 注入同样的随机源。
该实现用于测试边界；如果在业务中始终选择下界，就失去了随机分散重试的效果。

### 其他配置入口

| 需求 | 方法与约束 |
| --- | --- |
| 根据可选配置启用或清除预算 | `operation_time_budget_opt(Some(d))` / `total_time_budget_opt(Some(d))`；传 `None` 清除对应限制 |
| 显式移除耗时预算 | `without_operation_time_budget()` / `without_total_time_budget()` |
| 复用已有 `Arc` 回调 | `shared_rule(...)` / `shared_observer(...)`，传入对应 trait 对象，保持注册顺序 |
| 区分工作线程或设置栈大小 | worker 执行器的 `thread_name(name)` / `worker_stack_size(bytes)` |
| 在取消信号上等待 | 同步检查用 `is_cancelled()`；异步等待用流程令牌的 `cancelled().await` |

克隆的 `Retry` 共享回调对象，因此回调内部的计数器等可变状态也会被共享；需要时使用同步机制。
注入 worker 计时器时，必须保证计时器能在调用线程阻塞期间独立推进。
独立预算接口的 `RetryBudgetError` 还会区分 `Clock`、`Exhausted`、`AttemptInProgress` 和 `InvalidAttempt`，
分别用于时钟异常、限制耗尽、前一次尝试未完成以及令牌不属于当前尝试。
控制回调正常返回后，先刷新时钟，再处理取消或规则决策；时钟样本无效会成为基础设施错误。
回调自身 panic 时仍以 `CallbackFailed` 为主因，只尽力刷新耗时。

## 排障

| 现象 | 检查方向 |
| --- | --- |
| 配置了 `max_attempts(3)`，却只调用一次 | 未分类错误默认终止。先检查规则与 fallback，再考虑增加次数。 |
| `attempts() == 0` | 检查预先取消、零预算或零超时、尝试前回调以及基础设施错误。 |
| 返回 `Exhausted` 而非 `TimedOut` | 软预算限制准入，应检查 `limit`；硬超时错误带有 `scope`。 |
| 同步操作运行时间超过预算 | 无法打断同步闭包。限制底层 I/O，或选择合适的 async/worker 操作。 |
| 找不到 `AsyncRetry`、`TokioRetry` 或 `WorkerRetry` | 检查对应的 `async`、`tokio` 或 `worker` feature 是否开启；`AsyncRetry` 只需要调用方提供 executor。 |
| 操作次数少于重试调度通知数 | 调度不保证准入，后续取消、回调或限制可能阻止执行。 |
| `WorkerStillRunning` | 检查触发原因及操作、TLS 的清理流程，再决定是否启动替代任务。 |
| 成功结果中有诊断 | 检查完成观察者，其 panic 不会否定业务成功。 |
| 服务端等待提示被缩短 | 检查是否允许提示抖动、提示选择方式以及最终延迟上限。 |
| `Infrastructure::Clock` 或计时器错误 | 检查注入的时钟、计时器及诊断消息，不要伪造耗时值。 |

远程操作应使用有界重试，并考虑客户端内部是否已经重试，避免叠加放大请求次数。
对结果不确定的副作用，应提前定义核对与恢复方式。
本库不提供熔断器、取消层级或线程池。

## 延伸阅读与文档校验

- [中文 README](../README.zh_CN.md) · [English README](../README.md)
- [0.25.0 API 文档](https://docs.rs/qubit-retry/0.25.0/qubit_retry/)
- [设计文档](design.zh_CN.md) · [English user guide](user_guide.md)

在仓库根目录运行 `python3 -B scripts/check_doc_examples.py`，可以编译并执行两种语言的 README
和手册中所有带标注的 Rust/Cargo 代码块。检查器使用当前源码的 path 依赖、临时消费 crate
和离线 Cargo 解析，因此运行前需要缓存相应依赖。
