/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Attempt timeout policy.
//!
//! Author: Haixing Hu

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Action taken when one attempt exceeds its configured timeout.
///
/// The policy is used as the default decision for
/// [`crate::AttemptFailure::Timeout`]. Explicit failure listeners can still
/// return their own decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptTimeoutPolicy {
    /// Retry timed-out attempts while normal retry limits allow it.
    Retry,
    /// Abort the retry flow immediately when an attempt times out.
    Abort,
}

impl Default for AttemptTimeoutPolicy {
    /// Creates the default attempt-timeout policy.
    ///
    /// # Returns
    /// [`AttemptTimeoutPolicy::Retry`].
    #[inline]
    fn default() -> Self {
        Self::Retry
    }
}

impl fmt::Display for AttemptTimeoutPolicy {
    /// Formats the policy as lower-case config text.
    ///
    /// # Parameters
    /// - `f`: Formatter provided by the standard formatting machinery.
    ///
    /// # Returns
    /// Formatter result.
    ///
    /// # Errors
    /// Returns a formatting error only if the formatter rejects output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retry => f.write_str("retry"),
            Self::Abort => f.write_str("abort"),
        }
    }
}

impl FromStr for AttemptTimeoutPolicy {
    /// Error returned when policy text is unsupported.
    type Err = String;

    /// Parses a timeout policy from config text.
    ///
    /// # Parameters
    /// - `s`: Policy text. ASCII case is ignored.
    ///
    /// # Returns
    /// Parsed [`AttemptTimeoutPolicy`].
    ///
    /// # Errors
    /// Returns an error message when `s` is not `retry` or `abort`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "retry" => Ok(Self::Retry),
            "abort" => Ok(Self::Abort),
            _ => Err("attempt timeout policy must be `retry` or `abort`".to_string()),
        }
    }
}
