// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Storage for the default or caller-provided backoff random source.

use std::sync::Arc;

use crate::RetryRandomSource;
use crate::random::ThreadRetryRandomSource;

/// Keeps the random source used by one mutable backoff sequence.
///
/// The default variant selects the current thread's generator, while the
/// custom variant shares the caller-provided source through an `Arc`.
#[derive(Clone)]
pub enum RetryRandomSourceStorage {
    /// Thread-local random source used by the default constructor.
    Thread(ThreadRetryRandomSource),
    /// Caller-provided random source shared by the state.
    Custom(Arc<dyn RetryRandomSource>),
}

impl RetryRandomSourceStorage {
    /// Borrows the stored source through the common random-source interface.
    ///
    /// # Returns
    /// A reference to the selected source, tied to this storage's borrow.
    #[must_use]
    #[inline]
    pub fn as_source(&self) -> &dyn RetryRandomSource {
        match self {
            Self::Thread(source) => source,
            Self::Custom(source) => source.as_ref(),
        }
    }
}
