# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Documentation](https://img.shields.io/badge/docs-English-blue.svg)](README.md)

Qubit Retry 是面向 Rust 同步和异步操作的重试工具库，能够保留调用方的错误类型。

核心 API 是 `Retry<E>`。重试策略只绑定操作错误类型 `E`；每次 `run` 或 `run_async` 调用再引入自己的成功类型 `T`。

## 概览

Qubit Retry 适用于需要对易失败任务进行明确、可观测重试控制的 Rust 应用。它支持同步操作、基于 Tokio 的异步操作，以及隔离到 worker 线程中的阻塞任务。重试策略可通过 builder 配置，也可以在开启 `config` feature 后从 `qubit-config` 读取；生命周期 hook 能观察每次 attempt、失败、重试决策、终止错误和成功结果。

当你需要类型化的 retry error、受限的 elapsed 时间预算、Retry-After hint、能捕获 panic 的 worker 执行，或可由闭包/可复用函数对象实现的重试回调时，可以使用本 crate。

## 特性

- 同步重试不依赖任何可选 feature。
- 基于 Tokio 的异步重试通过可注入的单调 sleeper 支持单次 attempt 超时。
- 阻塞操作可通过 `run_in_worker` 使用线程隔离执行、panic 捕获、超时等待和协作取消。
- 可选的 `qubit-config` 集成可从配置中读取重试设置。
- 回调基于 `rs-function` 函子保存，既支持闭包，也支持自定义函数对象。
- `AttemptFailure<E>` 表示一次 attempt 失败：`Error(E)`、`Timeout`、`Panic(AttemptPanic)` 或 `Executor(AttemptExecutorError)`。
- `RetryError<E>` 表示整个 retry 流程的终止错误，包含 `reason`、`last_failure` 和 `RetryContext`。
- 独立的 elapsed 预算区分用户 operation 执行时间和整个 retry flow 时间。
- 生命周期 hook 明确分为：`before_attempt`、`on_success`、`on_failure`、`on_retry`、`on_error`。

## 安装

```toml
[dependencies]
qubit-retry = "0.16"
```

按需开启可选集成：

```toml
[dependencies]
qubit-retry = { version = "0.16", features = ["tokio", "config"] }
```

可选 feature：

- `tokio`：启用 `Retry::run_async`，并通过可注入的
  `qubit_clock::Timer` 支持异步单次 attempt 超时。
- `config`：启用 `RetryOptions::from_config` 和 `RetryConfigValues`，用于从 `qubit-config` 读取配置。

默认 feature 为空，因此同步重试不会引入 `tokio` 或 `qubit-config`。

## 基础同步重试

```rust
use qubit_retry::Retry;
use std::time::Duration;

fn read_config() -> Result<String, Box<dyn std::error::Error>> {
    let retry = Retry::<std::io::Error>::builder()
        .max_attempts(3)
        .fixed_delay(Duration::from_millis(100))
        .build()?;

    let text = retry.run(|| std::fs::read_to_string("config.toml"))?;
    Ok(text)
}
```

## 失败决策

默认情况下，operation error 会被重试，直到配置的 attempt 次数或总耗时限制终止流程。简单错误谓词可以使用 `retry_if_error`：

```rust
use qubit_retry::{Retry, RetryContext};
use std::time::Duration;

let retry = Retry::<ServiceError>::builder()
    .max_attempts(4)
    .exponential_backoff(Duration::from_millis(100), Duration::from_secs(2))
    .retry_if_error(|error: &ServiceError, _context: &RetryContext| error.is_retryable())
    .build()?;
```

如果决策需要读取 failure 类型、attempt timeout、retry-after hint 或其他 `RetryContext` 信息，可以使用 `on_failure`：

```rust
use qubit_retry::{Retry, RetryContext, AttemptFailure, AttemptFailureDecision};
use std::time::Duration;

let retry = Retry::<ServiceError>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .on_failure(
        |failure: &AttemptFailure<ServiceError>, context: &RetryContext| match failure {
            AttemptFailure::Error(error) if error.is_rate_limited() => {
                AttemptFailureDecision::RetryAfter(Duration::from_secs(1))
            }
            AttemptFailure::Error(error) if error.is_retryable() => AttemptFailureDecision::Retry,
            AttemptFailure::Timeout if context.attempt_timeout().is_some() => {
                AttemptFailureDecision::Abort
            }
            AttemptFailure::Panic(_) => AttemptFailureDecision::Abort,
            AttemptFailure::Executor(_) => AttemptFailureDecision::Abort,
            _ => AttemptFailureDecision::UseDefault,
        },
    )
    .build()?;
```

