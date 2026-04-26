# Qubit Retry

[CircleCI](https://circleci.com/gh/qubit-ltd/rs-retry)
[Coverage Status](https://coveralls.io/github/qubit-ltd/rs-retry?branch=main)
[Crates.io](https://crates.io/crates/qubit-retry)
[Rust](https://www.rust-lang.org)
[License](LICENSE)
[English Document](README.md)

Qubit Retry 是面向 Rust 同步和异步操作的重试工具库，能够保留调用方的错误类型。

核心 API 是 `Retry<E>`。重试策略只绑定操作错误类型 `E`；每次 `run` 或 `run_async` 调用再引入自己的成功类型 `T`。

## 特性

- 同步重试不依赖任何可选 feature。
- 基于 Tokio 的异步重试支持真正的单次 attempt 超时。
- 阻塞操作可通过 `run_in_worker` 使用线程隔离执行、panic 捕获、超时等待和协作取消。
- 可选的 `qubit-config` 集成可从配置中读取重试设置。
- 回调基于 `rs-function` 函子保存，既支持闭包，也支持自定义函数对象。
- `AttemptFailure<E>` 表示一次 attempt 失败：`Error(E)`、`Timeout`、`Panic(AttemptPanic)` 或 `Executor(AttemptExecutorError)`。
- `RetryError<E>` 表示整个 retry 流程的终止错误，包含 `reason`、`last_failure` 和 `RetryContext`。
- 生命周期 hook 明确分为：`before_attempt`、`on_success`、`on_failure`、`on_retry`、`on_error`。

## 安装

```toml
[dependencies]
qubit-retry = "0.8.0"
```

按需开启可选集成：

```toml
[dependencies]
qubit-retry = { version = "0.8.0", features = ["tokio", "config"] }
```

可选 feature：

- `tokio`：启用 `Retry::run_async`，并通过 `tokio::time::timeout` 支持异步单次 attempt 超时。
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

普通 `run()` 保持当前线程上的同步执行语义。它是开销最低的路径，适合 CAS 循环这类高频短操作；但 operation panic 会继续向调用方传播，且不会应用单次 attempt timeout。需要取消异步 future 时使用 `run_async()`；需要把阻塞工作放到 worker 线程中执行时，使用 `run_in_worker()`。

## Worker 线程重试

`run_in_worker()` 会把每次 attempt 都放到 worker 线程中运行。没有配置 attempt timeout 时，调用方等待 worker 返回，并把 worker panic 捕获为 `AttemptFailure::Panic`。worker 线程启动失败会报告为 `AttemptFailure::Executor`。配置了 attempt timeout 时，retry executor 会在超时后停止等待该 worker，标记本次 attempt 的 token 为 cancelled，并按配置的 `AttemptTimeoutPolicy` 继续处理。

Rust 不能安全地强杀运行中的线程，因此如果 operation 不检查 token 并主动返回，超时后的 worker 可能会继续运行。如果超时策略选择 retry，超时后的 worker 还可能和后续 attempt 并发重叠。阻塞 IO、第三方调用、可能 panic 的代码，或需要单次 attempt 超时隔离的任务适合使用这一路径；低延迟内存操作优先使用普通 `run()`。

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
    .abort_on_timeout()
    .build()?;

let response = retry.run_in_worker(blocking_fetch)?;
```

`run_blocking_with_timeout()` 仍然保留，作为 `run_in_worker()` 的兼容别名。

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

- `before_attempt`：在**每次真正执行用户操作之前**调用（含首次 attempt）。用于记录「即将开始第 N 次」；此时本次 attempt 尚未开始，**不**表示「刚失败、正在等重试」。
- `on_success`：每次 attempt 成功后调用。
- `on_failure`：每次产生 `AttemptFailure` 后调用，并返回 `AttemptFailureDecision`；在**选定**到下一次 attempt 前的等待时间、以及 `on_retry` **之前**执行，可影响退避/中止等决策。
- `on_retry`：在**已确认**会再试、且**到下一次** `before_attempt` **之前**的等待时间已从策略中**算出之后**调用（即晚于与本次失败相关的 `on_failure` 与决策合并）；在 executor **进入 sleep 等待**、以及下一次 `before_attempt` **之前**触发。**只读观察**（不能改变重试/退避）；`RetryContext::next_delay()` 为即将用于 sleep 的等待时长。若不会重试（资源用尽、被中止、已达末次等），**不会**调用 `on_retry`。
- `on_error`：retry 流程返回终止 `RetryError` 时调用一次。

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
            elapsed_ms = context.total_elapsed().as_millis(),
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
config.set("retry.max_elapsed_millis", 30_000u64)?;
config.set("retry.delay", "exponential")?;
config.set("retry.exponential_initial_delay_millis", 200u64)?;
config.set("retry.exponential_max_delay_millis", 5_000u64)?;
config.set("retry.exponential_multiplier", 2.0)?;
config.set("retry.jitter_factor", 0.2)?;
config.set("retry.attempt_timeout_millis", 2_000u64)?;
config.set("retry.attempt_timeout_policy", "retry")?;

let options = RetryOptions::from_config(&config.prefix_view("retry"))?;
let retry = Retry::<std::io::Error>::from_options(options)?;
```

支持的相对配置键：

- `max_attempts`
- `max_elapsed_millis`
- `max_elapsed_unlimited`
- `attempt_timeout_millis`
- `attempt_timeout_policy`：`retry` 或 `abort`
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
        eprintln!("attempts: {}", error.context().attempt());
        eprintln!("elapsed: {:?}", error.context().total_elapsed());

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

