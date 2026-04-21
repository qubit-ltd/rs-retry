/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use qubit_retry::{RetryAttemptFailure, RetryDelay, RetryExecutor};

/// Verifies README and README.zh_CN no longer contain stale public API symbols.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when README docs still contain stale API names.
#[test]
fn test_readme_uses_current_public_api_names() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let readme = fs::read_to_string(root.join("README.md")).expect("README.md should be readable");
    let readme_zh = fs::read_to_string(root.join("README.zh_CN.md"))
        .expect("README.zh_CN.md should be readable");

    for content in [&readme, &readme_zh] {
        assert!(content.contains("RetryDelay::none"));
        assert!(content.contains("RetryJitter::factor"));
        assert!(content.contains("RetryAttemptFailure"));

        assert!(!content.contains("use qubit_retry::{Delay, RetryExecutor};"));
        assert!(!content.contains("use qubit_retry::{AttemptFailure, Delay, RetryExecutor};"));
        assert!(!content.contains("ArcBiConsumer<RetryContext, AttemptFailure<E>>"));
        assert!(!content.contains("ArcBiConsumer<FailureContext, Option<AttemptFailure<E>>>"));
    }
}

/// Verifies README-style listener snippets compile and run with current API names.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
///
/// # Errors
/// The test fails through assertions when listener examples do not compile or do
/// not run as expected.
#[test]
fn test_readme_listener_example_compiles_and_runs() {
    let retry_count = Arc::new(AtomicUsize::new(0));
    let retry_count_for_listener = Arc::clone(&retry_count);
    let attempt_count = Arc::new(AtomicUsize::new(0));
    let attempt_count_for_run = Arc::clone(&attempt_count);

    let executor = RetryExecutor::<std::io::Error>::builder()
        .max_attempts(3)
        .delay(RetryDelay::fixed(Duration::from_millis(1)))
        .on_retry(move |context, failure| {
            assert!(context.attempt >= 1);
            if let RetryAttemptFailure::Error(error) = failure {
                assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
                retry_count_for_listener.fetch_add(1, Ordering::SeqCst);
            }
        })
        .build()
        .expect("executor should be built");

    let result = executor.run(|| {
        let attempt = attempt_count_for_run.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt == 1 {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "temporary",
            ))
        } else {
            Ok("ok")
        }
    });

    assert_eq!(result.expect("retry should eventually succeed"), "ok");
    assert_eq!(attempt_count.load(Ordering::SeqCst), 2);
    assert_eq!(retry_count.load(Ordering::SeqCst), 1);
}
