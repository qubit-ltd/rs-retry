use qubit_retry::AttemptFailure;
use qubit_retry::RetryContext;
use qubit_retry::RetryDecision;
use qubit_retry::RetryRule;

struct NoopRule;

impl RetryRule<()> for NoopRule {
    fn decide(&self, _failure: &AttemptFailure<()>, _context: &RetryContext) -> RetryDecision {
        RetryDecision::UseDefault
    }
}

#[test]
fn rule_trait_accepts_function_callbacks() {
    let rule: Box<dyn RetryRule<()>> = Box::new(NoopRule);
    let _ = rule;
}
