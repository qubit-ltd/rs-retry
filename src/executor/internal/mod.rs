// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared implementation state for retry executors.

mod effective_timeout;
mod retry_flow_state;

pub(crate) use effective_timeout::EffectiveTimeout;
pub(crate) use retry_flow_state::RetryFlowState;
