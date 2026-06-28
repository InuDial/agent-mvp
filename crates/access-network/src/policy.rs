//! Network resource policies.
//!
//! Network policies are intentionally small in this MVP: exact URL matching and
//! domain suffix matching. URL parsing is simple and called out as an MVP
//! boundary in the project docs.

use async_trait::async_trait;

use mvp_contract::PolicyGrant;
use mvp_core::policy::{Policy, PolicyContextFactory};

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
impl<F> Policy<F, NetworkFetchAction> for AllowExactUrlFetchPolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "network.allow_exact_url_fetch"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, action: &NetworkFetchAction) -> PolicyGrant {
        let predicate = format!("url == allowed_url: {} == {}", action.url, self.url);

        if action.url == self.url {
            PolicyGrant::allow(Some("URL matches exact allowed URL".into()))
                .with_predicate(predicate)
        } else {
            PolicyGrant::abstain(Some("URL does not match exact allowed URL".into()))
                .with_predicate(predicate)
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
impl<F> Policy<F, NetworkFetchAction> for AllowDomainFetchPolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "network.allow_domain_fetch"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, action: &NetworkFetchAction) -> PolicyGrant {
        let host = extract_host(&action.url);
        let predicate = match host {
            Some(host) => format!(
                "host == domain || host ends_with .domain: {} == {} || {} ends_with .{}",
                host, self.domain, host, self.domain
            ),
            None => format!(
                "host == domain || host ends_with .domain: <none> == {} || <none> ends_with .{}",
                self.domain, self.domain
            ),
        };

        match host {
            Some(host) if host == self.domain || host.ends_with(&format!(".{}", self.domain)) => {
                PolicyGrant::allow(Some("URL host is under allowed domain".into()))
                    .with_predicate(predicate)
            }
            _ => PolicyGrant::abstain(Some("URL host is outside allowed domain".into()))
                .with_predicate(predicate),
        }
    }
}

fn extract_host(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host_port = rest.split('/').next()?;
    let host = host_port.split(':').next()?;
    if host.is_empty() { None } else { Some(host) }
}
