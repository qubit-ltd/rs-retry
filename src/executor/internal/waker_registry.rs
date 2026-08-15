// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Waker ownership for pending retry-cancellation futures.

use std::collections::HashMap;
use std::task::Waker;

/// Tracks the current waker for each pending cancellation future.
#[derive(Debug, Default)]
pub(super) struct WakerRegistry {
    /// Identifier to consider for the next new registration.
    next_id: u64,
    /// Current wakers keyed by their future's stable identifier.
    wakers: HashMap<u64, Waker>,
}

impl WakerRegistry {
    /// Registers a waker and returns its identifier and any replaced waker.
    pub(in crate::executor::internal) fn register(
        &mut self,
        registration_id: Option<u64>,
        waker: Waker,
    ) -> (u64, Option<Waker>) {
        let registration_id = registration_id.unwrap_or_else(|| {
            loop {
                let candidate = self.next_id;
                self.next_id = self.next_id.wrapping_add(1);
                if !self.wakers.contains_key(&candidate) {
                    break candidate;
                }
            }
        });
        let replaced = self.wakers.insert(registration_id, waker);
        (registration_id, replaced)
    }

    /// Removes a registration and returns the waker that must be dropped.
    pub(in crate::executor::internal) fn unregister(
        &mut self,
        registration_id: u64,
    ) -> Option<Waker> {
        self.wakers.remove(&registration_id)
    }

    /// Removes every registered waker before any of them are invoked.
    pub(in crate::executor::internal) fn take_all(&mut self) -> Vec<Waker> {
        self.wakers.drain().map(|(_, waker)| waker).collect()
    }
}
