// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared asynchronous retry outcomes.

mod async_attempt_outcome;
mod async_backoff_outcome;

pub(in crate::executor) use async_attempt_outcome::AsyncAttemptOutcome;
pub(in crate::executor) use async_backoff_outcome::AsyncBackoffOutcome;
