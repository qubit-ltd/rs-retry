// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Waker for blocking retry delay notifications.

use std::sync::Arc;
use std::sync::mpsc;
use std::task::Wake;

/// Waker that forwards timer and cancellation notifications to one channel.
pub(super) struct BlockingBackoffWake {
    /// Notification sender shared by both futures.
    pub(super) sender: mpsc::Sender<()>,
}

impl Wake for BlockingBackoffWake {
    /// Wakes the blocking retry thread through its notification channel.
    #[inline]
    fn wake(self: Arc<Self>) {
        let _ = self.sender.send(());
    }

    /// Wakes the blocking retry thread without consuming the shared waker.
    #[inline]
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.sender.send(());
    }
}
