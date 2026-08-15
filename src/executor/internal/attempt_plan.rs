// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Admission plan for one runtime-independent attempt.

use super::EffectiveTimeout;

/// One admitted attempt together with its effective hard timeout.
#[derive(Clone, Copy)]
pub(crate) struct AttemptPlan {
    /// Runtime-independent timeout selected for this operation.
    pub(super) timeout: Option<EffectiveTimeout>,
}

impl AttemptPlan {
    /// Returns the effective hard timeout selected for this attempt.
    pub(crate) fn timeout(&self) -> Option<EffectiveTimeout> {
        self.timeout
    }
}
