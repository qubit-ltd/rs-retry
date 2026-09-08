// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime snapshot captured by a retry execution.

mod internal;
mod retry_context;

pub(crate) use internal::RetryContextParts;
pub use retry_context::RetryContext;
