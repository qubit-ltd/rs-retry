// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Defines the linear token for one admitted retry attempt.

use std::sync::Arc;

/// A single admitted retry attempt.
///
/// A value is returned by [`super::RetryBudget::begin_attempt`] and must be
/// consumed by [`super::RetryBudget::finish_attempt`]. It intentionally is not
/// clonable, so one attempt cannot be finished more than once.
#[must_use]
#[derive(Debug)]
pub struct RetryAttempt {
    /// One-based ordinal assigned when this attempt was admitted.
    pub(super) number: u32,

    /// Identity of the budget that admitted this operation.
    pub(super) owner: Arc<()>,
}

impl RetryAttempt {
    /// Returns the one-based ordinal of this admitted attempt.
    ///
    /// # Returns
    /// The committed one-based admission ordinal, independent of callbacks.
    #[inline(always)]
    #[must_use]
    pub const fn number(&self) -> u32 {
        self.number
    }
}
