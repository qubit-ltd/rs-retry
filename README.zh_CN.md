# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

Qubit Retry 为 Rust 客户端、存储操作和重连循环提供统一的类型安全重试策略。
它同时保留原始业务错误、流程停止原因，以及完成回调产生的附加诊断。

## 解决什么问题

读取存储快照时，瞬时超时可以重试，永久性错误应及时停止，调用方还需要知道实际执行了多少次操作。
手写循环容易把这些判断与关闭信号、时间计量混在一起。`RetryPolicy` 将经过验证的预算和退避配置
与执行过程分开；每次 `run` 都有独立状态。克隆 `Retry` 会共享规则和观察者，不要求 `E: Clone`。

## 安装

需要 Rust 1.94 或更新版本，默认 feature 集为空。

<!-- retry-example: kind=cargo features=none -->
```toml
[dependencies]
qubit-retry = "0.23"
```

Tokio 执行和配置序列化需要显式开启：

<!-- retry-example: kind=cargo features=tokio,serde -->
```toml
[dependencies]
qubit-retry = { version = "0.23", features = ["tokio", "serde"] }
```

## 快速开始

下面用确定性的存储读取模拟第一次超时、第二次成功，可以把闭包替换为实际客户端的有界调用。
十秒限制是**控制后续尝试准入的软预算**，既不能打断同步读取，也不会撤销已准入操作的成功。
辅助函数直接返回原始的类型化重试错误。示例使用立即退避以便快速验证；真实远程请求可采用指南中的抖动策略。

<!-- retry-example: kind=run features=none -->
```rust
use std::io;
use std::time::Duration;

use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryError;
use qubit_retry::RetryPolicy;
use qubit_retry::RetrySuccess;

fn fetch_snapshot(retry: &Retry<io::Error>) -> Result<RetrySuccess<Vec<u8>>, RetryError<io::Error>> {
    let mut calls = 0;
    retry.sync().run(|| {
        calls += 1;
        if calls == 1 {
            Err(io::Error::new(io::ErrorKind::TimedOut, "temporary read failure"))
        } else {
            Ok(b"snapshot-v2".to_vec())
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let policy = RetryPolicy::builder()
        .max_attempts(4)
        .total_time_budget(Duration::from_secs(10))
        .backoff(BackoffPolicy::immediate())
        .build()?;
    let retry = Retry::builder(policy)
        .rule(|failure: &AttemptFailure<io::Error>, _: &RetryContext| match failure {
            AttemptFailure::Error(error) if error.kind() == io::ErrorKind::TimedOut => RetryDecision::Retry,
            _ => RetryDecision::Abort,
        })
        .build();
    let (snapshot, context, diagnostics) = fetch_snapshot(&retry)?.into_parts();
    assert_eq!(snapshot, b"snapshot-v2");
    assert_eq!(context.attempts(), 2);
    assert!(diagnostics.is_empty());
    Ok(())
}
```

## 执行模式与限制

| 模式 | 操作与资源 | 超时及取消边界 |
| --- | --- | --- |
| `sync()` | 在调用线程执行 `FnMut` | 准入前、失败后和退避等待中检查取消；不支持操作硬超时 |
| `asynchronous()` | 支持非 `Send`、非静态生命周期 future；需要 `tokio` | 协作式单次/流程计时器可丢弃待完成 future，无法抢占阻塞的 poll |
| `worker()` | 需要启用 `worker` feature；操作须为 `Send + 'static`；每次尝试创建 worker 和 reaper | 请求协作退出，在真实时间清理宽限期内等待 join，包括 TLS 析构 |

默认允许**总共三次尝试**，采用立即退避，不设耗时预算。未被规则处理的业务错误默认终止；需要“全部重试”时请显式使用 `RetryFallback::Retry`。
捕获的单次超时和 worker panic 默认终止，但规则可在剩余预算内请求重试。
规则按注册顺序执行，第一个非 `UseDefault` 决策生效；流程超时、取消、控制回调失败和基础设施失败不能由规则恢复。

`operation_time_budget` 累计已准入操作的耗时，`total_time_budget` 还包含控制回调与退避。
两者都不是硬超时。完成计量有效时，sync 的成功优先于取消；async 同时就绪时先取操作结果，再检查取消和超时；
worker 则先检查取消、再检查超时，最后接受操作结果和线程已退出的确认。取消令牌的克隆共享永久取消状态，不支持重置或父子树。

