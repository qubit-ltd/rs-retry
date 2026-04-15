/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! Retry classifier decisions.
//!
//! A classifier returns one of these values after inspecting an application
//! error and attempt context.

/// Error classifier decision.
///
/// The decision is advisory for retrying: [`RetryDecision::Retry`] still obeys
/// attempt and elapsed-time limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Retry the operation if limits still allow it.
    Retry,
    /// Abort immediately and return the current failure.
    Abort,
}
