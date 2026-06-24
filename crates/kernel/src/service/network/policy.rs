use async_trait::async_trait;

use crate::policy::{KernelPolicyContext, Policy, PolicyDecision};

use super::action::NetworkFetchAction;

/// Policy that only allows one exact URL.
pub struct AllowExactUrlFetchPolicy {
    url: String,
}

impl AllowExactUrlFetchPolicy {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

#[async_trait]
impl Policy<KernelPolicyContext, NetworkFetchAction> for AllowExactUrlFetchPolicy {
    fn name(&self) -> &'static str {
        "network.allow_exact_url_fetch"
    }

    async fn grant(
        &self,
        _ctx: &KernelPolicyContext,
        action: &NetworkFetchAction,
    ) -> PolicyDecision {
        if action.url == self.url {
            PolicyDecision::Allow { reason: None }
        } else {
            PolicyDecision::Abstain
        }
    }
}

/// Policy that allows URLs under a domain suffix.
pub struct AllowDomainFetchPolicy {
    domain: String,
}

impl AllowDomainFetchPolicy {
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
        }
    }
}

#[async_trait]
impl Policy<KernelPolicyContext, NetworkFetchAction> for AllowDomainFetchPolicy {
    fn name(&self) -> &'static str {
        "network.allow_domain_fetch"
    }

    async fn grant(
        &self,
        _ctx: &KernelPolicyContext,
        action: &NetworkFetchAction,
    ) -> PolicyDecision {
        match super::extract_host(&action.url) {
            Some(host) if host == self.domain || host.ends_with(&format!(".{}", self.domain)) => {
                PolicyDecision::Allow { reason: None }
            }
            _ => PolicyDecision::Abstain,
        }
    }
}