`AttemptFailureDecision::UseDefault` 表示把控制权交回重试策略，由已配置的次数限制、耗时限制、delay、jitter 和可选 retry-after hint 决定下一步。

## 异步重试和超时

异步执行需要开启 `tokio` feature。单次 attempt 超时通过 builder 写入 `RetryOptions`。当 attempt 超时时，执行器会报告 `AttemptFailure::Timeout`，监听器可以通过 `RetryContext::attempt_timeout()` 读取配置的超时时间。operation panic 仍会在当前 async task 中继续 unwind；`run_async()` 不会把它转换成 `AttemptFailure::Panic`。

未注入 async timer 时，默认 Tokio clock 与 timer 会在 `run_async()` future
首次 poll 时创建。因此 retry policy 可以在 runtime 外构建，同时 paused Tokio time
仍与实际执行它的 runtime 对齐。显式注入的 Tokio timer 会保存自己的目标 runtime
Handle，可以在其他 runtime context 中 poll；目标 Runtime 必须保持存活并持续驱动，
直到 retry future 完成。

```rust
use qubit_retry::Retry;
use std::time::Duration;

async fn fetch_once() -> Result<String, std::io::Error> {
    Ok("response".to_string())
}

async fn fetch_with_retry() -> Result<String, Box<dyn std::error::Error>> {
    let retry = Retry::<std::io::Error>::builder()
        .max_attempts(3)
        .fixed_delay(Duration::from_millis(50))
        .attempt_timeout(Some(Duration::from_secs(2)))
        .retry_on_timeout()
        .build()?;

    let response = retry
        .run_async(|| async {
            fetch_once().await
        })
        .await?;

    Ok(response)
}
```

普通 `run()` 保持当前线程上的同步执行语义。它是开销最低的路径，适合 CAS 循环这类高频短操作。`run()` 不支持配置 `attempt_timeout`，当设置了该选项时会返回 `RetryErrorReason::UnsupportedOperation`。需要取消异步 future 时使用 `run_async()`；需要把阻塞工作放到 worker 线程中执行时，使用 `run_in_worker()`。

## Elapsed 预算

Retry 的 elapsed 预算通过当前执行模式所注入的单调 sleeper 计时，不使用
wall-clock 时间：

- `max_operation_elapsed`：累计用户 operation attempt 的执行时间。retry sleep、Retry-After sleep 和 listener 时间都不计入。
- `max_total_elapsed`：整个 retry flow 的总时间。operation attempt、retry sleep、Retry-After sleep、retry hint 提取、`before_attempt`、`on_failure` 和 `on_retry` 时间都计入。

终态 listener 保持通知语义。`on_success` 和 `on_error` 的耗时会增加调用方实际等待时间，但不会把已经成功的 operation 反向变成 retry failure。

async 和 worker-thread attempt 会从配置的 `attempt_timeout`、剩余 `max_operation_elapsed`、剩余 `max_total_elapsed` 中选最短值作为有效 attempt timeout。如果下一次 retry 或 Retry-After 延迟会耗尽剩余 `max_total_elapsed`，流程会在 sleep 前以 `RetryErrorReason::MaxTotalElapsedExceeded` 失败。retry sleep 不会被截断。

## 测试中的确定性时间

`RetryBuilder::blocking_timer` 配置 `run()` 和 `run_in_worker()` 使用的
clock 与等待机制。启用 `tokio` feature 后，`RetryBuilder::async_timer` 对
`run_async()` 提供同样能力，并负责异步 attempt timeout。因此固定延迟、指数退避和
Retry-After 延迟都可以使用手动时钟测试，无需等待真实时间：

```rust
use std::time::Duration;

use qubit_clock::{ManualMonotonicClock, MonotonicClock};
use qubit_retry::Retry;

let clock = ManualMonotonicClock::new_shared();
let timer = clock.new_timer();
let retry = Retry::<std::io::Error>::builder()
    .max_attempts(3)
    .exponential_backoff(Duration::from_secs(1), Duration::from_secs(8))
    .blocking_timer(timer)
    .build()?;
```

同一个 timer 始终同时承担单调 clock 与 delay driver，elapsed 预算和 sleep
不会悄悄落入不同时间域。如果 timer 无法表示某个 deadline，执行会以
`RetryErrorReason::SleeperFailed` 终止。

