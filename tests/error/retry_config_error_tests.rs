// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use qubit_datatype::{
    DataConversionError,
    DataType,
};
use qubit_value::ValueError;

/// Verifies configuration error display output for empty and non-empty paths.
#[test]
fn test_display_handles_empty_and_non_empty_paths() {
    let explicit =
        qubit_retry::RetryConfigError::invalid_value("retry.delay", "bad");
    assert_eq!(explicit.path(), "retry.delay");
    assert_eq!(explicit.message(), "bad");
    assert!(explicit.to_string().contains("retry.delay"));

    let converted = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::Other("broken".to_string()),
    );
    assert_eq!(converted.path(), "");
    assert!(converted.to_string().contains("broken"));
}

/// Verifies `ConfigError` conversions preserve key context where available.
#[test]
fn test_from_config_error_preserves_path_variants() {
    let not_found = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::PropertyNotFound("missing.key".to_string()),
    );
    assert_eq!(not_found.path(), "missing.key");

    let no_value = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::PropertyHasNoValue("empty.key".to_string()),
    );
    assert_eq!(no_value.path(), "empty.key");

    let final_property = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::PropertyIsFinal("final.key".to_string()),
    );
    assert_eq!(final_property.path(), "final.key");

    let deserialize = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::DeserializeError {
            path: "object.path".to_string(),
            message: "bad object".to_string(),
            source: None,
        },
    );
    assert_eq!(deserialize.path(), "object.path");

    let key_conflict = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::KeyConflict {
            path: "conflict.path".to_string(),
            existing: "scalar".to_string(),
            incoming: "object".to_string(),
        },
    );
    assert_eq!(key_conflict.path(), "conflict.path");
}

/// Verifies typed config conversion errors preserve the key field.
#[test]
fn test_from_config_error_preserves_typed_key_variants() {
    let type_mismatch = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::TypeMismatch {
            key: "typed.key".to_string(),
            expected: DataType::UInt32,
            actual: DataType::String,
        },
    );
    assert_eq!(type_mismatch.path(), "typed.key");

    let conversion = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::ConversionError {
            key: "converted.key".to_string(),
            source_index: None,
            source: DataConversionError::Unsupported {
                from: DataType::String,
                to: DataType::UInt32,
            },
        },
    );
    assert_eq!(conversion.path(), "converted.key");

    let value_error = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::ValueError {
            key: "value.key".to_string(),
            source: ValueError::TypeMismatch {
                expected: DataType::UInt32,
                actual: DataType::String,
            },
        },
    );
    assert_eq!(value_error.path(), "value.key");
}

/// Verifies config errors without a single key context use an empty path.
#[test]
fn test_from_config_error_uses_empty_path_without_key_context() {
    let cycle = qubit_retry::RetryConfigError::from(
        qubit_config::ConfigError::SubstitutionCycle {
            chain: vec!["a".to_string(), "b".to_string(), "a".to_string()],
        },
    );
    assert_eq!(cycle.path(), "");
    assert!(cycle.message().contains("a -> b -> a"));
}
