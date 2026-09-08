// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Weak channel waker for worker cancellation and timeout futures.

use std::sync::Arc;
use std::sync::Weak;
use std::sync::mpsc;
use std::task::Wake;

use super::worker_event::WorkerEvent;

/// Waker that forwards cancellation and timer readiness into the worker event
/// channel.
/// # Type Parameters
/// - `E`: Application error carried by the shared event channel.
pub(super) struct WorkerWake<E> {
    /// Weak sender that cannot conceal loss of both worker and reaper.
    pub(super) sender: Weak<mpsc::Sender<WorkerEvent<E>>>,
}

impl<E: Send + 'static> Wake for WorkerWake<E> {
    /// Sends a readiness notification when a registered future is woken.
    #[inline]
    fn wake(self: Arc<Self>) {
        if let Some(sender) = self.sender.upgrade() {
            let _ = sender.send(WorkerEvent::Wake);
        }
    }

    /// Sends readiness without consuming the shared waker.
    #[inline]
    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(sender) = self.sender.upgrade() {
            let _ = sender.send(WorkerEvent::Wake);
        }
    }
}
