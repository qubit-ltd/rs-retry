// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Inputs to one backoff calculation.

use std::time::Duration;

/// Optional caller-provided delay information for one scheduled retry.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use qubit_retry::BackoffPolicy;
/// use qubit_retry::BackoffRequest;
///
/// let mut state = BackoffPolicy::fixed(Duration::from_millis(10)).start();
/// let step = state.next(BackoffRequest::hint(Duration::from_millis(20)));
/// assert_eq!(step.effective_delay(), Duration::from_millis(20));
/// ```
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffRequest {
    /// Optional caller delay; absence delegates entirely to the policy.
    pub(crate) hint: Option<Duration>,
    /// Whether the caller permits jittering the hint itself.
    pub(crate) jitter_hint: bool,
}

impl BackoffRequest {
    /// Uses only the policy strategy.
    ///
    /// # Returns
    /// A request without a hint; all selection is delegated to the policy.
    #[inline]
    pub fn policy() -> Self {
        Self {
            hint: None,
            jitter_hint: false,
        }
    }

    /// Supplies a server or application delay hint.
    ///
    /// # Parameters
    /// - `delay`: Server/application hint that must not itself be jittered.
    ///
    /// # Returns
    /// A hint request whose precedence and final cap are decided by the policy.
    #[inline]
    pub fn hint(delay: Duration) -> Self {
        Self {
            hint: Some(delay),
            ..Self::policy()
        }
    }

    /// Supplies a hint that should also receive the policy jitter.
    ///
    /// # Parameters
    /// - `delay`: Caller hint explicitly allowing configured jitter.
    ///
    /// # Returns
    /// A jitter-permitting hint request; final capping still applies.
    #[inline]
    pub fn jittered_hint(delay: Duration) -> Self {
        Self {
            hint: Some(delay),
            jitter_hint: true,
        }
    }
}
