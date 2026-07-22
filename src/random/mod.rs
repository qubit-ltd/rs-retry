// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Random-source interfaces used by retry delay selection.

mod internal;
mod retry_random_source;

pub use retry_random_source::RetryRandomSource;

pub(crate) use internal::ThreadRetryRandomSource;
