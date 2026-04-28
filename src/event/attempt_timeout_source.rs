/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Attempt timeout source metadata.

use serde::{Deserialize, Serialize};

/// Source of a per-attempt timeout selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptTimeoutSource {
    /// Timeout selected from [`RetryOptions`](crate::RetryOptions) attempt timeout
    /// configuration.
    Configured,
    /// Timeout selected from remaining max-operation-elapsed budget.
    MaxOperationElapsed,
    /// Timeout selected from remaining max-total-elapsed budget.
    MaxTotalElapsed,
}
