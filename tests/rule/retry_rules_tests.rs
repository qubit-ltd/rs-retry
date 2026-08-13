use qubit_retry::AttemptFailure;
use qubit_retry::Retry;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryPolicy;
use qubit_retry::RetryRule;

struct NoopRule;

impl RetryRule<()> for NoopRule {
    fn decide(&self, _failure: &AttemptFailure<()>, _context: &RetryContext) -> RetryDecision {
        RetryDecision::UseDefault
    }
}

#[test]
fn retry_builder_accepts_ordered_rules() {
    let policy = RetryPolicy::builder().build().unwrap();
    let retry = Retry::<()>::builder(policy).rule(NoopRule).build();
    let _ = retry;
}
