// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;
use std::time::Duration;

use qubit_clock::ManualMonotonicClock;
use qubit_clock::MonotonicClock;
use qubit_clock::MonotonicInstant;
use qubit_clock::TimeError;
use qubit_clock::Timer;
use qubit_clock::TimerFuture;
use qubit_retry::AttemptCancellationToken;
use qubit_retry::AttemptFailure;
use qubit_retry::BackoffPolicy;
use qubit_retry::Retry;
use qubit_retry::RetryCancellationPhase;
use qubit_retry::RetryCancellationToken;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryFailure;
use qubit_retry::RetryInfrastructureFailure;
use qubit_retry::RetryObserver;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryTimeoutScope;
use qubit_retry::WorkerStopTrigger;

use crate::support::TestError;

/// Counts attempt-failed observer callbacks.
struct CountAttemptFailed {
    calls: Arc<AtomicUsize>,
}

/// Shared state for a manually completed pending timer future.
struct PendingTimerState {
    /// Whether the test has completed the pending timer.
    ready: AtomicBool,
    /// Latest waker registered by the blocking retry runner.
    waker: Mutex<Option<Waker>>,
}

impl PendingTimerState {
    /// Completes the timer and wakes its registered runner.
    fn complete(&self) {
        self.ready.store(true, Ordering::SeqCst);
        if let Some(waker) = self
            .waker
            .lock()
            .expect("pending timer waker lock should remain valid")
            .take()
        {
            waker.wake();
        }
    }
}

/// Future controlled by [`PendingTimerState`].
struct PendingTimerFuture {
    /// State shared with the test thread.
    state: Arc<PendingTimerState>,
}

impl Future for PendingTimerFuture {
    type Output = Result<(), TimeError>;

    /// Completes after the test marks the shared timer state ready.
    fn poll(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        if self.state.ready.load(Ordering::SeqCst) {
            Poll::Ready(Ok(()))
        } else {
            *self
                .state
                .waker
                .lock()
                .expect("pending timer waker lock should remain valid") =
                Some(context.waker().clone());
            if self.state.ready.load(Ordering::SeqCst) {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }
    }
}

/// Timer that reports registration and remains pending until explicitly set.
struct PendingTimer {
    /// Stable clock used by the retry controller.
    clock: Arc<ManualMonotonicClock>,
    /// Registration event sent to the coordinating test thread.
    registered: std::sync::mpsc::Sender<()>,
    /// State controlling the returned timer future.
    state: Arc<PendingTimerState>,
}

/// Timer that cancels the flow during registration and then reports failure.
struct CancellingFailingTimer {
    /// Stable clock used by the retry controller.
    clock: Arc<ManualMonotonicClock>,
    /// Token cancelled during timer registration.
    cancellation: RetryCancellationToken,
    /// Number of attempted timer registrations.
    registrations: Arc<AtomicUsize>,
}

impl Timer for CancellingFailingTimer {
    /// Returns the stable manual clock used by this timer.
    fn clock(&self) -> &dyn MonotonicClock {
        self.clock.as_ref()
    }

    /// Cancels the retry before returning a deterministic registration error.
    fn at(
        &self,
        _deadline: MonotonicInstant,
    ) -> Result<TimerFuture, TimeError> {
        self.registrations.fetch_add(1, Ordering::SeqCst);
        self.cancellation.cancel();
        Err(TimeError::InstantOverflow)
    }
}

impl Timer for PendingTimer {
    /// Returns the stable manual clock used by this timer.
    fn clock(&self) -> &dyn MonotonicClock {
        self.clock.as_ref()
    }

    /// Registers one pending deadline and notifies the test coordinator.
    fn at(
        &self,
        _deadline: MonotonicInstant,
    ) -> Result<TimerFuture, TimeError> {
        self.registered
            .send(())
            .expect("test should observe the backoff timer registration");
        Ok(Box::pin(PendingTimerFuture {
            state: Arc::clone(&self.state),
        }))
    }
}

impl RetryObserver<TestError> for CountAttemptFailed {
    /// Records one failed-attempt callback.
    fn on_attempt_failed(
        &self,
        _failure: &AttemptFailure<TestError>,
        _context: &RetryContext,
    ) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

/// Verifies pre-cancellation prevents worker construction and operation start.
#[test]
fn test_worker_pre_cancellation_does_not_start_operation() {
    let cancellation = RetryCancellationToken::new();
    cancellation.cancel();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .build()
            .expect("pre-cancellation policy should be valid"),
    )
    .build()
    .worker()
    .cancellation_token(cancellation)
    .run({
        let operation_calls = Arc::clone(&operation_calls);
        move |_: AttemptCancellationToken| {
            operation_calls.fetch_add(1, Ordering::SeqCst);
            Ok::<(), TestError>(())
        }
    })
    .expect_err("pre-cancellation must stop before spawning an operation");

