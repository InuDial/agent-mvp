use crate::tool::GrantId;

pub type PolicyId = u64;

/// The decision returned by a policy rule.
#[derive(Clone, Debug)]
pub enum PolicyDecision {
    Allow { reason: Option<String> },
    Deny { reason: Option<String> },
    Abstain,
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
