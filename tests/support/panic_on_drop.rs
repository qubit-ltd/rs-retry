// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

/// Panic payload whose destructor triggers a second panic after capture.
pub(crate) struct PanicOnDrop;

impl Drop for PanicOnDrop {
    /// Panics while the captured payload is being discarded.
    ///
    /// # Panics
    /// Always panics so worker-disconnect handling can be observed through the
    /// public retry API.
    fn drop(&mut self) {
        panic!("panic payload drop failed");
    }
}
