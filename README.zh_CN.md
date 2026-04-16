# Qubit Retry

[![CircleCI](https://circleci.com/gh/qubit-ltd/rs-retry.svg?style=shield)](https://circleci.com/gh/qubit-ltd/rs-retry)
[![Coverage Status](https://coveralls.io/repos/github/qubit-ltd/rs-retry/badge.svg?branch=main)](https://coveralls.io/github/qubit-ltd/rs-retry?branch=main)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

Qubit Retry 为 Rust 同步和异步操作提供保留错误类型的重试策略。

核心 API 是 `RetryExecutor<E>`。执行器只绑定操作错误类型 `E`；成功类型 `T` 由 `run` 或 `run_async` 在执行时引入。因此普通错误重试不再要求 `T: Clone + Eq + Hash`。

## 特性

- 保留原始错误类型的 `RetryError<E>`。
- 通过 `RetryExecutor::run` 执行同步重试。
- 通过 `RetryExecutor::run_async` 执行异步重试。
- 通过 `RetryExecutor::run_async_with_timeout` 实现真实的异步单次 attempt 超时。
- 通过 `timeout_decision` / `abort_on_timeout` 可配置 timeout 失败时的处理行为。
- 延迟策略：`RetryDelay::none`、`RetryDelay::fixed`、`RetryDelay::random`、`RetryDelay::exponential`。
- 通过 `RetryJitter::factor` 支持对称 jitter。
- 通过 `retry_if` 或 `retry_decide` 显式判断错误是否可重试。
- 重试/失败/终止监听使用 context + borrowed failure 载荷模型。
- 监听回调使用 `qubit-function` 函子存储（`ArcConsumer` / `ArcBiConsumer`）。
- `RetryOptions` 是不可变配置快照，支持从 `qubit-config` 读取。

## 核心概念

`qubit-retry` 采用“保留错误类型”的执行器设计，把重试策略、错误判定与业务执行边界清晰分离：

- `RetryExecutor<E>` 保存重试行为和错误分类逻辑。
- `run<T, _>` 和 `run_async<T, _, _>` 只在执行时引入成功类型。
- 监听回调只观察 context 元数据与 borrowed failure，不持有成功值。
- `RetryOptions` 提供经过验证的不可变重试配置快照。

## 安装

```toml
[dependencies]
qubit-retry = "0.6.0"
```

## 基础同步重试

```rust
use qubit_retry::{RetryDelay, RetryExecutor};
use std::time::Duration;

fn read_config() -> Result<String, Box<dyn std::error::Error>> {
    let executor = RetryExecutor::<std::io::Error>::builder()
        .max_attempts(3)
        .delay(RetryDelay::fixed(Duration::from_millis(100)))
        .build()?;

    let text = executor.run(|| std::fs::read_to_string("config.toml"))?;
    Ok(text)
}
```

## 错误判定

默认情况下，所有操作错误都会被视为可重试，直到 attempt 次数或总耗时预算耗尽。只有部分错误可重试时，使用 `retry_if`：

```rust
use qubit_retry::{RetryDelay, RetryExecutor};
use std::time::Duration;

#[derive(Debug)]
enum ServiceError {
    RateLimited,
    TemporaryUnavailable,
    InvalidRequest,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ServiceError {}

fn is_retryable(error: &ServiceError) -> bool {
    matches!(
        error,
        ServiceError::RateLimited | ServiceError::TemporaryUnavailable
    )
}

let executor = RetryExecutor::<ServiceError>::builder()
    .max_attempts(4)
    .delay(RetryDelay::exponential(
        Duration::from_millis(100),
        Duration::from_secs(2),
        2.0,
    ))
    .retry_if(|error, _context| is_retryable(error))
    .build()?;
```

需要返回具名决策时，可以使用 `retry_decide`：

```rust
use qubit_retry::{RetryDecision, RetryExecutor};

let executor = RetryExecutor::<ServiceError>::builder()
    .max_attempts(3)
    .retry_decide(|error, context| {
        if context.attempt == 1 && is_retryable(error) {
            RetryDecision::Retry
        } else {
            RetryDecision::Abort
        }
    })
    .build()?;
```

## 异步重试和单次 attempt 超时

`run_async_with_timeout` 基于 `tokio::time::timeout`，超时 attempt 会在 Future 边界被真实取消。

```rust
use qubit_retry::{RetryDelay, RetryExecutor};
use std::time::Duration;

async fn fetch_once() -> Result<String, std::io::Error> {
    Ok("response".to_string())
}

async fn fetch_with_retry() -> Result<String, Box<dyn std::error::Error>> {
    let executor = RetryExecutor::<std::io::Error>::builder()
        .max_attempts(3)
        .delay(RetryDelay::fixed(Duration::from_millis(50)))
        .build()?;

    let response = executor
        .run_async_with_timeout(Duration::from_secs(2), || async {
            fetch_once().await
        })
        .await?;

    Ok(response)
}
```

不需要单次 attempt 超时时，使用 `run_async`：

```rust
let response = executor
    .run_async(|| async {
        fetch_once().await
    })
    .await?;
```

## 监听器

重试/失败/终止监听都会收到一个 context 对象和 borrowed failure 参数。成功监听仍只接收 `SuccessContext`。

```rust
pub type RetryListener<E> = ArcBiConsumer<RetryContext, RetryAttemptFailure<E>>;
pub type FailureListener<E> = ArcBiConsumer<RetryFailureContext, Option<RetryAttemptFailure<E>>>;
pub type AbortListener<E> = ArcBiConsumer<RetryAbortContext, RetryAttemptFailure<E>>;
pub type SuccessListener = ArcConsumer<RetrySuccessContext>;
```

```rust
use qubit_retry::{RetryAttemptFailure, RetryDelay, RetryExecutor};
use std::time::Duration;

let executor = RetryExecutor::<std::io::Error>::builder()
    .max_attempts(3)
    .delay(RetryDelay::fixed(Duration::from_millis(100)))
    .on_retry(|context, failure| {
        if let RetryAttemptFailure::Error(error) = failure {
            tracing::warn!(
                attempt = context.attempt,
                delay_ms = context.next_delay.as_millis(),
                error = %error,
                "retrying operation",
            );
        }
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

## 配置

`RetryOptions` 是不可变快照。从 `qubit-config` 读取只发生在构造阶段。

```rust
use qubit_config::Config;
use qubit_retry::{RetryOptions, RetryExecutor};

let mut config = Config::new();
config.set("retry.max_attempts", 5u32)?;
config.set("retry.max_elapsed_millis", 30_000u64)?;
config.set("retry.delay", "exponential")?;
config.set("retry.exponential_initial_delay_millis", 200u64)?;
config.set("retry.exponential_max_delay_millis", 5_000u64)?;
config.set("retry.exponential_multiplier", 2.0)?;
config.set("retry.jitter_factor", 0.2)?;

let options = RetryOptions::from_config(&config.prefix_view("retry"))?;
let executor = RetryExecutor::<std::io::Error>::from_options(options)?;
```

支持的相对配置键：

- `max_attempts`
- `max_elapsed_millis`（`0` 表示 0ms 预算）
- `max_elapsed_unlimited`（`true` 表示显式无限总耗时）
- `delay`：`none`、`fixed`、`random`、`exponential` 或 `exponential_backoff`
- `fixed_delay_millis`
- `random_min_delay_millis`
- `random_max_delay_millis`
- `exponential_initial_delay_millis`
- `exponential_max_delay_millis`
- `exponential_multiplier`
- `jitter_factor`

## 错误处理

如果最终失败来自业务操作错误，`RetryError<E>` 会保留原始错误：

```rust
use qubit_retry::{RetryError, RetryExecutor};

let executor = RetryExecutor::<std::io::Error>::builder()
    .max_attempts(2)
    .build()?;

match executor.run(|| std::fs::read_to_string("missing.toml")) {
    Ok(text) => println!("{text}"),
    Err(error) => {
        eprintln!("attempts: {}", error.attempts());
        if let Some(source) = error.last_error() {
            eprintln!("last error: {source}");
        }

        if let RetryError::AttemptsExceeded { max_attempts, .. } = error {
            eprintln!("max attempts: {max_attempts}");
        }
    }
}
```
