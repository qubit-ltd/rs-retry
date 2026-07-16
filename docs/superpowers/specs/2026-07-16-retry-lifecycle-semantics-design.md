# Retry 生命周期语义与可维护性改进设计

## 背景

`qubit-retry` 同时支持同线程、异步和 worker 三种执行模式。当前三个 runner
重复实现了预算检查、attempt 编号、监听器通知、耗时记录和终止错误构造，导致相同
生命周期语义分散在多个分支中。现有实现还存在两处可观察语义不一致：

1. async/worker 的在途 attempt 被 elapsed budget 截止时，会直接返回
   `RetryError`，没有调用文档承诺的 `on_failure`。
2. `before_attempt` 运行前已经递增 attempt 计数；如果监听器耗尽总预算，业务
   operation 从未进入执行阶段，`RetryError::attempts()` 仍返回 1。

下游使用进一步放大了这些问题：`rs-cas` 依赖 timeout failure 记录报告和事件，
而 `rs-http` 的 failure listener 假定只会收到 `AttemptFailure::Error`。

## 目标

- 让每一个 attempt 执行结果产生的 `AttemptFailure` 都被 `on_failure` 精确观察
  一次。
- 保证 elapsed budget 是不可由监听器覆盖的硬终止边界。
- 区分“即将执行的 attempt 序号”和“已进入执行阶段的 attempt 数”。
- 集中三种 runner 的公共生命周期，降低重复和分支复杂度。
- 补齐同步、异步、worker 以及实际下游的回归测试。
- 修复 `rs-retry` 范围内已确认的 Rust 文件组织、Rustdoc、方法顺序和 inline
  属性技术债务。

## 非目标

- 不改变现有公开类型、方法名称或模块导出路径。
- 不增加新的公开 listener 或 retry policy API。
- 不处理闭包装箱、动态分发或其他性能路径；该议题留待后续单独讨论。
- 不改变同线程执行无法中断运行中闭包的既有约束。
- 不用虚拟时钟替换 worker executor 基于标准库 channel 的真实超时机制。

## 生命周期语义

### Attempt 阶段

一次 attempt 经过以下阶段：

1. 检查已消耗的 operation/total elapsed budget。
2. 计算即将执行的 attempt 序号 `executed_attempts + 1`，构造
   `before_attempt` context。
3. 调用 `before_attempt`。
4. 重新计算有效 timeout 并检查监听器消耗后的 budget。
5. 将 attempt 提交为“已进入执行阶段”。
6. 调用同线程 operation、创建并轮询 async operation，或进入 worker executor。
7. 记录 attempt elapsed 和累计 operation elapsed。
8. 通知 success，或观察 failure 后执行 retry/terminal 决策。

因此：

- `before_attempt` 继续收到从 1 开始的即将执行序号。
- `RetryError::attempts()` 和 terminal context 的 attempt 表示已经通过步骤 4、
  进入步骤 5 的次数。
- 第一次 operation 前由 `before_attempt` 耗尽预算时，attempts 为 0。
- 第 N 次 operation 前耗尽预算时，attempts 保持 N-1，并保留上一失败（如有）。
- worker 已通过前置检查并进入 executor 后，即使线程创建失败，也计为一次
  attempt；否则允许 retry executor failure 时可能无法受 max-attempts 约束。

### Hard elapsed timeout

async/worker 的有效 attempt timeout 可能来自配置的 attempt timeout、剩余
max-operation-elapsed 或剩余 max-total-elapsed。配置 timeout 仍由普通 failure
policy 决定；elapsed budget timeout 使用以下固定顺序：

1. 构造带有正确 `AttemptTimeoutSource` 的 failure context。
2. 运行 retry-after hint extractor，使 failure listener 仍看到与普通 failure
   相同阶段的 context。
3. 按注册顺序调用所有 `on_failure` listener，确保每个 listener 精确观察一次。
4. 忽略 listener 返回的 `Retry`、`RetryAfter`、`Abort` 或 `UseDefault`。
5. 返回原始硬终止原因：`MaxOperationElapsedExceeded` 或
   `MaxTotalElapsedExceeded`。
6. 不选择重试延迟，不调用 `on_retry`；最终仍调用一次 `on_error`。

监听器耗时计入 total elapsed，terminal context 在监听器结束后刷新
`total_elapsed`。若监听器 panic，则继续服从既有 `ListenerPanicPolicy`，本设计不
改变 panic 隔离语义。

这里的 failure 指 operation、async timeout 或 worker executor 已进入 attempt
执行阶段后产生的结果。retry sleeper 失败、同线程模式不支持配置 attempt timeout
等发生在 attempt 之外的 terminal executor diagnostic 仍只进入 `on_error`，不伪装
成一次 attempt failure notification。

## 内部结构

### `executor/internal/attempt_lifecycle.rs`

新增私有内部模块，集中 runner 共有的阶段转换：

- 同线程 attempt 的预算预检、before listener、预算复检和提交。
- 可中断 attempt 的有效 timeout 计算、预算预检、before listener、重算、
  复检和提交。
- attempt 完成后的 elapsed 记录与 `RetryContext` 构造。

模块只接受 `RetryFlowState`、`RetryOptions`、`RetryEvents` 和现有 timeout 类型，
不公开新 API，不持有 operation，也不抽象三种执行机制。

### `RetryFlowState`

状态只保存已提交 attempt 数、累计 operation elapsed、起始时间和上一失败。
它提供“下一 attempt 序号”“按指定 attempt 构造 context”“提交下一 attempt”等
窄接口；不直接调用用户 operation。