`run()` 和 `run_in_worker()` 会通过 `BlockingSleeper` 适配 `blocking_timer`；调用线程
park 后，其 backend 必须继续独立推进。standard timer 自带 worker，manual time 必须
由其他控制方 advance。不要注入以被阻塞调用线程作为唯一 current-thread runtime
驱动者的 Tokio timer。`async_timer` 没有阻塞要求，但其 deadline driver 必须保持存活
并持续推进。

worker attempt timeout 和 cancellation grace 仍使用操作系统线程/channel 时间：
注入的 blocking timer 控制 retry flow 的 elapsed 统计与 attempt 之间的退避，
但不能替代用于观察真实 worker 线程是否退出的原生等待。

## Worker 线程重试

`run_in_worker()` 会把每次 attempt 都放到 worker 线程中运行。没有配置 attempt timeout 时，调用方等待 worker 返回，并把 worker panic 捕获为 `AttemptFailure::Panic`。worker 线程启动失败会报告为 `AttemptFailure::Executor`。配置了 attempt timeout 时，retry executor 会在超时后停止等待该 worker，标记本次 attempt 的 token 为 cancelled，并最多等待 `worker_cancel_grace`（默认 `100ms`）让 worker 退出，然后再按配置的 `AttemptTimeoutPolicy` 继续处理。

Rust 不能安全地强杀运行中的线程，因此如果 operation 不检查 token 并主动返回，超时后的 worker 可能会继续运行。如果 worker 在取消 grace 结束后仍未退出，retry flow 会返回 `RetryErrorReason::WorkerStillRunning`，不会再启动新的 worker；`RetryContext::unreaped_worker_count()` 会记录未回收 worker 数量。阻塞 IO、第三方调用、可能 panic 的代码，或需要单次 attempt 超时隔离的任务适合使用这一路径；低延迟内存操作优先使用普通 `run()`。

```rust
use qubit_retry::{AttemptCancelToken, Retry};
use std::time::Duration;

fn blocking_fetch(token: AttemptCancelToken) -> Result<String, std::io::Error> {
    for _ in 0..20 {
        if token.is_cancelled() {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::read_to_string("payload.txt")
}

let retry = Retry::<std::io::Error>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(50))
    .attempt_timeout(Some(Duration::from_secs(2)))
    .worker_cancel_grace(Duration::from_millis(25))
    .abort_on_timeout()
    .build()?;

let response = retry.run_in_worker(blocking_fetch)?;
```

## Retry-After Hint

如果 attempt failure 中携带 retry-after 信息，可以通过 `retry_after_hint` 注册 hint extractor。extractor 的返回值是 `Option<Duration>`：`Some(delay)` 表示“下一次重试前等待这段时间”，`None` 表示“没有可用 hint”。当所有 failure listener 都返回 `UseDefault` 时，默认策略会优先使用 `Some(delay)`；否则会回退到已配置的 delay 策略。

```rust
use qubit_retry::{AttemptFailure, Retry, RetryContext};
use std::time::Duration;

let retry = Retry::<ServiceError>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .retry_after_hint(
        |failure: &AttemptFailure<ServiceError>, _context: &RetryContext| {
            failure.as_error().and_then(ServiceError::retry_after)
        },
    )
    .build()?;
```

如果 hint 只依赖 operation error，可以使用 `retry_after_from_error`，这是 `retry_after_hint` 的简化封装：

```rust
let retry = Retry::<ServiceError>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .retry_after_from_error(|error: &ServiceError| error.retry_after())
    .build()?;
```

listener 也可以通过 `RetryContext::retry_after_hint()` 读取提取结果。

## 监听器

listener 是生命周期 hook，而不是另一套策略系统：

- `before_attempt`：在**每次真正执行用户操作之前**调用（含首次 attempt），接收即将执行的一基序号。只有 callback 返回且 post-listener 预算复查通过后，本次 attempt 才算准入；因此 callback 可能看到 attempt `1`，但 operation 调用次数为 `0`，终态 `RetryError::attempts()` 也为 `0`。
- `on_success`：每次 attempt 成功后调用。
- `on_failure`：每个已准入 attempt 产生 `AttemptFailure` 时恰好调用一次，并返回 `AttemptFailureDecision`；在**选定**到下一次 attempt 前的等待时间、以及 `on_retry` **之前**执行，通常可影响退避/中止等决策。
- `on_retry`：在**已确认**会再试、且**到下一次** `before_attempt` **之前**的等待时间已从策略中**算出之后**调用（即晚于与本次失败相关的 `on_failure` 与决策合并）；在 executor **进入 sleep 等待**、以及下一次 `before_attempt` **之前**触发。**只读观察**（不能改变重试/退避）；`RetryContext::next_delay()` 为即将用于 sleep 的等待时长。若不会重试（资源用尽、被中止、已达末次等），**不会**调用 `on_retry`。
- `on_error`：retry 流程返回终止 `RetryError` 时调用一次。