    let RetryFailure::Cancelled {
        phase,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a cancellation terminal");
    };
    assert_eq!(*phase, RetryCancellationPhase::BeforeAttempt);
    assert!(last_failure.is_none());
    assert_eq!(error.context().attempts(), 0);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(operation_calls.load(Ordering::SeqCst), 0);
}

/// Verifies active cancellation marks the attempt token and wins over success.
#[test]
fn test_worker_attempt_cancellation_discards_late_success() {
    let cancellation = RetryCancellationToken::new();
    let operation_cancellation = cancellation.clone();
    let observed_token = Arc::new(Mutex::new(None));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .build()
            .expect("attempt cancellation policy should be valid"),
    )
    .build()
    .worker()
    .cancellation_token(cancellation)
    .run({
        let observed_token = Arc::clone(&observed_token);
        move |token: AttemptCancellationToken| {
            *observed_token
                .lock()
                .expect("attempt token slot should remain valid") =
                Some(token.clone());
            operation_cancellation.cancel();
            Ok::<(), TestError>(())
        }
    })
    .expect_err("active cancellation must win over a late success");

    let RetryFailure::Cancelled {
        phase,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a cancellation terminal");
    };
    assert_eq!(*phase, RetryCancellationPhase::Attempt);
    assert!(last_failure.is_none());
    assert!(
        observed_token
            .lock()
            .expect("attempt token slot should remain valid")
            .as_ref()
            .expect("operation should expose its attempt token")
            .is_cancelled()
    );
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(
        error
            .context()
            .current_attempt()
            .map(std::num::NonZeroU32::get),
        Some(1)
    );
}

/// Verifies an unbounded grace still reaps a cooperatively exiting worker.
#[test]
fn test_worker_attempt_cancellation_supports_maximum_grace() {
    let cancellation = RetryCancellationToken::new();
    let operation_cancellation = cancellation.clone();
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .build()
            .expect("maximum-grace cancellation policy should be valid"),
    )
    .build()
    .worker()
    .cancellation_grace(Duration::MAX)
    .cancellation_token(cancellation)
    .run(move |_: AttemptCancellationToken| {
        operation_cancellation.cancel();
        Ok::<(), TestError>(())
    })
    .expect_err("cooperative cancellation must remain a cancellation terminal");

    let RetryFailure::Cancelled {
        phase,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a cancellation terminal");
    };
    assert_eq!(*phase, RetryCancellationPhase::Attempt);
    assert!(last_failure.is_none());
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(
        error
            .context()
            .current_attempt()
            .map(std::num::NonZeroU32::get),
        Some(1)
    );
}

/// Verifies a late application error cannot invoke failure callbacks or rules.
#[test]
fn test_worker_attempt_cancellation_discards_late_error() {
    let cancellation = RetryCancellationToken::new();
    let operation_cancellation = cancellation.clone();
    let failed_observer_calls = Arc::new(AtomicUsize::new(0));
    let rule_calls = Arc::new(AtomicUsize::new(0));
    let retry = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .build()
            .expect("late-error cancellation policy should be valid"),
    )
    .observer(CountAttemptFailed {
        calls: Arc::clone(&failed_observer_calls),
    })
    .rule({
        let rule_calls = Arc::clone(&rule_calls);
        move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            rule_calls.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        }
    })
    .build();

    let error = retry
        .worker()
        .cancellation_token(cancellation)
        .run(move |_: AttemptCancellationToken| {
            operation_cancellation.cancel();
            Err::<(), _>(TestError("late error"))
        })
        .expect_err("active cancellation must win over a late error");

    let RetryFailure::Cancelled {
        phase,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a cancellation terminal");
    };
    assert_eq!(*phase, RetryCancellationPhase::Attempt);
    assert!(last_failure.is_none());
    assert_eq!(failed_observer_calls.load(Ordering::SeqCst), 0);
    assert_eq!(rule_calls.load(Ordering::SeqCst), 0);
}

