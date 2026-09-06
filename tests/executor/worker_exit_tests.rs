// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Worker completion must remain cancellable while TLS destructors run.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::sync::mpsc::{self};
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TimerFuture;
use qubit_retry::Retry;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryPolicy;
use qubit_retry::WorkerStopTrigger;

struct ExitGate {
    entered: Sender<()>,
    release: Receiver<()>,
}

impl Drop for ExitGate {
    fn drop(&mut self) {
        let _ = self.entered.send(());
        let _ = self.release.recv();
    }
}

thread_local! {
    static EXIT_GATE: RefCell<Option<ExitGate>> = const { RefCell::new(None) };
}

/// Cancels after the operation has returned but before TLS destruction
/// finishes.
#[test]
fn test_worker_exit_cancellation_bounds_tls_destructor() {
    assert_tls_exit_is_bounded(false, WorkerStopTrigger::Cancellation);
}

/// A returned business error cannot trigger another attempt before thread exit.
#[test]
fn test_worker_exit_error_cancellation_bounds_tls_destructor() {
    assert_tls_exit_is_bounded(true, WorkerStopTrigger::Cancellation);
}

/// The original manual attempt deadline remains active during TLS destruction.
#[test]
fn test_worker_exit_timeout_bounds_tls_destructor() {
    assert_tls_exit_is_bounded(false, WorkerStopTrigger::AttemptTimeout);
}

/// A failed timer poll also bounds the wait for TLS destruction.
#[test]
fn test_worker_exit_timer_failure_bounds_tls_destructor() {
    assert_tls_exit_is_bounded(false, WorkerStopTrigger::TimerFailure);
}

/// Installs a controlled destructor and always releases it before asserting.
fn assert_tls_exit_is_bounded(operation_fails: bool, trigger: WorkerStopTrigger) {
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let (result_sender, result_receiver) = mpsc::channel();
    let gate = Mutex::new(Some(ExitGate {
        entered: entered_sender,
        release: release_receiver,
    }));
    let calls = Arc::new(AtomicUsize::new(0));
    let operation_calls = Arc::clone(&calls);
    let token = RetryCancellationToken::new();
    let runner_token = token.clone();
    let clock = ManualMonotonicClock::new_shared();
    let timer = Arc::new(ExitTimer {
        timer: clock.new_timer(),
        fail: trigger == WorkerStopTrigger::TimerFailure,
    });
    let runner = std::thread::spawn(move || {
        let retry = Retry::<&'static str>::builder(RetryPolicy::builder().build().expect("valid policy")).build();
        let mut worker = retry
            .worker()
            .timer(timer)
            .cancellation_token(runner_token)
            .cancellation_grace(Duration::from_millis(20));
        if trigger != WorkerStopTrigger::Cancellation {
            worker = worker.attempt_timeout(Duration::from_secs(60));
        }
        let result = worker.run(move |_| {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            EXIT_GATE.with(|slot| *slot.borrow_mut() = gate.lock().expect("gate lock").take());
            if operation_fails {
                Err("operation failed")
            } else {
                Ok(())
            }
        });
        let _ = result_sender.send(result);
    });
    let entered = entered_receiver.recv_timeout(Duration::from_secs(2));
    if trigger == WorkerStopTrigger::Cancellation || entered.is_err() {
        token.cancel();
    } else {
        clock
            .advance(Duration::from_secs(60))
            .expect("expire original deadline");
    }
    let result = result_receiver.recv_timeout(Duration::from_secs(2));
    let _ = release_sender.send(());
    runner.join().expect("retry runner joins after gate release");
    entered.expect("worker entered TLS destruction");
    let error = result
        .expect("retry must return before TLS gate release")
        .expect_err("worker still exiting");
    assert!(matches!(error.failure(), RetryFailure::Infrastructure {
        failure: RetryInfrastructureFailure::WorkerStillRunning { trigger: actual }, ..
    } if *actual == trigger));
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// Converts a manually released timeout into a controlled poll failure.
struct ExitTimer {
    timer: Arc<dyn Timer>,
    fail: bool,
}

impl Timer for ExitTimer {
    fn clock(&self) -> &dyn MonotonicClock {
        self.timer.clock()
    }

    fn at(&self, deadline: MonotonicInstant) -> Result<TimerFuture, TimeError> {
        let future = self.timer.at(deadline)?;
        let fail = self.fail;
        Ok(Box::pin(async move {
            future.await?;
            if fail { Err(TimeError::InstantOverflow) } else { Ok(()) }
        }))
    }
}

/// Existing downstream matches can route the new infrastructure cause to their
/// fallback.
#[test]
fn test_worker_channel_closed_preserves_downstream_fallback() {
    let failure = RetryInfrastructureFailure::WorkerChannelClosed;
    let category = match &failure {
        RetryInfrastructureFailure::Clock { .. } => "clock",
        RetryInfrastructureFailure::Timer { .. } => "timer",
        RetryInfrastructureFailure::WorkerSpawn { .. } => "spawn",
        RetryInfrastructureFailure::WorkerStillRunning { .. } => "running",
        _ => "unknown infrastructure",
    };
    assert_eq!(category, "unknown infrastructure");
    assert_eq!(failure.message(), None);
    assert_eq!(failure.worker_stop_trigger(), None);
    assert_eq!(
        failure.to_string(),
        "worker event channel closed before exit was confirmed"
    );
}
