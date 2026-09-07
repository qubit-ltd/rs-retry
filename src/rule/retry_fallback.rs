/// Default action when every retry rule delegates an application error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RetryFallback {
    /// Stop after the first unclassified application error.
    #[default]
    Abort,
    /// Retry unclassified application errors using the policy backoff.
    Retry,
}
