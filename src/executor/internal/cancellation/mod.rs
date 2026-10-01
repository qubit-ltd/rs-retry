// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-independent cancellation state.

mod retry_cancellation_state;
mod waker_registry;

pub(in crate::executor) use retry_cancellation_state::RetryCancellationState;
