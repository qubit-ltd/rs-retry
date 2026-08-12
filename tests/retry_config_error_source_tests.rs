// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
// =============================================================================
#![cfg(feature = "config")]

use std::error::Error as _;

use qubit_config::ConfigError;
use qubit_retry::RetryConfigError;

#[test]
fn test_config_conversion_preserves_the_structured_source() {
    let error = RetryConfigError::from(ConfigError::Other("broken".to_owned()));

    assert!(
        error
            .source()
            .and_then(|source| source.downcast_ref::<ConfigError>())
            .is_some()
    );
}
