# Qubit Retry

[![CircleCI](https://circleci.com/gh/qubit-ltd/rs-retry.svg?style=shield)](https://circleci.com/gh/qubit-ltd/rs-retry)
[![Coverage Status](https://coveralls.io/repos/github/qubit-ltd/rs-retry/badge.svg?branch=main)](https://coveralls.io/github/qubit-ltd/rs-retry?branch=main)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

Qubit Retry 为 Rust 同步和异步操作提供保留错误类型的重试策略。

核心 API 是 `Retry<E>`。重试策略只绑定操作错误类型 `E`；成功类型 `T` 由每次 `run` 或 `run_async` 调用引入。

## 特性

- 同步重试不依赖任何可选 feature。
- 异步重试和单次 attempt 超时通过 `tokio` feature 提供。
- `qubit-config` 集成通过 `config` feature 提供。
- 回调基于 `rs-function` 函子保存，既支持闭包，也支持自定义函数对象。
- `AttemptFailure<E>` 表示一次 attempt 失败：`Error(E)` 或 `Timeout`。
- `RetryError<E>` 表示整个 retry 流程的终止错误，包含 `reason`、`last_failure` 和 `RetryContext`。
- 生命周期 hook 明确分为：`before_attempt`、`on_success`、`on_failure`、`on_error`。

## 安装

```toml
[dependencies]
qubit-retry = "0.7.0"
```

按需开启可选集成：

```toml
[dependencies]
qubit-retry = { version = "0.7.0", features = ["tokio", "config"] }
```

可选 feature：

- `tokio`：启用 `Retry::run_async`，并通过 `tokio::time::timeout` 支持异步单次 attempt 超时。
- `config`：启用 `RetryOptions::from_config` 和 `RetryConfigValues`，用于从 `qubit-config` 读取重试配置。

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

默认情况下，operation error 会被重试，直到 attempt 次数或总耗时限制终止流程。简单错误谓词使用 `retry_if_error`：

```rust
use qubit_retry::{Retry, RetryContext};
use std::time::Duration;

let retry = Retry::<ServiceError>::builder()
    .max_attempts(4)
    .exponential_backoff(Duration::from_millis(100), Duration::from_secs(2))
    .retry_if_error(|error: &ServiceError, _context: &RetryContext| error.is_retryable())
    .build()?;
```

如果决策需要读取 attempt timeout、retry-after hint 或 failure 类型，使用 `on_failure`：

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
            _ => AttemptFailureDecision::UseDefault,
        },
    )
    .build()?;
```

`AttemptFailureDecision::UseDefault` 表示交回框架默认策略，由已配置的次数限制、耗时限制、delay、jitter 和可选 retry-after hint 决定下一步。

## 异步重试和超时

异步执行需要开启 `tokio` feature。单次 attempt 超时在 builder 上配置，超时后会产生 `AttemptFailure::Timeout`，超时时间可从 `RetryContext::attempt_timeout()` 读取。

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

## Retry-After Hint

如果 attempt failure 中携带 retry-after 信息，可以通过 `retry_after_hint` 注册 hint extractor。extractor 的返回值是 `Option<Duration>`：`Some(delay)` 表示“下一次重试前等待这段时间”，`None` 表示“没有可用 hint”。当所有 failure listener 都返回 `UseDefault` 时，默认策略会优先使用 `Some(delay)`；否则回退到已配置的 delay 策略。

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

如果 hint 只依赖 operation error，可以使用 `retry_after_from_error`，它是 `retry_after_hint` 的便捷封装：

```rust
let retry = Retry::<ServiceError>::builder()
    .max_attempts(3)
    .fixed_delay(Duration::from_millis(100))
    .retry_after_from_error(|error: &ServiceError| error.retry_after())
    .build()?;
```

listener 也可以通过 `RetryContext::retry_after_hint()` 读取提取结果。

## 监听器

listener 是生命周期 hook，不再拆成多套策略系统：

- `before_attempt`：每次 attempt 前调用，包括第一次 attempt。
- `on_success`：每次 attempt 成功后调用。
- `on_failure`：每次产生 `AttemptFailure` 后调用，并返回 `AttemptFailureDecision`。
- `on_error`：retry 流程返回终止 `RetryError` 时调用一次。

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
    .on_error(|error: &RetryError<std::io::Error>, context: &RetryContext| {
        tracing::error!(
            reason = ?error.reason(),
            attempts = context.attempt(),
            elapsed_ms = context.total_elapsed().as_millis(),
            "retry flow failed",
        );
    })
    .build()?;
```

## 配置

`RetryOptions` 是不可变配置快照。从 `qubit-config` 读取需要开启 `config` feature，并且只发生在构造阶段。

```rust
use qubit_config::Config;
use qubit_retry::{Retry, RetryOptions};

let mut config = Config::new();
config.set("retry.max_attempts", 5u32)?;
config.set("retry.max_elapsed_millis", 30_000u64)?;
config.set("retry.delay", "exponential")?;
config.set("retry.exponential_initial_delay_millis", 200u64)?;
config.set("retry.exponential_max_delay_millis", 5_000u64)?;
config.set("retry.exponential_multiplier", 2.0)?;
config.set("retry.jitter_factor", 0.2)?;

let options = RetryOptions::from_config(&config.prefix_view("retry"))?;
let retry = Retry::<std::io::Error>::from_options(options)?;
```

支持的相对配置键：

- `max_attempts`
- `max_elapsed_millis`
- `max_elapsed_unlimited`
- `delay`：`none`、`fixed`、`random`、`exponential` 或 `exponential_backoff`
- `fixed_delay_millis`
- `random_min_delay_millis`
- `random_max_delay_millis`
- `exponential_initial_delay_millis`
- `exponential_max_delay_millis`
- `exponential_multiplier`
- `jitter_factor`

## 错误处理

通过 `RetryError::reason()`、`RetryError::last_failure()` 和 `RetryError::context()` 区分终止原因与最后一次 attempt 失败：

```rust
use qubit_retry::{Retry, RetryErrorReason, AttemptFailure};

let retry = Retry::<std::io::Error>::builder()
    .max_attempts(2)
    .build()?;

match retry.run(|| std::fs::read_to_string("missing.toml")) {
    Ok(text) => println!("{text}"),
    Err(error) => {
        eprintln!("reason: {:?}", error.reason());
        eprintln!("attempts: {}", error.context().attempt());
        eprintln!("elapsed: {:?}", error.context().total_elapsed());

        if error.reason() == RetryErrorReason::AttemptsExceeded {
            if let Some(AttemptFailure::Error(source)) = error.last_failure() {
                eprintln!("last operation error: {source}");
            }
        }
    }
}
```
