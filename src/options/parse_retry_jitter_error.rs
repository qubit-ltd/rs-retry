/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry jitter parse error alias.

use parse_display::ParseError;

/// Failure to parse a [`crate::RetryJitter`] from text.
pub type ParseRetryJitterError = ParseError;