/// Verifies non-cooperative cancellation retains the active worker scope.
#[test]
fn test_worker_cancellation_reports_still_running_with_cancellation_trigger() {
    let cancellation = RetryCancellationToken::new();
    let operation_cancellation = cancellation.clone();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let failed_observer_calls = Arc::new(AtomicUsize::new(0));
    let rule_calls = Arc::new(AtomicUsize::new(0));
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(BackoffPolicy::immediate())
            .build()
            .expect("worker cancellation policy should be valid"),
    )
    .observer(CountAttemptFailed {
        calls: Arc::clone(&failed_observer_calls),
    })
    .rule({
        let rule_calls = Arc::clone(&rule_calls);
        move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            rule_calls.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        }
    })
    .build()
    .worker()
    .cancellation_grace(Duration::ZERO)
    .cancellation_token(cancellation)
    .run({
        let operation_calls = Arc::clone(&operation_calls);
        let release_receiver = Arc::clone(&release_receiver);
        move |_: AttemptCancellationToken| {
            if operation_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(TestError("previous failure"));
            }
            operation_cancellation.cancel();
            release_receiver
                .lock()
                .expect("release receiver lock should remain valid")
                .recv()
                .expect("test should release the detached worker");
            Ok::<(), TestError>(())
        }
    })
    .expect_err("a non-cooperative cancelled worker must remain visible");
    release_sender
        .send(())
        .expect("detached test worker should receive its release");

    let RetryFailure::Infrastructure {
        failure: RetryInfrastructureFailure::WorkerStillRunning { trigger },
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a worker-still-running infrastructure failure");
    };
    assert_eq!(*trigger, WorkerStopTrigger::Cancellation);
    assert_eq!(
        last_failure.as_ref().and_then(AttemptFailure::as_error),
        Some(&TestError("previous failure"))
    );
    assert_eq!(error.context().attempts(), 2);
    assert_eq!(
        error
            .context()
            .current_attempt()
            .map(std::num::NonZeroU32::get),
        Some(2)
    );
    assert_eq!(operation_calls.load(Ordering::SeqCst), 2);
    assert_eq!(failed_observer_calls.load(Ordering::SeqCst), 1);
    assert_eq!(rule_calls.load(Ordering::SeqCst), 1);
}

/// Verifies cancellation wins when a pending backoff timer also becomes ready.
#[test]
fn test_worker_backoff_cancellation_wins_over_timer_completion() {
    let cancellation = RetryCancellationToken::new();
    let runner_cancellation = cancellation.clone();
    let operation_calls = Arc::new(AtomicUsize::new(0));
    let runner_operation_calls = Arc::clone(&operation_calls);
    let (registered_sender, registered_receiver) = std::sync::mpsc::channel();
    let timer_state = Arc::new(PendingTimerState {
        ready: AtomicBool::new(false),
        waker: Mutex::new(None),
    });
    let timer = Arc::new(PendingTimer {
        clock: ManualMonotonicClock::new_shared(),
        registered: registered_sender,
        state: Arc::clone(&timer_state),
    });
    let (result_sender, result_receiver) = std::sync::mpsc::channel();
    let runner = std::thread::spawn(move || {
        let delay = Duration::from_secs(4);
        let result = Retry::<TestError>::builder(
            RetryPolicy::builder()
                .max_attempts(2)
                .backoff(
                    BackoffPolicy::fixed(Duration::from_secs(1))
                        .prefer_retry_after(),
                )
                .build()
                .expect("backoff cancellation policy should be valid"),
        )
        .rule(move |_: &AttemptFailure<TestError>, _: &RetryContext| {
            RetryDecision::RetryWithHint(delay)
        })
        .build()
        .worker()
        .timer(timer)
        .cancellation_token(runner_cancellation)
        .run(move |_: AttemptCancellationToken| {
            runner_operation_calls.fetch_add(1, Ordering::SeqCst);
            Err::<(), _>(TestError("backoff"))
        });
        result_sender
            .send(result)
            .expect("test should receive the retry result");
    });

    registered_receiver
        .recv()
        .expect("backoff timer should be registered");
    cancellation.cancel();
    timer_state.complete();
    let error = result_receiver
        .recv()
        .expect("cancelled retry should return")
        .expect_err("backoff cancellation must terminate the retry");
    runner.join().expect("worker retry runner should not panic");

    let RetryFailure::Cancelled {
        phase,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a cancellation terminal");
    };
    assert_eq!(*phase, RetryCancellationPhase::Backoff);
    assert_eq!(
        last_failure.as_ref().and_then(AttemptFailure::as_error),
        Some(&TestError("backoff"))
    );
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().next_delay(), Some(Duration::from_secs(4)));
    assert_eq!(
        error.context().retry_after_hint(),
        Some(Duration::from_secs(4))
    );
    assert_eq!(operation_calls.load(Ordering::SeqCst), 1);
}

