// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry lifecycle observation.

mod internal;
mod retry_observer;

pub(crate) use internal::RetryObservers;
pub use retry_observer::RetryObserver;

pub use crate::event::RetryContext;
