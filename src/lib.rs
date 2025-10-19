/*******************************************************************************
 *
 *    Copyright (c) 2025.
 *    3-Prism Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! # Prism3 Retry - Retry module
//!
//! A fully-featured, type-safe retry management system, ported from Java's `ltd.qubit.commons.util.retry` package to Rust.
//!
//! ## Design Philosophy
//!
//! ### Core Design Principles
//!
//! 1. **Type Safety First** - Leverage Rust's type system to ensure type safety at compile time
//! 2. **Result Error Handling** - Use Rust's Result type for error handling instead of exceptions
//! 3. **Zero-Cost Abstraction** - Use enums instead of trait objects to avoid dynamic dispatch overhead
//! 4. **Unified Interface** - Provide generic APIs that support all primitive types and custom types
//! 5. **Event-Driven** - Support various event listeners during the retry process
//!
//! ## Module Structure
//!
//! ```text
//! retry/
//! |-- mod.rs              # Module entry point, exports public API
//! |-- builder.rs           # RetryBuilder struct (core retry builder)
//! |-- config.rs            # RetryConfig trait and DefaultRetryConfig
//! |-- delay_strategy.rs    # RetryDelayStrategy enum
//! |-- events.rs            # Event type definitions
//! |-- error.rs             # Error type definitions
//! |-- executor.rs          # RetryExecutor executor
//! ```
//!
//! ## Core Features
//!
//! - ✅ **Type-Safe Retry** - Use generic API to support any return type
//! - ✅ **Multiple Delay Strategies** - Support fixed delay, random delay, exponential backoff
//! - ✅ **Flexible Error Handling** - Result-based error handling with error type identification
//! - ✅ **Result-Driven Retry** - Support retry logic based on return values
//! - ✅ **Event Listening** - Support various event callbacks during retry process
//! - ✅ **Configuration Integration** - Seamless integration with prism3-config's config module
//! - ✅ **Timeout Control** - Support single operation timeout and overall timeout control
//! - ✅ **Sync and Async** - Support both synchronous and asynchronous operation retries
//!
//! ## Usage Examples
//!
//! ### Basic Synchronous Retry
//! ```rust
//! use prism3_retry::{RetryBuilder, RetryDelayStrategy, RetryResult, RetryEvent};
//! use std::time::Duration;
//!
//! // Basic retry configuration
//! let executor = RetryBuilder::new()
//!     .set_max_attempts(3)
//!     .set_delay_strategy(RetryDelayStrategy::Fixed { delay: Duration::from_secs(1) })
//!     .failed_on_results(vec!["RETRY".to_string(), "TEMP_FAIL".to_string()])
//!     .on_retry(|event: &RetryEvent<String>| println!("Retry attempt {}", event.attempt_count()))
//!     .build();
//!
//! // 使用 RetryResult 类型别名简化返回类型
//! let result: RetryResult<String> = executor.run(|| -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
//!     // 可以使用 ? 操作符，io::Error 会自动通过 From trait 转换为 RetryError
//!     // 例如: std::fs::read_to_string("config.txt")?;
//!     Ok("SUCCESS".to_string())
//! });
//!
//! assert!(result.is_ok());
//! ```
//!
//! ### Async Retry with Timeout Control
//! ```rust,no_run
//! use prism3_retry::{RetryBuilder, RetryResult};
//! use std::time::Duration;
//!
//! # async fn example() {
//! // Configure timeout control
//! let executor = RetryBuilder::<String>::new()
//!     .set_max_attempts(3)
//!     .set_operation_timeout(Some(Duration::from_secs(5)))  // Max 5 seconds per operation
//!     .set_max_duration(Some(Duration::from_secs(30)))      // Max 30 seconds total
//!     .build();
//!
//! // 使用 RetryResult 简化异步函数的返回类型
//! let result: RetryResult<String> = executor.run_async(|| async {
//!     // 异步操作超时会被真正中断
//!     // 可以使用 ? 操作符，错误会自动转换
//!     // 例如: tokio::fs::read_to_string("config.txt").await?;
//!     Ok("SUCCESS".to_string())
//! }).await;
//!
//! assert!(result.is_ok());
//! # }
//! ```
//!
//! For detailed usage, please refer to the documentation of each struct.
//!
//! ## Author
//!
//! Haixing Hu

pub mod builder;
pub mod config;
pub mod default_config;
pub mod delay_strategy;
pub mod error;
pub mod event;
pub mod executor;
pub mod simple_config;

// Re-export public API
pub use builder::RetryBuilder;
pub use config::RetryConfig;
pub use default_config::DefaultRetryConfig;
pub use delay_strategy::RetryDelayStrategy;
pub use error::RetryError;
pub use event::{
    AbortEvent, AbortReason, FailureEvent, RetryDecision, RetryEvent, RetryReason, SuccessEvent,
};
pub use executor::RetryExecutor;
pub use simple_config::SimpleRetryConfig;

// Common type aliases
pub type RetryResult<T> = Result<T, RetryError>;

/// Type alias for retry builder using default configuration
///
/// This is the most commonly used `RetryBuilder` type, using `DefaultRetryConfig` as the configuration type.
///
/// # Example
///
/// ```rust
/// use prism3_retry::DefaultRetryBuilder;
///
/// let builder = DefaultRetryBuilder::<String>::new()
///     .set_max_attempts(3)
///     .build();
/// ```
pub type DefaultRetryBuilder<T> = RetryBuilder<T, DefaultRetryConfig>;

/// Type alias for retry executor using default configuration
///
/// This is the most commonly used `RetryExecutor` type, using `DefaultRetryConfig` as the configuration type.
///
/// # Example
///
/// ```rust
/// use prism3_retry::{RetryBuilder, DefaultRetryExecutor};
///
/// let executor: DefaultRetryExecutor<String> = RetryBuilder::new()
///     .set_max_attempts(3)
///     .build();
/// ```
pub type DefaultRetryExecutor<T> = RetryExecutor<T, DefaultRetryConfig>;
