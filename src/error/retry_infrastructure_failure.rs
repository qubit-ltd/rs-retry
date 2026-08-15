// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry-flow infrastructure failure values.

use std::fmt;

use super::WorkerStopTrigger;

/// Infrastructure failure that prevented safe retry-flow continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryInfrastructureFailure {
    /// Reading the retry clock failed.
    Clock {
        /// Diagnostic supplied by the clock implementation.
        message: Box<str>,
    },
    /// Waiting on a retry timer failed.
    Timer {
        /// Diagnostic supplied by the timer implementation.
        message: Box<str>,
    },
    /// Starting an isolated worker failed.
    WorkerSpawn {
        /// Diagnostic supplied by the worker runtime.
        message: Box<str>,
    },
    /// A stopped worker did not exit within its grace period.
    WorkerStillRunning {
        /// Event that requested the worker to stop.
        trigger: WorkerStopTrigger,
    },
}

impl RetryInfrastructureFailure {
    /// Returns the infrastructure diagnostic message.
    ///
    /// # Returns
    /// `Some(&str)` for clock, timer, and worker-spawn failures, or `None` for
    /// a worker that remained running.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Clock { message }
            | Self::Timer { message }
            | Self::WorkerSpawn { message } => Some(message),
            Self::WorkerStillRunning { .. } => None,
        }
    }

    /// Returns the trigger for a worker that remained running.
    ///
    /// # Returns
    /// `Some(WorkerStopTrigger)` for `WorkerStillRunning`, or `None` for other
    /// infrastructure failures.
    #[must_use]
    pub fn worker_stop_trigger(&self) -> Option<WorkerStopTrigger> {
        match self {
            Self::WorkerStillRunning { trigger } => Some(*trigger),
            Self::Clock { .. }
            | Self::Timer { .. }
            | Self::WorkerSpawn { .. } => None,
        }
    }
}

impl fmt::Display for RetryInfrastructureFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock { message } => {
                write!(formatter, "clock failed: {message}")
            }
            Self::Timer { message } => {
                write!(formatter, "timer failed: {message}")
            }
            Self::WorkerSpawn { message } => {
                write!(formatter, "worker spawn failed: {message}")
            }
            Self::WorkerStillRunning { trigger } => {
                write!(formatter, "worker still running after {trigger}")
            }
        }
    }
}
