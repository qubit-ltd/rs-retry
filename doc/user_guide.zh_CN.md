# Qubit Retry 用户指南

[English](user_guide.md) · [README](../README.zh_CN.md) · [设计文档](design.zh_CN.md)

## 读者与贯穿场景

本指南适用于 **qubit-retry 0.23**，面向客户端和存储组件作者。贯穿场景是读取存储快照：
临时不可用时恢复读取，永久性错误及时停止，应用关闭时能够退出，并保留最终结果。
成功标准是返回一份快照及准确的实际准入次数，重试不能掩盖结果不确定的副作用。
[README 示例](../README.zh_CN.md#快速开始)给出了完整最小实现，直接保留 `RetryError<io::Error>`。

## 概念模型

`RetryPolicy` 是可复用配置，`Retry` 在此基础上持有有序规则和观察者。每次 `run` 独立创建预算、退避索引和上下文。
`max_attempts` 包含首次操作；默认总共三次尝试、立即退避、无耗时限制，未分类业务错误默认立即终止。
`AttemptFailure` 描述一次操作的失败，`RetryErrorReason` 描述整个流程为何停止。
`context.attempts()` 统计已准入次数；`current_attempt()` 还可能表示准入前回调或尚未结束的活动操作。
如果业务明确希望重试未分类错误，需设置 `RetryFallback::Retry`，让它使用策略中的退避行为。

## 安装与最小配置

使用 Rust 1.94 或更新版本。默认 feature 集为空：

<!-- retry-example: kind=cargo features=none -->
```toml
[dependencies]
qubit-retry = "0.23"
```

异步执行需要 `tokio`，配置序列化需要 `serde`：

<!-- retry-example: kind=cargo features=tokio,serde -->
```toml
[dependencies]
qubit-retry = { version = "0.23", features = ["tokio", "serde"] }
```

手动时钟示例另用 `qubit-clock` 0.13 的 `test-util`，JSON 示例使用 `serde_json` 1；
异步二进制使用 Tokio 1.52 或更新版本的 `rt`、`macros`、`time` 特性。文档校验器会提供这些示例依赖。

## 核心流程：分类、执行并保留结果

先用规则区分快照读取中的瞬时错误和永久性错误。规则按注册顺序执行，第一个非 `UseDefault` 决策生效。
`Abort` 保留失败后停止；`Retry`、`RetryWithHint` 和 `RetryWithJitteredHint` 都不能绕过准入限制。
全部规则委托默认行为时，未分类业务错误会立即终止；设置 `RetryFallback::Retry` 后，才会使用策略退避重试。
单次超时以 `TimedOut` 终止，捕获的 worker panic 以 `Aborted` 终止。

按操作实际行为选择执行模式：

| 操作 | 模式 | 约束 |
| --- | --- | --- |
| 当前线程上的有界阻塞读取 | `sync()` | 无法打断闭包，支持借用状态和 `FnMut` |
| 可以安全取消的异步客户端调用 | `asynchronous()` | 需要 Tokio feature/运行时；future 不必为 `Send` 或静态生命周期 |
| 收到令牌后可协作退出的阻塞调用 | `worker()` | 需要启用 `worker` feature；操作须为 `Fn + Send + Sync + 'static`，结果和错误须为 `Send + 'static`；每次创建 worker 与 reaper |

成功和失败都应完整保留三元组。只转换业务错误类型时使用 `map_error`：存在业务错误才调用一次 `FnOnce` 映射函数，
否则不调用；映射函数 panic 会向外传播。转换保留终态分类、上下文和完成诊断，不额外要求 `Clone`、`Send` 或 `'static`。
单纯克隆 `Retry` 不会克隆业务错误 `E`。

## 时间限制与应用关闭

`operation_time_budget` 累计已准入操作的耗时；`total_time_budget` 计量单调流程时间，包括控制回调和等待。
二者都只限制后续准入，已经准入的操作越过预算后成功仍是成功。
async/worker 的 `hard_attempt_timeout`、`hard_flow_timeout` 限制协作式等待，不能抢占任意同步代码。
阻塞回调或阻塞 future poll 会推迟检查；完成回调不在耗时计量和超时控制范围内。

同步读取失败后，取消可以阻止再次读取：

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

完成时钟有效时，sync 返回的 `Ok` 优先于取消。令牌克隆共享永久取消状态，不支持重置、父子树或内置截止时间。
控制回调正常返回后先刷新时钟，再检查取消；无效样本返回时钟基础设施错误。
控制回调 panic 则保留自身主因，只尽力更新时间。退避取消优先于已就绪的等待结束。

异步读取尚未完成时，取消会丢弃操作 future：

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

同一次 poll 中结果、取消和超时同时就绪时，async 依次优先选择结果、取消、计时器。
选中的错误仍进入失败处理，选中的成功仍须通过完成计量。单次与流程截止时间相等时归因为单次超时。
丢弃 future 并不会回滚已经发出的 HTTP 请求或事务；对有副作用的操作，应采用幂等键、事务边界或业务对账后再决定重试。

阻塞操作通过单次尝试令牌协作退出：

<!-- retry-example: kind=run features=worker -->
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

流程先检查取消，再检查计时器，最后才接受操作结果及 join 完成的确认。join 包括线程局部存储（TLS）析构。
即使注入手动 timer，`cancellation_grace` 也使用真实单调时间。worker/TLS 清理超出宽限期会返回
`Infrastructure::WorkerStillRunning` 并保留 `WorkerStopTrigger`，当前流程不再准入下一次尝试。
worker 和已分离的 reaper 可能继续存活；没有超时或取消时，等待退出可能没有时间上限。本库不强杀线程，也不提供 worker 池。

## 服务端提示、抖动与重连

指数退避可降低持续访问繁忙服务的频率，抖动用于分散并发客户端。Full jitter 在零到选中延迟之间采样；
bounded jitter 的相对偏差必须在 `[0, 1]` 内。合法随机源下，均匀采样精确保留端点且不越过区间。

`BackoffRequest::hint` 不对服务端值应用抖动，`jittered_hint` 则允许抖动。
`prefer_retry_after` 优先提示，`use_retry_after_as_minimum` 将提示与退避组合，`ignore_retry_after` 忽略提示。
`maximum_delay()` 只是基础策略上限；`limit_delay()` 最后执行，限制完成提示和抖动处理后的最终延迟。
下面的例子有意把服务端一秒最小等待截短到半秒：

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

必须遵守服务端最小等待时，不应配置更小的最终 cap。剩余预算不足以容纳最小等待，就停止重试。
`BackoffStep` 保留基础延迟、有效延迟与来源；新流程或连接达到业务定义的稳定条件后，才重置退避状态。
rs-http 的 SSE 独立组合 `RetryBudget` 和 `BackoffState`，再施加自身的至少 1ms 约束与服务端延迟上限，不产生 retry 完成诊断。

## 独立预算与确定性时钟

自定义重连循环可以独立计量准入操作，无须使用执行门面。完成当前 token 后才能申请下一次尝试，token 只属于原预算。
丢弃 token 会让该流程无法继续准入。`check_retry_after` 检查拟等待时间，实际等待后下次准入还会复查。
以下手动时钟示例不依赖网络或文件：

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

通过 `.timer(...)` 向执行器注入同一手动时钟创建的 timer。测试应先观察到操作或计时器注册边界，再推进时间，
不要用真实 sleep 同步正确性断言。切换时钟域或时钟回退会返回结构化错误，不能用伪造时间掩盖失效计量。
worker 清理宽限期始终使用真实时间，因此测试还需要明确的释放和 join 协议。

## JSON 配置

`serde` 只覆盖经过验证的 policy、limit 和 backoff 配置。Duration 使用 `seconds`、`nanoseconds`，纳秒部分小于十亿；
策略标签使用稳定的 snake_case。未知字段或非法组合会被拒绝。省略可选耗时限制表示不设限制；可选 `delay_limit`
在提示和抖动之后生效。Bounded jitter 必须提供数值 ratio，`null` 不等于缺省。
运行时回调、结果和错误都不是跨进程序列化协议。

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

## 观察者与完成诊断

控制回调覆盖准入前、失败提交后、规则选择和允许调度退避后的阶段。控制回调 panic 会以 `CallbackFailed` 停止流程，
保留类型、索引、阶段和载荷。`on_retry_scheduled` 只表示延迟在当时符合预算，后续回调和准入还会复查取消、预算及超时。
实际工作次数应看终态 `attempts()`。

`on_success` / `on_terminal_failure` 对每个正常返回的结果执行一次，首次准入前失败也会通知。
这些同步回调必须简短且不阻塞；它们看到的是冻结后的结果，panic 只附加诊断，后续完成观察者仍会执行。
下面刻意让审计出口 panic：panic hook 可能打印消息，但重试结果会保留诊断。

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
    assert_eq!(failure.and_then(|failure| failure.as_error()).map(String::as_str), Some("offline"));
    assert_eq!(context.attempts(), 1);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].phase(), RetryCallbackPhase::TerminalFailure);
}
```

sync/async 操作 panic、异步 run future 被丢弃或进程中止时，不保证完成通知。只有 worker 捕获操作 panic。
捕获的载荷区分静态字符串、拥有所有权的字符串及非字符串。非字符串载荷析构再次 panic 时仍保留 `NonString`，
仅遗忘二次载荷以防递归析构；保护范围不包括任意业务值析构、panic hook 或进程 abort。

## 适配层边界与版本迁移

0.23 中 `RetrySuccess::into_parts` 返回三元组，`RetryError::into_parts` 返回四元组，
删除 `into_parts_with_diagnostics` 且不保留别名。旧 `into_value` / `into_failure` 改为显式命名的
`into_value_discarding_diagnostics` / `into_failure_discarding_diagnostics`，同时丢弃上下文。
默认行为和各模式优先级见前文；控制回调正常返回后的时钟验证，现在优先于返回的 Abort 决策和取消，回调 panic 仍保留自身主因。

HTTP 在每种重试终态中以完整 `RetryError<HttpError>` 作为 source，并保留请求、状态、预览、提示和脱敏策略；
原始 HTTP 错误及后端 source 仍可沿链读取。CAS 保留超时状态快照及领域分类，同时在 `CasError` 中保留完成诊断。
EventBus 在诊断为空时保持原有转换，非空时创建带领域 source 和共享上下文的终止包装 `RetryCompletionDiagnostics`。
这些适配层目前不在成功流程中注册完成观察者，所以显式丢弃空诊断；未来增加注册入口时，必须同时提供成功诊断出口。

## 排障与使用边界

| 现象 | 检查与处理 |
| --- | --- |
| 一次操作也没有执行 | 查看 attempts、回调阶段、零预算、取消和 timer 注册失败 |
| 调用次数少于调度通知数 | 调度只是当时允许；检查最终预算/超时以及回调耗时 |
| 同步调用超出预算 | 预算是软限制；约束操作本身或选择适合的 async/worker 接口 |
| 取消后操作仍有副作用 | 取消停止等待，不撤销外部已提交效果；先对账或保证幂等 |
| `Infrastructure::Clock` | 检查 timer/clock 域及单调性，不复用 token 或伪造时间戳 |
| `WorkerStillRunning` | 检查触发源和操作/TLS 退出协议，不自动启动替代流程 |
| 成功值带有诊断 | 检查完成观察者；其失败不否定业务成功 |
| Retry-After 比预期短 | 检查最终 cap、提示策略、是否允许抖动及下游 SSE 约束 |

本库不内置熔断器、取消层级、其他异步运行时或线程池执行。远程操作应设置有界次数和预算，性能应结合真实操作成本评估。
修改仓库时先运行 `./align-ci.sh`，再运行 `./ci-check.sh`。项目 CI 会在临时 path 消费 crate 中执行两种语言所有标注的
Rust/Cargo 代码块；rustdoc 示例单独验证。

## 延伸阅读

[README](../README.zh_CN.md) · [设计文档](design.zh_CN.md) · [API](https://docs.rs/qubit-retry) · [English](user_guide.md)
