/*******************************************************************************
 *
 *    Copyright (c) 2025 - 2026.
 *    Haixing Hu, Qubit Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/

/// Verifies coverage-only hooks exercise defensive retry executor paths.
///
/// # Parameters
/// This test has no parameters.
///
/// # Returns
/// This test returns nothing.
#[test]
fn test_coverage_support_exercises_defensive_paths() {
    let diagnostics = qubit_retry::coverage_support::exercise_defensive_paths();

    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("failed to spawn retry worker thread")),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("owned panic")),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("timeout source absent=true")),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("sleep budget exhausted=true")),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("zero grace empty worker exited=false")),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("application source")),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("owned application error")),
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("attempt timed out; cancelled=true")),
    );
    assert!(
        diagnostics
            .iter()
            .filter(|message| message.contains("retry worker thread stopped without sending"))
            .count()
            >= 2,
    );
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("attempt panicked: coverage panic")),
    );
}
