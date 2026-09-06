# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

Qubit Retry 是面向 Rust 服务、客户端和存储代码的类型安全重试引擎。它把
尝试次数预算、退避、回调、超时与取消集中在同一条控制流中，同时保留原始业务错误和流程停止的准确原因。

## 安装

```toml
[dependencies]
qubit-retry = "0.21"
```

Tokio 执行与稳定的配置序列化均为可选能力：

```toml
qubit-retry = { version = "0.21", features = ["tokio", "serde"] }
```

## 快速开始

存储客户端可以只重试瞬时 I/O 错误，为整个流程设置时限，并在最终失败时读取结构化终态，而不是把原因压平成字符串：

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

领域错误如果带有服务端 `Retry-After`，规则可以返回
`RetryDecision::RetryWithHint(delay)`。退避策略会决定优先采用、将其作为最小延迟，还是忽略该提示。

### 同步取消

同步执行会在尝试准入前、操作失败后和退避等待中检查取消状态，无法打断已经运行的闭包。
因此每次操作都应能够在有限时间内结束；若操作返回 `Ok`，即使令牌已取消，仍然保留成功结果。
下面的操作在取消后返回失败，流程不会再启动下一次尝试：

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

### 异步取消

开启 `tokio` 特性后，运行时无关的取消令牌可以中断当前尝试或退避等待，
`RetryFailure::Cancelled` 会指出取消发生在哪个阶段：

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

应用可以把克隆的令牌交给关闭流程，并在需要停止时调用 `cancel()`。取消状态一旦设置，所有令牌克隆都能永久看到它。

### 阻塞工作线程取消

工作线程模式用独立线程隔离阻塞代码。流程取消令牌负责停止重试流程，单次尝试取消令牌
则通知当前操作协作退出：

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
            // 实际操作会在这里处理一个有界工作单元。
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

Rust 无法强制终止不配合的线程。工作线程如果未在宽限期内退出，流程会失败即终止，
通过 `WorkerStillRunning` 保留准确的超时或取消触发源。
每次尝试会创建一个 worker 和一个 reaper 线程，由 reaper 执行 worker 的 join。
调用线程等待的是线程彻底退出的确认，包含线程局部存储（TLS）析构完成，整个等待遵循超时和取消协议。
仅收到操作结果并不代表线程已经退出。如果操作或 TLS 析构不配合，宽限期结束后可能仍遗留这两个线程；
当前流程不会再启动下一次尝试。未设置超时且没有取消时，等待线程退出可能没有时间上限。

## 为什么需要这个项目

手写重试循环常把尝试次数、等待、关闭信号与错误转换散落在多个调用点，最终很容易丢掉最后一次业务错误，或把超时、取消和回调 panic 混成同一种失败。Qubit Retry
把这些决策收敛到策略驱动的流程中，并让成功与终态错误都携带一致的 `RetryContext`。

## 核心能力与边界

- `RetryPolicy` 把尝试次数、操作时限与总流程时限预算，同立即、固定、均匀或指数退避组合起来。
- `Retry::sync()` 在当前线程执行闭包，在安全边界支持协作取消，但不提供打断当前闭包的硬超时。
- `Retry::asynchronous()` 在 `tokio` 特性下支持单次尝试/流程超时与取消；Tokio 是唯一的异步运行时集成。
- `Retry::worker()` 捕获 panic、保留停止触发源，并且在旧工作线程仍可能运行时绝不启动新工作线程。
- `RetryFailure` 区分主动中止、预算耗尽、超时、取消、回调失败和基础设施失败；
  `AttemptFailure` 保留业务错误、超时范围或稳定的 panic 载荷。
- `Retry`、`RetryRules` 和 `RetryObservers` 可以克隆，不要求操作错误类型 `E` 实现
  `Clone`；回调通过内部引用计数共享。
- 规则和控制阶段观察者 panic 会终止流程；完成观察者 panic 则保留主结果并附加诊断。
  两者均保留回调类型、注册索引、阶段和 panic 载荷分类。
