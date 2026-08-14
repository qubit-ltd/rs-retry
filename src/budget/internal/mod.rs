// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private continuation-budget helper types.

mod borrowed_monotonic_clock;
mod retry_resource;

pub(super) use borrowed_monotonic_clock::BorrowedMonotonicClock;
pub(super) use retry_resource::RetryResource;
