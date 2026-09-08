# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

Qubit Retry 是一个 Rust 重试库，适用于存储读取、客户端重连等可能暂时失败的操作。
你负责判断哪些错误可以重试、每次间隔多久；库负责执行尝试、检查限制和取消信号，
并在结束时返回业务结果或原始错误，以及次数、耗时等执行信息。

## 安装

需要 **Rust 1.94 或更新版本**。Cargo 包名为 `qubit-retry`，Rust 代码中使用
`qubit_retry` 导入。同步执行无需开启任何可选 feature。

<!-- retry-example: kind=cargo features=none -->
```toml
[dependencies]
qubit-retry = "0.23"
```

| Feature | 提供的能力 |
| --- | --- |
| `async` | 使用 `StdTimer`、不绑定具体运行时的异步执行 |
| `tokio` | 使用 `TokioTimer` 的 Tokio 异步执行（同时开启 `async`） |
| `worker` | 在独立线程执行阻塞操作，支持协作式取消和超时 |
| `serde` | 序列化策略配置，并在反序列化时校验配置 |

这些 feature 默认均不开启，可以按需组合。各模式的依赖配置与示例见
[用户手册](doc/user_guide.zh_CN.md)。

## 快速开始

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

## 为什么需要它

重试循环除了等待，还要判断错误能否恢复、何时停止，并向调用方交代失败原因。
Qubit Retry 将这些判断集中到可复用的 `Retry` 中。每次 `run` 都从新的计数器和退避状态开始，
不同请求不会相互占用重试次数。

| 需求 | 对应能力 |
| --- | --- |
| 只重试特定错误 | 按顺序执行规则，直接判断业务错误类型 |
| 避免连续请求失败的服务 | 固定、均匀随机或指数退避，可叠加抖动和服务端等待提示 |
| 限制尝试并响应应用关闭 | 次数限制、耗时预算、取消令牌，以及 async/worker 超时 |
| 查明最终发生了什么 | 终止原因、最后一次失败、次数、耗时和完成回调诊断 |
| 保留已有重连循环 | 单独使用 `BackoffState` 和 `RetryBudget` |

## 选择执行模式

| 模式 | 适用场景 | 使用边界 |
| --- | --- | --- |
| `Retry::new(&config)` | 在当前线程执行耗时可控的操作 | 无法打断正在执行的操作 |
| `AsyncRetry::new(&config)` | 在任意 executor 中运行异步代码 | 使用 `StdTimer`，调用方仍需 poll 返回的 future |
| `TokioRetry::new(&config)` | 调用 Tokio 异步客户端 | 可丢弃尚未完成的 future，无法打断其中的阻塞代码 |
| `WorkerRetry::new(&config)` | 阻塞操作能够检查取消令牌并退出 | 请求线程退出并等待清理，不能强制终止线程 |

耗时预算决定能否开始下一次尝试，不能为正在执行的同步调用强制设定截止时间。
取消也无法撤销已经发生的外部写入；重试有副作用的操作时，需要由业务保证幂等或使用其他恢复方案。
本库不提供 HTTP 客户端、熔断器或线程池。

## 延伸阅读

- [中文用户手册](doc/user_guide.zh_CN.md) · [English user guide](doc/user_guide.md)：完整用法、超时、错误处理、配置
- [Rust API 文档](https://docs.rs/qubit-retry/0.23.0/qubit_retry/)：公开类型与方法
- [设计文档](doc/design.zh_CN.md)：执行顺序与内部契约
- [English README](README.md)

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
