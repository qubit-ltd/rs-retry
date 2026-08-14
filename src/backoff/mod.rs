// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Reusable retry and reconnect backoff calculations.

mod backoff_delay_source;
mod backoff_policy;
mod backoff_request;
mod backoff_state;
mod backoff_step;
mod internal;

pub use backoff_delay_source::BackoffDelaySource;
pub use backoff_policy::BackoffPolicy;
pub use backoff_request::BackoffRequest;
pub use backoff_state::BackoffState;
pub use backoff_step::BackoffStep;