- `BackoffState` 可用于普通重试与 SSE 重连，支持服务端提示、抖动和防溢出的指数增长。
- 可选 `serde` 特性只序列化配置；运行结果、错误和回调状态不会作为稳定的跨进程序列化协议。

预算只决定能否启动下一次尝试。已经开始的操作即使越过预算后才成功，仍会返回成功。策略不会强杀同步操作或不配合的工作线程。取消令牌
不可重置，也不提供父子树或内置截止时间。

### 默认行为与失败分类

`RetryPolicy::builder().build()` 默认允许**总共三次尝试**，即首次执行加最多两次立即重试，
不设置耗时预算；不是在首次执行之外再重试三次。没有自定义规则时，任意业务 `Err(E)`
都可以重试。如果业务中存在永久性错误，应显式分类。

| 事件 | 默认行为 | 规则能否覆盖 |
| --- | --- | --- |
| 业务 `Err(E)` | 在预算内重试；耗尽时保留最后一次错误 | 可返回 `Abort`、`Retry` 或 `RetryWithHint` |
| 捕获的单次尝试超时 | 以 `TimedOut` 终止 | 流程仍可继续时，可请求重试 |
| 捕获的 worker 操作 panic | 以 `Aborted` 终止 | 流程仍可继续时，可请求重试 |
| sync/async 操作 panic | 向调用者或轮询任务展开栈 | 不进入规则 |
| 次数或耗时预算耗尽 | 以 `Exhausted` 终止 | 不能绕过准入限制 |
| 流程超时 | 以 `TimedOut` 终止 | 不能延长流程截止时间 |
| 取消 | 在执行门面的取消边界以 `Cancelled` 终止 | 不能重置令牌 |
| 规则或控制阶段观察者 panic | 以 `CallbackFailed` 终止 | 后续控制回调不能恢复流程 |
| 时钟、timer、线程创建、通道或仍在运行的 worker 故障 | 以 `Infrastructure` 终止 | 不重试 |
| 完成观察者 panic | 保留成功或终态错误，附加诊断 | 后续完成观察者仍会执行 |

规则按注册顺序执行，第一个非 `UseDefault` 决策生效；所有规则都委托默认行为时，采用上表分类。
只有 worker 会把操作 panic 捕获为尝试失败。sync 和 async 操作 panic 均向外传播，
包括创建或轮询异步操作 future 时的 panic；这些路径不会发送完成通知。

### 预算与超时

| 配置 | 计量范围 | 作用 |
| --- | --- | --- |
| `max_operation_elapsed` | 已准入操作的累计执行时间 | 软性续试预算，不能中断已准入操作 |
| `max_total_elapsed` | 单调时钟计量的流程时间，含退避和控制回调 | 软性续试预算，已完成的成功仍然有效 |
| `attempt_timeout`（async/worker） | 单次已准入尝试 | 停止等待并丢弃异步尝试，或请求 worker 退出 |
| `flow_timeout`（async/worker） | 整个执行流程 | 限制活动尝试和退避等待；worker 清理还可能占用 `cancellation_grace` |

超时不能抢占同步回调，也不能打断阻塞了轮询线程的异步 future。即使注入 timer，worker
清理宽限期仍使用真实时间。完成回调在终态上下文冻结后执行，其耗时不计入上下文，也不受重试超时约束。

### 完成诊断与错误映射

实现 `RetryObserver::on_success` 和 `on_terminal_failure`，可为每个 `run` 返回结果收到一次
完成通知，包括首次准入前的失败。观察者按注册顺序同步执行，应保持简短且不阻塞。
完成观察者 panic 不会改变已冻结的结果、触发重试或再次发送终止通知。消费结果前，可通过
`RetrySuccess` 或 `RetryError` 的 `completion_callback_failures()` 读取诊断：

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

`AttemptFailure::map_error`、`RetryFailure::map_error` 和 `RetryError::map_error` 只消费保留的
业务错误。有业务错误时 `FnOnce` 映射函数调用一次，否则不调用；不要求额外的 `Clone`、`Send`
或 `'static` 约束。映射保留失败分类和终态数据，`RetryError::map_error` 还保留上下文及完成诊断；
映射函数自身 panic 会向调用者传播。`into_parts()` 会丢弃完成诊断，要保留诊断应使用
`into_parts_with_diagnostics()`。`RetrySuccess::into_value()` 和 `RetryError::into_failure()`
也会丢弃这些附加诊断。操作展开栈、异步 `run` future 被丢弃或进程中止时，不保证发送完成通知。

