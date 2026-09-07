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
#[must_use]
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
    /// # Parameters
    /// - `callback`: Category of callback that panicked.
    /// - `index`: Zero-based registration index of the callback.
    /// - `phase`: Lifecycle phase in which the callback panicked.
    /// - `panic`: Stable representation of the panic payload.
    ///
    /// # Returns
    /// An owned diagnostic with exact callback attribution and payload
    /// classification.
    #[must_use = "retain the callback failure"]
    #[inline]
    pub fn new(callback: RetryCallbackKind, index: usize, phase: RetryCallbackPhase, panic: RetryPanic) -> Self {
        Self {
            callback,
            index,
            phase,
            panic,
        }
    }

    /// Returns the callback category.
    ///
    /// # Returns
    /// Rule or observer category recorded when the panic was captured.
    #[must_use]
    #[inline(always)]
    pub fn callback(&self) -> RetryCallbackKind {
        self.callback
    }

    /// Returns the callback's zero-based registration index.
    ///
    /// # Returns
    /// Zero-based registration position within the callback collection.
    #[must_use]
    #[inline(always)]
    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns the lifecycle phase in which the callback panicked.
    ///
    /// # Returns
    /// Lifecycle phase observed at the capture boundary.
    #[must_use]
    #[inline(always)]
    pub fn phase(&self) -> RetryCallbackPhase {
        self.phase
    }

    /// Returns the stable panic payload representation.
    ///
    /// # Returns
    /// Borrowed payload representation; no downcast or allocation occurs.
    #[must_use]
    #[inline(always)]
    pub fn panic(&self) -> &RetryPanic {
        &self.panic
    }
}

impl fmt::Display for RetryCallbackFailure {
    ///
    /// Formats callback category, index, phase, and captured panic together.
    ///
    /// # Parameters
    /// - `formatter`: Destination supplied by the formatting machinery.
    ///
    /// # Returns
    /// The result of writing this diagnostic representation.
    ///
    /// # Errors
    /// Returns a formatting error if the destination rejects a write.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} callback {} panicked during {}: {}",
            self.callback, self.index, self.phase, self.panic
        )
    }
}
