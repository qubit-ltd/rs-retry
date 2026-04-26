/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Terminal retry-flow error reasons.

use serde::{Deserialize, Serialize};

/// Reason why the whole retry flow stopped with an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryErrorReason {
    /// A listener or retry policy aborted the retry flow.
    Aborted,
    /// No attempts remain.
    AttemptsExceeded,
    /// The total elapsed-time budget was exhausted.
    MaxElapsedExceeded,
}