## 延伸阅读

- [Rust API 文档](https://docs.rs/qubit-retry)
- [English README](README.md)
- [代码仓库](https://github.com/qubit-ltd/rs-retry)

## 从 0.20 升级到 0.21

把直接依赖的 `qubit-retry` 升级到 `0.21`，并同步更新 HTTP、CAS、EventBus 适配层及相关锁文件条目。
完成回调默认不执行任何操作，因此既有 observer 实现仍可编译。穷举匹配 `RetryCallbackPhase`
时，需要处理新增的 `Success` 和 `TerminalFailure`；匹配适配层的非穷尽错误类型时应保留 fallback。
Tokio 和 serde 仍须显式启用，默认 feature 集为空。

检查消费结果的位置：需要完成诊断时，使用 `into_parts_with_diagnostics()` 替代 `into_parts()`，
或先读取 `completion_callback_failures()`。只转换业务错误时优先使用 `map_error`；需要改变终态语义的
适配器仍须保留自身的领域转换。

### 保留的 0.20 契约

`RetryBudget` 与所有执行门面现在共享同一套预算计量。`begin_attempt`、
`finish_attempt`、`snapshot` 和 `check_retry_after` 返回 `RetryBudgetError`，
预算耗尽通过 `RetryBudgetError::Exhausted(kind)` 表示。`check_retry_after`
需要可变借用。必须先完成当前 token，再申请下一次尝试；token 不能交给其他预算。
异常时钟返回错误，软性耗时预算也不再要求能够表示绝对截止时间。

只有选中的延迟符合当前预算，才会触发 `on_retry_scheduled`。回调执行后和下一次
准入时仍会检查预算与取消状态，因此该事件不保证下一次操作一定开始。
`on_before_attempt` 仍在准入之前发出，实际准入次数请读取终态的 `context.attempts()`。

`BackoffPolicy::maximum_delay()` 只描述基础策略，尚未计入抖动和服务端提示。
需要限制最终等待时间时，调用 `.limit_delay(duration)`。可选的 serde 字段
`delay_limit` 沿用 `{seconds, nanoseconds}` 格式，没有该字段的旧配置仍可读取。

Worker 的单次和流程超时现在由注入的 timer 驱动，包括手动时间；线程清理宽限期
`cancellation_grace` 始终使用真实时间。活动 timer 失败时会请求取消；如果线程
不配合退出，则返回带有 `WorkerStopTrigger::TimerFailure` 的 `WorkerStillRunning`，
当前流程不会再启动其他尝试。

## 下一版本迁移说明

- 克隆 `Retry` 不要求业务错误类型实现 `Clone`；回调集合会共享，而每次 `run`
  都会创建独立的执行状态。
- 观察者方法和回调阶段更名为 `on_before_attempt` 与 `BeforeAttempt`，其准入前的
  执行位置保持不变。
- 非字符串回调 payload 的析构再次 panic 时，仍保留 `NonString` 分类和结构化回调终态；
  只有这个异常路径中的二次 payload 会被遗忘。

## 测试

```bash
# 使用默认 feature 集运行测试
cargo test

# 使用项目声明的全部 feature 运行测试
cargo test --all-features

# 运行项目 CI 检查
./ci-check.sh

# 检查代码覆盖率
./coverage.sh
```

## 许可证

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

本项目基于 Apache License 2.0 授权。完整许可证文本请参阅
[LICENSE](LICENSE)。

## 贡献

欢迎贡献。请遵循 Rust API 指南，及时更新公共 API 文档与测试，并在提交
Pull Request 前运行 `./align-ci.sh`格式化代码，运行`./ci-check.sh`对齐CI要求。

## 作者

**Haixing Hu** - *Qubit Co. Ltd.*

仓库地址：[https://github.com/qubit-ltd/rs-retry](https://github.com/qubit-ltd/rs-retry)
