/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Configuration keys and built-in defaults for retry options.
//!
//! The `KEY_*` strings are the **relative configuration keys** for each retry
//! option's stored value: under a [`qubit_config::ConfigReader`] prefix (for example
//! `retry.`), they name the property whose value is read when building
//! [`crate::RetryOptions`] (see [`crate::RetryOptions::from_config`] and
//! [`crate::RetryConfigValues`]). They are not
//! delay/jitter strategy tokens themselves; those live in the option value (for
//! example the string behind [`KEY_DELAY`]).
//!
//! **Default constants here are the source of truth.** Each type's
//! [`std::default::Default`] implementation should assign from these values (for
//! example [`crate::RetryDelay::default`] parses [`DEFAULT_RETRY_DELAY`] and
//! [`crate::RetryJitter::default`] parses [`DEFAULT_RETRY_JITTER`] via
//! [`std::str::FromStr`]), rather than the reverse. This module
//! avoids depending on option types such as [`crate::RetryJitter`] so there is no
//! cycle with their `Default` impls. Composed defaults such as
//! [`crate::RetryOptions::default`] should prefer delegating to those `Default`
//! impls together with the scalar defaults declared here.
//!
//! Author: Haixing Hu

use std::time::Duration;

// ------------------------------------------------------------------------- keys

/// Config key for the maximum attempts option value (including the initial attempt).
pub const KEY_MAX_ATTEMPTS: &str = "max_attempts";

/// Config key for the maximum elapsed budget option value, in milliseconds. When
/// absent, the merge uses `default.max_elapsed`. A stored value of `0` means
/// unlimited (`None`).
pub const KEY_MAX_ELAPSED_MILLIS: &str = "max_elapsed_millis";

/// Config key for the delay strategy option value (strategy name / encoded form).
pub const KEY_DELAY: &str = "delay";

/// Config key: backward-compatible alias for the delay strategy option value (same
/// meaning as [`KEY_DELAY`]).
pub const KEY_DELAY_STRATEGY: &str = "delay_strategy";

/// Config key for the fixed delay option value, in milliseconds.
pub const KEY_FIXED_DELAY_MILLIS: &str = "fixed_delay_millis";

/// Config key for the random delay minimum option value, in milliseconds.
pub const KEY_RANDOM_MIN_DELAY_MILLIS: &str = "random_min_delay_millis";

/// Config key for the random delay maximum option value, in milliseconds.
pub const KEY_RANDOM_MAX_DELAY_MILLIS: &str = "random_max_delay_millis";

/// Config key for the exponential backoff initial delay option value, in milliseconds.
pub const KEY_EXPONENTIAL_INITIAL_DELAY_MILLIS: &str = "exponential_initial_delay_millis";

/// Config key for the exponential backoff maximum delay option value, in milliseconds.
pub const KEY_EXPONENTIAL_MAX_DELAY_MILLIS: &str = "exponential_max_delay_millis";

/// Config key for the exponential backoff multiplier option value.
pub const KEY_EXPONENTIAL_MULTIPLIER: &str = "exponential_multiplier";

/// Config key for the jitter factor option value (numeric factor, not the `none` /
/// `factor:` text form used by [`DEFAULT_RETRY_JITTER`]).
pub const KEY_JITTER_FACTOR: &str = "jitter_factor";

// --------------------------------------------------------------------- defaults

/// Default maximum attempts (including the initial attempt) for
/// [`crate::RetryOptions::default`].
pub const DEFAULT_RETRY_MAX_ATTEMPTS: u32 = 3;

/// Default total elapsed-time budget for [`crate::RetryOptions::default`]:
/// unlimited (`None`).
pub const DEFAULT_RETRY_MAX_ELAPSED: Option<Duration> = None;

/// Default delay text for [`crate::RetryDelay::default`] and any code that should
/// match the library's built-in delay default.
///
/// Parsed with [`std::str::FromStr`] as implemented for [`crate::RetryDelay`].
/// Grammar (same as that type's `Display` / `from_str` contract):
///
/// - `none`
/// - `fixed(<millis>ms)`
/// - `random(<min_millis>ms..=<max_millis>ms)`
/// - `exponential(initial=<millis>ms, max=<millis>ms, multiplier=<f64>)`
///
/// Invalid text makes [`crate::RetryDelay::default`] panic at runtime; keep this
/// constant in sync with [`crate::RetryDelay`] parsing rules when you change it.
pub const DEFAULT_RETRY_DELAY: &str = "exponential(initial=1000ms, max=60000ms, multiplier=2.0)";

/// Default jitter text for [`crate::RetryJitter::default`] and any code that should
/// match the library's built-in jitter default.
///
/// Parsed with [`std::str::FromStr`] as implemented for [`crate::RetryJitter`].
/// Grammar (same as that type's `Display` / `from_str` contract):
///
/// - **No jitter:** use `none` in any ASCII letter case, for example `"none"` or
///   `"NONE"` ([`crate::RetryJitter::None`]).
/// - **Factor jitter:** use `factor:` immediately followed by a floating-point
///   literal in the inclusive range **`0.0` … `1.0`** (optional ASCII space after the
///   colon). Examples: `"factor:0.25"`, `"factor: 0.5"`. This selects
///   [`crate::RetryJitter::Factor`] with that coefficient.
///
/// Invalid text or an out-of-range factor makes [`crate::RetryJitter::default`]
/// panic at runtime; keep this constant in sync with [`crate::RetryJitter::validate`]
/// rules when you change it.
pub const DEFAULT_RETRY_JITTER: &str = "none";
