// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Successful retry execution result.

use serde::Deserialize;
use serde::Serialize;

use crate::RetryContext;

/// Successful retry value together with the final retry context.
#[must_use]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RetrySuccess<T> {
    value: T,
    context: RetryContext,
}

impl<T> RetrySuccess<T> {
    #[inline]
    pub(crate) fn new(value: T, context: RetryContext) -> Self {
        Self { value, context }
    }

    /// Returns the successful operation value.
    #[inline(always)]
    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns the final retry context.
    #[inline(always)]
    #[must_use = "inspect the final retry context"]
    pub fn context(&self) -> &RetryContext {
        &self.context
    }

    /// Consumes this result and returns the successful value.
    #[inline(always)]
    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Consumes this result and returns its value and final context.
    #[inline(always)]
    #[must_use = "consume the value and final retry context"]
    pub fn into_parts(self) -> (T, RetryContext) {
        (self.value, self.context)
    }
}
