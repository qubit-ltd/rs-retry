use std::time::Duration;

use qubit_retry::BackoffRequest;

#[test]
fn constructors_capture_public_delay_inputs() {
    let _ = BackoffRequest::policy();
    let _ = BackoffRequest::hint(Duration::from_millis(1));
    let _ = BackoffRequest::jittered_hint(Duration::from_millis(1));
}
