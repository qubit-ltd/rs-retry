# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

Qubit Retry 是一个面向 Rust 易失败任务的类型安全重试引擎，适合 HTTP 客户端、存储层和后台服务。它让调用方保留原始业务错误，同时清楚知道流程为何停止，避免把重试逻辑散落在业务代码中。

## 一个完整场景

HTTP 客户端可以把瞬时网络错误交给重试规则处理，并保留服务端的 `Retry-After` 提示：

```rust
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
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

let response = retry.sync().run(|| std::fs::read("Cargo.toml"))?.into_value();
# Ok::<(), Box<dyn std::error::Error>>(())
```

同一个策略还可以交给 `retry.asynchronous()` 处理 Tokio future，或交给
`retry.worker()` 隔离阻塞任务。

## 安装

```toml
[dependencies]
qubit-retry = "0.18"
```

异步执行需要开启 Tokio feature：

```toml
qubit-retry = { version = "0.18", features = ["tokio"] }
```

## 核心能力与边界

- `RetryPolicy` 将 attempt/elapsed 预算和不透明、可复用的
  `BackoffPolicy` 组合在一起。
- `Retry::sync()` 在当前线程执行闭包，不提供 timeout；任意同步 Rust 代码都无法被安全强制中断。
- `Retry::asynchronous()`（需要 `tokio` feature）提供单次 attempt 和整个 flow 的 timeout。
- `Retry::worker()` 在 worker 线程隔离阻塞 attempt，捕获 panic，并在启动下一个 worker 前等待协作式取消。
- 按注册顺序执行 `RetryRule`，第一个非 `UseDefault` 决策生效；`RetryObserver` 接收隔离后的生命周期回调。
- `AttemptFailureKind` 和 `RetryErrorKind` 是稳定分类；详细的
  `AttemptFailure` 与 `RetryErrorReason` 标记为 non-exhaustive，便于未来扩展运行时细节。
- `BackoffState` 同时支持普通 retry 与 SSE 等 reconnect 流程，可处理显式延迟、服务端 hint、jitter 和指数增长溢出。

预算只决定是否允许启动下一次 attempt。已经开始的 operation 即使跨过预算后才成功，也会返回成功。策略不会强杀同步 operation，也不会强杀不配合取消的 worker 线程。

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