注册多个 failure listener 时，所有 listener 会按注册顺序执行。最后一个非 `UseDefault` 的 `AttemptFailureDecision` 会成为最终生效决策；如果所有 listener 都返回 `UseDefault`，则交回已配置的 retry 策略处理。

如果 in-flight timeout 来自已耗尽的 `max_operation_elapsed` 或 `max_total_elapsed`，它属于不可覆盖的硬终止。retry-after extractor 和所有 `on_failure` listener 仍会各执行一次，但 listener 决策不能覆盖已耗尽的预算，且不会发出 `on_retry`；随后 `on_error` 恰好观察一次硬预算终态。未由已准入 attempt 产生的终态诊断——例如 pre-attempt 预算终止、不受支持的执行配置，或 attempt 外的 sleeper/executor 失败——会直接进入 `on_error`，不会合成 `on_failure` 事件。

`before_attempt` 与 `on_retry` 的直观差别：`before_attempt` 对准「**下一次** attempt **开始前**」；`on_retry` 对准「**某次** attempt **已经失败**、且**已经**为**后续**重试**选好间隔**、但**尚未**开始等待或下一轮 attempt 的那一刻」。

```rust
use qubit_retry::{
    AttemptFailure, AttemptFailureDecision, Retry, RetryContext, RetryError,
};

let retry = Retry::<std::io::Error>::builder()
    .max_attempts(3)
    .before_attempt(|context: &RetryContext| {
        tracing::debug!(attempt = context.attempt(), "starting attempt");
    })
    .on_success(|context: &RetryContext| {
        tracing::debug!(attempt = context.attempt(), "attempt succeeded");
    })
    .on_failure(
        |failure: &AttemptFailure<std::io::Error>, context: &RetryContext| {
            tracing::warn!(
                failure = %failure,
                attempt = context.attempt(),
                retry_after_hint = ?context.retry_after_hint(),
                "attempt failed",
            );
            AttemptFailureDecision::UseDefault
        },
    )
    .on_retry(
        |failure: &AttemptFailure<std::io::Error>, context: &RetryContext| {
            tracing::info!(
                failure = %failure,
                attempt = context.attempt(),
                next_delay = ?context.next_delay(),
                "will sleep before next attempt",
            );
        },
    )
    .on_error(|error: &RetryError<std::io::Error>, context: &RetryContext| {
        tracing::error!(
            reason = ?error.reason(),
            attempts = context.attempt(),
            operation_elapsed_ms = context.operation_elapsed().as_millis(),
            total_elapsed_ms = context.total_elapsed().as_millis(),
            "retry flow failed",
        );
    })
    .build()?;
```

## 配置

`RetryOptions` 是不可变配置快照。从 `qubit-config` 读取配置需要开启 `config` feature，并且只发生在构造阶段。

```rust
use qubit_config::Config;
use qubit_retry::{Retry, RetryOptions};

let mut config = Config::new();
config.set("retry.max_attempts", 5u32)?;
config.set("retry.max_operation_elapsed_millis", 30_000u64)?;
config.set("retry.max_total_elapsed_millis", 60_000u64)?;
config.set("retry.delay", "exponential")?;
config.set("retry.exponential_initial_delay_millis", 200u64)?;
config.set("retry.exponential_max_delay_millis", 5_000u64)?;
config.set("retry.exponential_multiplier", 2.0)?;
config.set("retry.jitter_factor", 0.2)?;
config.set("retry.attempt_timeout_millis", 2_000u64)?;
config.set("retry.attempt_timeout_policy", "retry")?;
config.set("retry.worker_cancel_grace_millis", 25u64)?;

let options = RetryOptions::from_config(&config.prefix_view("retry"))?;
let retry = Retry::<std::io::Error>::from_options(options)?;
```

支持的相对配置键：

