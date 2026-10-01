// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Blocking backoff support.

mod blocking_backoff;
mod blocking_backoff_outcome;
mod blocking_backoff_wake;

pub(crate) use blocking_backoff::wait_for_backoff;
pub(crate) use blocking_backoff_outcome::BlockingBackoffOutcome;
