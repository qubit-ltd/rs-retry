// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Structured retry callback failures.

use std::fmt;

use super::RetryCallbackKind;
use super::RetryCallbackPhase;
use super::RetryPanic;

/// Panic raised by one registered retry callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryCallbackFailure {
    /// Category of callback that panicked.
    callback: RetryCallbackKind,
    /// Zero-based registration index of the callback.
    index: usize,
    /// Lifecycle phase in which the callback panicked.
    phase: RetryCallbackPhase,
    /// Stable representation of the panic payload.
    panic: RetryPanic,
}

impl RetryCallbackFailure {
    /// Creates a callback failure with complete callback attribution.
    ///
    /// # Arguments
    /// - `callback`: Category of callback that panicked.
    /// - `index`: Zero-based registration index of the callback.
    /// - `phase`: Lifecycle phase in which the callback panicked.
    /// - `panic`: Stable representation of the panic payload.
    #[must_use]
    pub fn new(
        callback: RetryCallbackKind,
        index: usize,
        phase: RetryCallbackPhase,
        panic: RetryPanic,
    ) -> Self {
        Self {
            callback,
            index,
            phase,
            panic,
        }
    }

    /// Returns the callback category.
    #[must_use]
    pub fn callback(&self) -> RetryCallbackKind {
        self.callback
    }

    /// Returns the callback's zero-based registration index.
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns the lifecycle phase in which the callback panicked.
    #[must_use]
    pub fn phase(&self) -> RetryCallbackPhase {
        self.phase
    }

    /// Returns the stable panic payload representation.
    #[must_use]
    pub fn panic(&self) -> &RetryPanic {
        &self.panic
    }
}

impl fmt::Display for RetryCallbackFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} callback {} panicked during {}: {}",
            self.callback, self.index, self.phase, self.panic
        )
    }
}