- `max_attempts`
- `max_operation_elapsed_millis`
- `max_operation_elapsed_unlimited`
- `max_total_elapsed_millis`
- `max_total_elapsed_unlimited`
- `attempt_timeout_millis`
- `attempt_timeout_policy`：`retry` 或 `abort`
- `worker_cancel_grace_millis`
- `delay`：`none`、`fixed`、`random`、`exponential` 或 `exponential_backoff`
- `fixed_delay_millis`
- `random_min_delay_millis`
- `random_max_delay_millis`
- `exponential_initial_delay_millis`
- `exponential_max_delay_millis`
- `exponential_multiplier`
- `jitter_factor`

## 错误处理

通过 `RetryError::reason()`、`RetryError::last_failure()` 和 `RetryError::context()` 可以区分终止原因与最后一次 attempt 失败：

```rust
use qubit_retry::{Retry, RetryErrorReason, AttemptFailure};

let retry = Retry::<std::io::Error>::builder()
    .max_attempts(2)
    .build()?;

match retry.run(|| std::fs::read_to_string("missing.toml")) {
    Ok(text) => println!("{text}"),
    Err(error) => {
        eprintln!("reason: {:?}", error.reason());
        eprintln!("admitted attempts: {}", error.attempts());
        eprintln!("operation elapsed: {:?}", error.context().operation_elapsed());
        eprintln!("total elapsed: {:?}", error.context().total_elapsed());

        match error.last_failure() {
            Some(AttemptFailure::Error(source)) => {
                eprintln!("last operation error: {source}");
            }
            Some(AttemptFailure::Timeout) => {
                eprintln!("last attempt timed out");
            }
            Some(AttemptFailure::Panic(panic)) => {
                eprintln!("last attempt panicked: {}", panic.message());
            }
            Some(AttemptFailure::Executor(executor)) => {
                eprintln!("retry executor failed: {}", executor.message());
            }
            None => {}
        }
    }
}
```

## 文档

- API 文档：[docs.rs/qubit-retry](https://docs.rs/qubit-retry)
- Crate 发布页：[crates.io/crates/qubit-retry](https://crates.io/crates/qubit-retry)
- 源码仓库：[github.com/qubit-ltd/rs-retry](https://github.com/qubit-ltd/rs-retry)

## 测试

快速在本地跑一遍：

```bash
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

若要与持续集成（CI）保持一致，请在项目根目录执行：

```bash
./align-ci.sh
./ci-check.sh
./coverage.sh
```

`./align-ci.sh` 会格式化代码并执行本地 Clippy 修复，使分支与 CI 规则对齐。`./ci-check.sh` 会运行与流水线等价的完整检查，包括格式检查、Clippy warnings deny、debug/release 构建、all-feature 测试、rustdoc warnings deny、JSON 覆盖率阈值检查以及安全审计。`./coverage.sh` 用于生成覆盖率报告；可通过 `./coverage.sh help` 查看 HTML、text、LCOV、JSON、Cobertura 或 all 等输出格式。

## 参与贡献

欢迎通过 Issue 与 Pull Request 参与本仓库。建议：

- 报告缺陷、讨论设计或较大能力扩展时，可先开 Issue 对齐方向再投入实现。
- 单次 PR 尽量聚焦单一行为变更、缺陷修复或文档更新，便于审查与合并。
- 代码贡献在提交前必须运行 `./align-ci.sh`，通过 `./ci-check.sh`，并使用 `./coverage.sh` 查看覆盖率。
- 修改运行期行为时，请补充或更新相应测试。
- 若影响对外 API 或用户可见行为，请同步更新本文档或相关 rustdoc。

向本仓库贡献内容即表示您同意以 [Apache License, Version 2.0](LICENSE)（与本项目相同）授权您的贡献。

## 许可证与版权

Copyright (c) 2026. Haixing Hu.

本软件依据 [Apache License, Version 2.0](LICENSE) 授权；完整许可文本见仓库根目录的 `LICENSE` 文件。

## 作者与维护

**Haixing Hu** — Qubit Co. Ltd.

| | |
| --- | --- |
| **源码仓库** | [github.com/qubit-ltd/rs-retry](https://github.com/qubit-ltd/rs-retry) |
| **API 文档** | [docs.rs/qubit-retry](https://docs.rs/qubit-retry) |
| **Crate 发布** | [crates.io/crates/qubit-retry](https://crates.io/crates/qubit-retry) |
