# Qubit Retry

[![Rust CI](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-retry/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-retry/coverage-badge.json)](https://qubit-ltd.github.io/rs-retry/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-retry.svg?color=blue)](https://crates.io/crates/qubit-retry)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Qubit Retry is a Rust library for retrying operations that can fail temporarily,
such as reading from a storage service or reconnecting a client. You decide which
failures are retryable and how long to wait; the library runs the attempts, checks
limits and cancellation, and returns the value or the original error together
with execution details.

## Installation

Requires **Rust 1.94+**. The Cargo package is `qubit-retry`; import it as
`qubit_retry` in Rust. Synchronous execution needs no optional features.

<!-- retry-example: kind=cargo features=none -->
```toml
[dependencies]
qubit-retry = "0.23"
```

| Feature | Adds |
| --- | --- |
| `tokio` | Async execution on a Tokio runtime |
| `worker` | Cooperative cancellation and timeouts for blocking work on dedicated threads |
| `serde` | Serialization and validated deserialization of policy configuration |

All three are opt-in and can be combined. See the [user guide](doc/user_guide.md)
for dependencies and examples for each mode.

## Quick start

A storage service is temporarily unavailable. Retry its read and return the
snapshot when it recovers. The separate `read_snapshot` function simulates two
failed calls followed by success. Its counter only generates test responses;
it does not implement retry control. `RetryConfig` sets the attempt limit,
waiting time, and exponential backoff; the rule classifies retryable errors.

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

// Simulate storage: the first and second calls time out; the third succeeds.
// simulated_calls only produces test responses; it does not limit or schedule retries.
// Replace this function with a real storage read, without the simulation counter.
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

Add the dependency to your application's `Cargo.toml`, put the complete example
in `src/main.rs`, and run `cargo run`:

```text
snapshot-v2, attempts=3
```

The library calls `read_snapshot`, waits 100 ms after its failure, and calls it
again. After the second failure it waits 200 ms; the third call succeeds.
**The fixture chooses responses; the policy controls further attempts and waits.**
`max_attempts(5)` includes the first call and allows at most four retries.
`context.attempts()` is the library's recorded execution count.

Replace `read_snapshot` with your real client call and remove the simulation
counter; no application retry loop or sleep is needed. Set `max_attempts` to 2
and the library returns `Exhausted { limit: Attempts }` after the second failure,
without making the fixture's third call. Unclassified errors abort by default.

## Why use it

A retry loop needs more than a delay: it must distinguish a temporary failure
from an invalid request, stop when limits are reached, and tell callers what
happened. Qubit Retry keeps these decisions in a reusable `Retry` definition.
Each `run` starts with fresh counters and backoff state, so one request does not
consume another request's retry allowance.

| Need | Support |
| --- | --- |
| Retry selected errors | Ordered rules over your own error type |
| Spread repeated requests over time | Fixed, uniform, or exponential backoff with optional jitter and server delay hints |
| Bound retries and support shutdown | Attempt limits, elapsed budgets, cancellation tokens, and async/worker timeouts |
| Diagnose the final outcome | Stop reason, last failure, attempt count, elapsed time, and completion callback diagnostics |
| Keep an existing reconnect loop | Use `BackoffState` and `RetryBudget` independently |

## Choose an execution mode

| Mode | Use it for | Boundary |
| --- | --- | --- |
| `sync()` | A bounded operation on the calling thread | Cannot interrupt the operation |
| `tokio()` | An async client on Tokio | Cancels a pending future; cannot interrupt blocking code inside it |
| `worker()` | Blocking work that checks a cancellation token | Requests thread exit and waits for cleanup; cannot forcibly terminate a thread |

Elapsed budgets control whether another attempt may start. They do not impose
a deadline on an already running synchronous call. Cancellation also cannot undo
an external write; operations with side effects need an application-level retry
strategy, such as idempotency keys. The library provides no HTTP client, circuit
breaker, or worker pool.

## Learn more

- [User guide](doc/user_guide.md) · [中文用户手册](doc/user_guide.zh_CN.md): complete workflows, timeouts, error handling, and configuration
- [Rust API documentation](https://docs.rs/qubit-retry/0.23.0/qubit_retry/): public types and methods
- [Design](doc/design.md): execution ordering and internal contracts
- [中文 README](README.zh_CN.md)

## Testing

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
./ci-check.sh

# Check code coverage
./coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./align-ci.sh` to format code and
`./ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-retry](https://github.com/qubit-ltd/rs-retry)
