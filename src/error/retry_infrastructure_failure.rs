// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retry-flow infrastructure failure values.

use std::fmt;

#[cfg(feature = "worker")]
use super::WorkerStopTrigger;

/// Infrastructure failure that prevented safe retry-flow continuation.
///
/// # Examples
/// ```
/// use qubit_retry::RetryInfrastructureFailure;
///
/// let failure = RetryInfrastructureFailure::Timer {
///     message: "timer unavailable".into(),
/// };
/// assert_eq!(failure.message(), Some("timer unavailable"));
/// ```
///
/// With the `worker` feature, a worker stop trigger can be inspected as well:
///
/// ```
/// # #[cfg(feature = "worker")] {
/// use qubit_retry::RetryInfrastructureFailure;
/// use qubit_retry::WorkerStopTrigger;
///
/// let failure = RetryInfrastructureFailure::WorkerStillRunning {
///     trigger: WorkerStopTrigger::Cancellation,
/// };
/// assert_eq!(failure.worker_stop_trigger(), Some(WorkerStopTrigger::Cancellation));
/// # }
/// ```
#[must_use]
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
    #[cfg(feature = "worker")]
    WorkerSpawn {
        /// Diagnostic supplied by the worker runtime.
        message: Box<str>,
    },
    /// The worker event channel closed before its exit protocol completed.
    #[cfg(feature = "worker")]
    WorkerChannelClosed,
    /// A stopped worker did not exit within its grace period.
    #[cfg(feature = "worker")]
    WorkerStillRunning {
        /// Event that requested the worker to stop.
        trigger: WorkerStopTrigger,
    },
}

impl RetryInfrastructureFailure {
    /// Returns the diagnostic text for failures that carry one.
    ///
    /// # Returns
    /// `Some(&str)` for clock, timer, and (with `worker`) worker-spawn
    /// failures. Returns `None` for a closed worker event channel or a worker
    /// that remained running after a stop request.
    #[must_use]
    #[inline]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Clock { message } | Self::Timer { message } => Some(message),
            #[cfg(feature = "worker")]
            Self::WorkerSpawn { message } => Some(message),
            #[cfg(feature = "worker")]
            Self::WorkerStillRunning { .. } | Self::WorkerChannelClosed => None,
        }
    }

    /// Returns the event that requested a still-running worker to stop.
    ///
    /// # Returns
    /// `Some(WorkerStopTrigger)` for `WorkerStillRunning`, or `None` for other
    /// infrastructure failures.
    #[must_use]
    #[inline]
    #[cfg(feature = "worker")]
    pub fn worker_stop_trigger(&self) -> Option<WorkerStopTrigger> {
        match self {
            Self::WorkerStillRunning { trigger } => Some(*trigger),
            Self::Clock { .. } | Self::Timer { .. } | Self::WorkerSpawn { .. } | Self::WorkerChannelClosed => None,
        }
    }
}

impl fmt::Display for RetryInfrastructureFailure {
    /// Formats the infrastructure cause and available runtime diagnostic.
    ///
    /// # Parameters
    /// - `formatter`: Destination supplied by the formatting machinery.
    ///
    /// # Returns
    /// The result of writing this diagnostic representation.
    ///
    /// # Errors
    /// Returns a formatting error if the destination rejects a write.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clock { message } => {
                write!(formatter, "clock failed: {message}")
            }
            Self::Timer { message } => {
                write!(formatter, "timer failed: {message}")
            }
            #[cfg(feature = "worker")]
            Self::WorkerSpawn { message } => {
                write!(formatter, "worker spawn failed: {message}")
            }
            #[cfg(feature = "worker")]
            Self::WorkerChannelClosed => {
                write!(formatter, "worker event channel closed before exit was confirmed")
            }
            #[cfg(feature = "worker")]
            Self::WorkerStillRunning { trigger } => {
                write!(formatter, "worker still running after {trigger}")
            }
        }
    }
}