/// Verifies registration-time cancellation wins over the timer error.
#[test]
fn test_worker_backoff_registration_cancellation_wins_over_timer_failure() {
    let delay = Duration::from_secs(4);
    let cancellation = RetryCancellationToken::new();
    let registrations = Arc::new(AtomicUsize::new(0));
    let timer = Arc::new(CancellingFailingTimer {
        clock: ManualMonotonicClock::new_shared(),
        cancellation: cancellation.clone(),
        registrations: Arc::clone(&registrations),
    });
    let error = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .max_attempts(2)
            .backoff(
                BackoffPolicy::fixed(Duration::from_secs(1))
                    .prefer_retry_after(),
            )
            .build()
            .expect("registration cancellation policy should be valid"),
    )
    .rule(move |_: &AttemptFailure<TestError>, _: &RetryContext| {
        RetryDecision::RetryWithHint(delay)
    })
    .build()
    .worker()
    .timer(timer)
    .cancellation_token(cancellation)
    .run(|_: AttemptCancellationToken| Err::<(), _>(TestError("registration")))
    .expect_err("registration-time cancellation must stop the retry");

    assert_eq!(registrations.load(Ordering::SeqCst), 1);
    let RetryFailure::Cancelled {
        phase,
        last_failure,
        ..
    } = error.failure()
    else {
        panic!("expected a cancellation terminal");
    };
    assert_eq!(*phase, RetryCancellationPhase::Backoff);
    assert_eq!(
        last_failure.as_ref().and_then(AttemptFailure::as_error),
        Some(&TestError("registration"))
    );
    assert_eq!(error.context().attempts(), 1);
    assert_eq!(error.context().current_attempt(), None);
    assert_eq!(error.context().next_delay(), Some(delay));
    assert_eq!(error.context().retry_after_hint(), Some(delay));
}

/// Verifies a blocked worker retains the attempt-timeout stop trigger.
#[test]
fn test_worker_still_running_retains_attempt_timeout_trigger() {
    assert_blocked_worker_timeout_trigger(
        RetryTimeoutScope::Attempt,
        WorkerStopTrigger::AttemptTimeout,
    );
}

/// Verifies a blocked worker retains the flow-timeout stop trigger.
#[test]
fn test_worker_still_running_retains_flow_timeout_trigger() {
    assert_blocked_worker_timeout_trigger(
        RetryTimeoutScope::Flow,
        WorkerStopTrigger::FlowTimeout,
    );
}

/// Runs one blocked worker and checks its deterministic timeout trigger.
fn assert_blocked_worker_timeout_trigger(
    scope: RetryTimeoutScope,
    expected_trigger: WorkerStopTrigger,
) {
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let retry = Retry::<TestError>::builder(
        RetryPolicy::builder()
            .build()
            .expect("blocked-worker policy should be valid"),
    )
    .build();
    let clock = ManualMonotonicClock::new_shared();
    let worker = retry
        .worker()
        .timer(clock.new_timer())
        .cancellation_grace(Duration::ZERO);
    let worker = match scope {
        RetryTimeoutScope::Attempt => {
            worker.attempt_timeout(Duration::from_nanos(1))
        }
        RetryTimeoutScope::Flow => worker.flow_timeout(Duration::from_nanos(1)),
    };
    let error = worker
        .run({
            let release_receiver = Arc::clone(&release_receiver);
            move |_: AttemptCancellationToken| {
                release_receiver
                    .lock()
                    .expect("release receiver lock should remain valid")
                    .recv()
                    .expect("test should release the detached worker");
                Ok::<(), TestError>(())
            }
        })
        .expect_err("blocked worker must outlive its zero grace period");
    release_sender
        .send(())
        .expect("detached test worker should receive its release");

    let RetryFailure::Infrastructure {
        failure: RetryInfrastructureFailure::WorkerStillRunning { trigger },
        ..
    } = error.failure()
    else {
        panic!("expected a worker-still-running infrastructure failure");
    };
    assert_eq!(*trigger, expected_trigger);
}
