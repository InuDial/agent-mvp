use crate::tool::GrantId;

pub type PolicyId = u64;

/// The decision returned by a policy rule.
#[derive(Clone, Debug)]
pub enum PolicyDecision {
    Allow,
    Deny,
    Abstain,
}

#[derive(Clone, Debug)]
pub struct PolicyGrant {
    decision: PolicyDecision,
    reason: Option<String>,
    predicate: Option<String>,
}

impl PolicyGrant {
    pub fn allow(reason: Option<String>) -> Self {
        Self {
            decision: PolicyDecision::Allow,
            reason,
            predicate: None,
        }
    }

    pub fn deny(reason: Option<String>) -> Self {
        Self {
            decision: PolicyDecision::Deny,
            reason,
            predicate: None,
        }
    }

    pub fn abstain(reason: Option<String>) -> Self {
        Self {
            decision: PolicyDecision::Abstain,
            reason,
            predicate: None,
        }
    }

    pub fn decision(&self) -> &PolicyDecision {
        &self.decision
    }

    pub fn with_predicate(mut self, predicate: impl Into<String>) -> Self {
        self.predicate = Some(predicate.into());
        self
    }

    pub fn into_decision_and_reason(self) -> (PolicyDecision, Option<String>) {
        (self.decision, self.reason)
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn predicate(&self) -> Option<&str> {
        self.predicate.as_deref()
    }
}

#[derive(Clone, Debug)]
pub enum GrantDecision {
    Allow(GrantId),
    Deny,
}

#[derive(Clone, Debug)]
pub enum GrantSource {
    Policy {
        policy_name: &'static str,
        policy_id: PolicyId,
    },
    NoMatchingPolicy,
}
