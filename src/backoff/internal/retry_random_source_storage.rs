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

/// Random source retained by one mutable backoff sequence.
#[derive(Clone)]
pub enum RetryRandomSourceStorage {
    /// Thread-local random source used by the default constructor.
    Thread(ThreadRetryRandomSource),
    /// Caller-provided random source shared by the state.
    Custom(Arc<dyn RetryRandomSource>),
}

impl RetryRandomSourceStorage {
    /// Borrows the stored source through the common random-source interface.
    pub fn as_source(&self) -> &dyn RetryRandomSource {
        match self {
            Self::Thread(source) => source,
            Self::Custom(source) => source.as_ref(),
        }
    }
}
