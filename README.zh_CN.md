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
qubit-retry = "0.19"
```

Tokio 执行与稳定的配置序列化均为可选能力：

```toml
qubit-retry = { version = "0.19", features = ["tokio", "serde"] }
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

## 为什么需要这个项目

手写重试循环常把尝试次数、等待、关闭信号与错误转换散落在多个调用点，最终很容易丢掉最后一次业务错误，或把超时、取消和回调 panic 混成同一种失败。Qubit Retry
把这些决策收敛到策略驱动的流程中，并让成功与终态错误都携带一致的 `RetryContext`。

## 核心能力与边界

- `RetryPolicy` 把尝试次数、操作时限与总流程时限预算，同立即、固定、均匀或指数退避组合起来。
- `Retry::sync()` 在当前线程执行闭包，不承诺超时或取消，因为任意同步代码无法被安全中断。
- `Retry::asynchronous()` 在 `tokio` 特性下支持单次尝试/流程超时与取消；Tokio 是唯一的异步运行时集成。
- `Retry::worker()` 捕获 panic、保留停止触发源，并且在旧工作线程仍可能运行时绝不启动新工作线程。
- `RetryFailure` 区分主动中止、预算耗尽、超时、取消、回调失败和基础设施失败；
  `AttemptFailure` 保留业务错误、超时范围或稳定的 panic 载荷。
- 规则与观察者按注册顺序执行；回调 panic 会失败即终止，并保留回调类型、索引、阶段和字符串载荷。
- `BackoffState` 可用于普通重试与 SSE 重连，支持服务端提示、抖动和防溢出的指数增长。
- 可选 `serde` 特性只序列化配置；运行结果、错误和回调状态不会作为稳定的跨进程序列化协议。

预算只决定能否启动下一次尝试。已经开始的操作即使越过预算后才成功，仍会返回成功。策略不会强杀同步操作或不配合的工作线程。取消令牌
不可重置，也不提供父子树或内置截止时间。

## 延伸阅读

- [Rust API 文档](https://docs.rs/qubit-retry)
- [English README](README.md)
- [代码仓库](https://github.com/qubit-ltd/rs-retry)

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