worker 清理宽限期结束后仍未退出，会返回带触发原因的 `WorkerStillRunning`，不再启动下一次尝试。
不配合的操作或 TLS 析构可能让 worker、reaper 两个线程继续存活；没有超时和取消时，退出等待可能没有上限。

## 错误、提示与完成诊断

`RetryErrorReason` 区分 `Aborted`、`Exhausted`、`TimedOut`、`Cancelled`、`CallbackFailed` 和 `Infrastructure`；
`AttemptFailure` 保留业务错误、超时范围或已捕获的 panic。只有 worker 捕获操作 panic，sync/async 操作 panic
会向调用者或轮询任务传播。

完成观察者在结果冻结后同步执行，每个正常返回的结果通知一次，包括准入前失败。完成回调 panic 只附加有序诊断，
不会替换结果，后续完成观察者仍会执行。完成耗时不计入上下文，也不受重试超时控制。
操作展开栈、异步 run future 被丢弃或进程中止时，不保证完成通知。

`into_parts()` 同时保留值或失败、上下文和诊断。需要保留失败信息时应使用完整拆解结果，当前没有有损的 `into_failure()` 别名。
`map_error` 只转换已保留的业务错误，`FnOnce` 映射函数调用零次或一次，
不增加 `Clone`、`Send` 或 `'static` 约束。显式命名的 `into_value_discarding_diagnostics()` 和
`into_value_discarding_diagnostics()` 会丢弃上下文和诊断。当前没有
`into_failure_discarding_diagnostics()`；需要保留失败时应使用完整的四元素错误拆解结果。
有损转换只应在明确不需要被丢弃信息的出口使用。

`BackoffState` 可以独立用于 SSE 或其他重连循环。立即、固定、均匀和指数退避支持 full/bounded 抖动及服务端提示。
`maximum_delay()` 只描述基础策略，`limit_delay()` 则在提示和抖动之后限制最终延迟，**可能截短服务端要求的最小等待**。
必须遵守该最小等待时，不应设置更小的最终上限；应停止重试或调整预算。
`serde` 只用于配置，不将运行时错误、结果或回调作为序列化协议。

## 升级到 0.23

- 同步升级直接依赖、适配层及锁文件。`into_parts()` 现在返回原因、最后一次失败、上下文与 `Box<[RetryCallbackFailure]>` 诊断。
- 用 `RetryErrorReason` 和独立的 `last_failure` 替换 `RetryFailure<E>`；适配器可用 `into_metadata_and_error()` 分离业务错误和终止元数据。
- `RetryLimits`/`limits()` 改为 `RetryAdmissionLimits`/`admission_limits()`；时间预算改为 `operation_time_budget`/`total_time_budget`。
- `attempt_timeout`/`flow_timeout` 改为 `hard_attempt_timeout`/`hard_flow_timeout`，worker 执行必须显式开启 `worker` feature。
- 只有明确允许丢失诊断时，才把 `into_value()` / `into_failure()` 改为显式丢弃名称；转换结果优先完整拆解。
- 控制回调正常返回后，先刷新时钟，再检查取消或处理返回的决策。异常时钟返回 `Infrastructure::Clock`；
  回调 panic 仍保留 `CallbackFailed`，仅尽力刷新计量。取消快照现在包含控制回调耗时。
- 均匀退避端点精确落在区间内。worker 与回调的非字符串 panic 载荷即使在析构时再次 panic，也保留 `NonString`；
  只遗忘异常路径上的二次载荷，以防递归析构。
- HTTP 的重试错误直接 source 改为完整 `RetryError<HttpError>`；CAS 的 getter 和消费拆解保留完成诊断；
  EventBus 仅在诊断非空时包装领域错误，默认规则将该包装视为终止错误。
  内置适配层成功路径当前不注册完成观察者，因此显式丢弃空诊断。

原有准入契约保持：`RetryBudget` 必须完成当前 token 才能再次准入，token 不能跨预算使用。
`on_before_attempt` 在准入前执行，`on_retry_scheduled` 不保证下一次操作一定开始。
从更早版本升级时，还应使用 `on_before_attempt` / `BeforeAttempt`，并处理 `Success` / `TerminalFailure` 完成阶段。

## 延伸阅读

- [用户指南](doc/user_guide.zh_CN.md)：取消、硬超时、提示、serde、时钟、诊断及排障
- [设计文档](doc/design.zh_CN.md)：准入、优先级、所有权和 worker 协议
- [Rust API 文档](https://docs.rs/qubit-retry)
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
