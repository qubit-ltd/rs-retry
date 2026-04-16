/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry decisions returned by a [`crate::RetryDecider`].
//!
//! A decider returns one of these values after inspecting an application error
//! and attempt context.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Decision returned by a [`crate::RetryDecider`] after inspecting an error.
///
/// The decision is advisory for retrying: [`RetryDecision::Retry`] still obeys
/// attempt and elapsed-time limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum RetryDecision {
    /// Retry the operation if limits still allow it.
    Retry,
    /// Abort immediately and return the current failure.
    Abort,
}
