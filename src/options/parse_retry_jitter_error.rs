/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026 Haixing Hu.
 *
 *    SPDX-License-Identifier: Apache-2.0
 *
 *    Licensed under the Apache License, Version 2.0.
 *
 ******************************************************************************/
//! Retry jitter parse error alias.

use parse_display::ParseError;

/// Failure to parse a [`crate::RetryJitter`] from text.
pub type ParseRetryJitterError = ParseError;