### `RetryFailureHandler`

将 failure observation（hint 提取、failure listener 调用、context 刷新）与后续
策略处理分成内部阶段。普通失败沿用现有决策、限制检查、delay 和 `on_retry`
顺序；hard elapsed timeout 复用 observation 阶段后直接构造硬终止错误。

### 三个 runner

- `RetryRunner` 只负责同线程 operation 调用和 blocking sleep。
- `AsyncRetryRunner` 只负责 async operation 与 timeout future 的选择和 async
  sleep。
- `WorkerRetryRunner` 只负责 worker outcome、未回收 worker 安全边界和 blocking
  sleep。

不使用宏、async trait 或统一 operation 类型，以免把第 6 项性能/类型擦除议题
混入本次修改。

## 下游兼容

### `rs-http`

`HttpClient::retry_failure_decision` 改为显式处理 `AttemptFailure`：

- `AttemptFailure::Error(error)` 继续执行 HTTP retryability 和 delay 逻辑。
- `Timeout`、`Panic`、`Executor` 返回 `UseDefault`，不 panic。

这样 max-duration 截止在途 HTTP future 时，`qubit-retry` 可以通知 listener，
之后仍由现有 `map_retry_error` 映射为 HTTP timeout。

### `rs-cas`

现有 listener 已处理 `AttemptFailure::Timeout`，无需生产代码调整。新增回归测试，
证明 elapsed-budget 在途 timeout 会增加 timeout 报告并发送对应 attempt-failed
事件，同时不会发送 retry-scheduled 行为。

## 测试设计

### TDD 回归测试

先添加并观察以下测试在旧实现上按预期失败：

- async max-operation/max-total 在途 timeout 调用 `on_failure` 一次，保留硬终止
  原因，并且不调用 `on_retry`。
- worker 对应 timeout 具有相同通知语义。
- sync、async、worker 在第一次 `before_attempt` 耗尽总预算时，listener 看到
  attempt 1，operation 未执行，terminal attempts 为 0。
- 第二次 `before_attempt` 耗尽预算时，terminal attempts 为 1 且保留第一次失败。
- `rs-http` 的在途 max-duration timeout 不因 failure listener panic。
- `rs-cas` 能观察 elapsed-budget timeout 并更新报告/事件。

### 确定性

- listener 消耗时间和 async deadline 使用 `qubit-clock` 手动 monotonic clock、
  manual blocking/async sleeper 驱动，不用 wall-clock sleep 断言正确性。
- worker timeout 仍使用标准库真实 deadline，但 operation 用 cancellation token、
  channel 或 barrier 协调，移除用于“碰运气等待”的长 sleep。
- wall-clock 上限只作为防死锁保护，不作为业务语义断言。

### 测试文件组织

将 runner 和 failure-handler 行为按生产源码责任放入镜像命名测试文件：

- `tests/executor/retry_runner_tests.rs`
- `tests/executor/async_retry_runner_tests.rs`
- `tests/executor/worker_retry_runner_tests.rs`
- `tests/executor/retry_failure_handler_tests.rs`
- `tests/executor/worker_attempt_executor_tests.rs`

迁移时只改变组织和确定性设施，不顺带改变无关断言。测试继续通过
`tests/executor/mod.rs` 注册，不在生产文件中增加 inline tests，也不为测试扩大
生产可见性。

## 代码风格与可读性

在 `rs-retry/src` 和对应外部测试范围进行完整复核并修正：

- 每个 struct/enum/trait 独立文件；`RetryJitterFactorFormat` 移入
  `options/internal/retry_jitter_factor_format.rs`，保留既有序列化行为。
- 每个新文件复用仓库完整版权头。
- 将 `# Parameters` 统一为 `# Arguments`，删除“无参数”“不会返回错误”等无信息
  模板，同时保留并补全实际 Arguments、Returns、Errors、Panics 和副作用约束。
- inherent impl 按构造器、可见性、同组功能邻接排序；getter 位于 setter 之前。
- getter、setter 和纯转发使用 `#[inline(always)]`；其他合适的短函数使用
  `#[inline]`；复杂控制流不强制 inline。
- 只拆分与本次生命周期复杂度或已确认一文件多类型问题直接相关的实现，不改变
  公共 API，也不进行性能改写。

## 验证

每个行为变更遵循 red-green-refactor：先运行单个回归测试确认旧实现失败，再做
最小生产修复并确认通过。完成所有修改后，每个受影响仓库分别检查 diff 和状态。

`rs-retry` 按仓库规定顺序运行：

1. `./align-ci.sh`
2. `./ci-check.sh`
3. 仅当 CI 报告覆盖率低于阈值时运行 `./coverage.sh json`

`rs-http` 和 `rs-cas` 同样从各自仓库运行现有 alignment/CI 脚本；若脚本不存在，
只使用仓库明确提供的等价命令。任何失败先判断是否由本次改动引入，只修复授权
范围内的问题。

## 风险与约束

- `on_failure` 将开始看到此前被遗漏的 elapsed-budget timeout。这是有意修复，
  但所有已知下游必须同时验证对非业务错误的穷尽处理。
- attempt 计数在“operation 从未进入执行阶段”的终止路径上由 1 变为 0；这是
  文档语义修复，也是可观察行为变更，需在 README 和 Rustdoc 中明确。
- worker 的真实线程超时仍可能受调度噪声影响，因此测试只验证状态和事件，不验证
  精确 wall-clock 时长。
- 本次修改不执行 `git add`、`git commit` 或 `git push`；三个仓库的修改保持彼此
  独立，供用户自行审阅和提交。
