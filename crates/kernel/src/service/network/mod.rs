use crate::error::ExecutionError;
use crate::policy::{PolicyContext, PolicyEngine, PolicyPlane};

pub mod access;
pub mod action;
pub mod policy;

pub use access::{DenyNetwork, NetworkAccess, StaticNetwork};
pub use action::NetworkFetchAction;
pub use policy::{AllowDomainFetchPolicy, AllowExactUrlFetchPolicy};

/// Network sub-context exposed as `ctx.network()`.
///
/// Public methods remain natural and function-like. Internally they follow the
/// same pipeline: construct an action, ask policy to grant it, then execute the
/// granted action. Grant / execute audit stays in the shared policy/action core.
pub struct NetworkContext<'a, Ctx: PolicyContext> {
    network: &'a dyn NetworkAccess,
    policy: &'a PolicyPlane<Ctx>,
    policy_ctx: Ctx,
}

impl<'a, Ctx: PolicyContext> NetworkContext<'a, Ctx> {
    pub fn new(
        network: &'a dyn NetworkAccess,
        policy: &'a PolicyPlane<Ctx>,
        policy_ctx: Ctx,
    ) -> Self {
        Self {
            network,
            policy,
            policy_ctx,
        }
    }

    pub async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, ExecutionError> {
        let action = NetworkFetchAction::new(url.to_owned());
        let granted = self
            .policy
            .grant(&self.policy_ctx, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.network).await
    }
}

pub(crate) fn extract_host(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host_port = rest.split('/').next()?;
    let host = host_port.split(':').next()?;
    if host.is_empty() { None } else { Some(host) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AuthorizationError;
    use crate::service::fs::StdFs;
    use crate::tool::{ToolPlane, ToolPlaneContext, test_utils::*};
    use mvp_contract::{Capabilities, Capability};

    #[tokio::test]
    async fn url_fetch_grant_fetches_exact_url() {
        let ws = TempWorkspace::new();
        let network =
            StaticNetwork::new([("https://example.test/hello".to_owned(), b"hello".to_vec())]);
        let reg = Box::leak(Box::new(registration([Capability::NetworkFetch].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), network);
        plane
            .policy
            .append::<NetworkFetchAction, _>(AllowExactUrlFetchPolicy::new(
                "https://example.test/hello",
            ));
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

        let body = ctx
            .network()
            .fetch_url("https://example.test/hello")
            .await
            .unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn domain_policy_allows_matching_subdomain() {
        let ws = TempWorkspace::new();
        let network = StaticNetwork::new([(
            "https://docs.example.test/index".to_owned(),
            b"docs".to_vec(),
        )]);
        let reg = Box::leak(Box::new(registration([Capability::NetworkFetch].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), network);
        plane
            .policy
            .append::<NetworkFetchAction, _>(AllowDomainFetchPolicy::new("example.test"));
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

        let body = ctx
            .network()
            .fetch_url("https://docs.example.test/index")
            .await
            .unwrap();
        assert_eq!(body, b"docs");
    }

    #[tokio::test]
    async fn network_requires_matching_policy() {
        let ws = TempWorkspace::new();
        let network = StaticNetwork::new([]);
        let reg = Box::leak(Box::new(registration(Capabilities::empty())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let plane = ToolPlane::new(StdFs::new(), network);
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

        let denied = ctx.network().fetch_url("https://example.test/hello").await;
        assert!(matches!(
            denied,
            Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
        ));
    }
}
