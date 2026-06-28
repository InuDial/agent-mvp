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

#[derive(Clone, Debug)]
pub struct PolicyEvaluation {
    policy_name: &'static str,
    policy_id: PolicyId,
    policy_stage: &'static str,
    grant: PolicyGrant,
}

impl PolicyEvaluation {
    pub fn new(
        policy_name: &'static str,
        policy_id: PolicyId,
        policy_stage: &'static str,
        grant: PolicyGrant,
    ) -> Self {
        Self {
            policy_name,
            policy_id,
            policy_stage,
            grant,
        }
    }

    pub fn policy_name(&self) -> &'static str {
        self.policy_name
    }

    pub fn policy_id(&self) -> PolicyId {
        self.policy_id
    }

    pub fn policy_stage(&self) -> &'static str {
        self.policy_stage
    }

    pub fn grant(&self) -> &PolicyGrant {
        &self.grant
    }
}

#[derive(Clone, Debug)]
pub enum PolicyOutcome {
    Allow {
        policy_name: &'static str,
        policy_id: PolicyId,
        reason: Option<String>,
    },
    Deny {
        source: GrantSource,
        reason: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct PolicyReport {
    evaluations: Vec<PolicyEvaluation>,
    outcome: PolicyOutcome,
}

impl PolicyReport {
    pub fn allow(
        evaluations: Vec<PolicyEvaluation>,
        policy_name: &'static str,
        policy_id: PolicyId,
        reason: Option<String>,
    ) -> Self {
        Self {
            evaluations,
            outcome: PolicyOutcome::Allow {
                policy_name,
                policy_id,
                reason,
            },
        }
    }

    pub fn deny_from_policy(
        evaluations: Vec<PolicyEvaluation>,
        policy_name: &'static str,
        policy_id: PolicyId,
        reason: Option<String>,
    ) -> Self {
        Self {
            evaluations,
            outcome: PolicyOutcome::Deny {
                source: GrantSource::Policy {
                    policy_name,
                    policy_id,
                },
                reason,
            },
        }
    }

    pub fn deny_without_match(evaluations: Vec<PolicyEvaluation>, reason: Option<String>) -> Self {
        Self {
            evaluations,
            outcome: PolicyOutcome::Deny {
                source: GrantSource::NoMatchingPolicy,
                reason,
            },
        }
    }

    pub fn evaluations(&self) -> &[PolicyEvaluation] {
        &self.evaluations
    }

    pub fn into_outcome(self) -> PolicyOutcome {
        self.outcome
    }
}
