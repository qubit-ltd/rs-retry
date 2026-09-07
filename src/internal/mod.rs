// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared panic-payload conversion for callbacks and worker threads.

mod retry_panic_from_payload;

pub(crate) use retry_panic_from_payload::retry_panic_from_payload;
