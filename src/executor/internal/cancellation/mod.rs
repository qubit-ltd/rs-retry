//! Runtime-independent cancellation state.

mod retry_cancellation_state;
mod waker_registry;

pub(in crate::executor) use retry_cancellation_state::RetryCancellationState;
