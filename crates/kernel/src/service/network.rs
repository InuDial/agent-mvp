use async_trait::async_trait;
use mvp_contract::Capability;

use crate::tool::{next_grant_id, GrantId, ToolPlaneContext};
use crate::{audit, error::*};

mod sealed {
    pub trait SealedNetwork {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkGrantScope {
    Url { url: String },
    AnyUrl,
}

/// Grant for fetching one precise URL.
pub struct UrlFetchGrant {
    id: GrantId,
    url: String,
}

impl UrlFetchGrant {
    fn issue(url: String) -> Self {
        Self {
            id: next_grant_id(),
            url,
        }
    }

    pub fn id(&self) -> GrantId {
        self.id
    }

    fn scope(&self) -> NetworkGrantScope {
        NetworkGrantScope::Url {
            url: self.url.clone(),
        }
    }
}

/// Grant for fetching any URL allowed by the invocation policy.
pub struct AnyUrlFetchGrant {
    id: GrantId,
}

impl AnyUrlFetchGrant {
    fn issue() -> Self {
        Self {
            id: next_grant_id(),
        }
    }

    pub fn id(&self) -> GrantId {
        self.id
    }

    fn scope(&self) -> NetworkGrantScope {
        NetworkGrantScope::AnyUrl
    }
}

#[async_trait]
pub trait NetworkAccess: sealed::SealedNetwork + Send + Sync {
    async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, CapabilityError>;
}

/// Network implementation that denies all network access.
///
/// This is useful for tests and demos that only exercise filesystem tools while
/// still keeping `ToolPlane` shaped like the real kernel service container.
pub struct DenyNetwork;

impl sealed::SealedNetwork for DenyNetwork {}

#[async_trait]
impl NetworkAccess for DenyNetwork {
    async fn fetch_url(&self, _url: &str) -> Result<Vec<u8>, CapabilityError> {
        Err(CapabilityError::Denied)
    }
}

/// In-memory network implementation for deterministic tests.
pub struct StaticNetwork {
    responses: std::collections::BTreeMap<String, Vec<u8>>,
}

impl StaticNetwork {
    pub fn new(responses: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
        }
    }
}

impl sealed::SealedNetwork for StaticNetwork {}

#[async_trait]
impl NetworkAccess for StaticNetwork {
    async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, CapabilityError> {
        self.responses
            .get(url)
            .cloned()
            .ok_or(CapabilityError::Denied)
    }
}

/// Network sub-context exposed as `ctx.network()`.
///
/// Like `ctx.fs()`, this single sub-context handles both grant issuance and
/// grant consumption. Dynamic flows can issue more grants at runtime by calling
/// back into this context.
pub struct NetworkContext<'a> {
    parent: &'a ToolPlaneContext<'a>,
}

impl<'a> NetworkContext<'a> {
    /// Authorize fetching one exact URL.
    pub async fn grant_url_fetch(&self, url: &str) -> Result<UrlFetchGrant, AuthorizationError> {
        self.ensure_network_capability()?;

        let grant = UrlFetchGrant::issue(url.to_owned());
        self.audit_issued("network.fetch.url", grant.id(), grant.scope());

        Ok(grant)
    }

    /// Authorize generic network fetches.
    pub async fn grant_any_url_fetch(&self) -> Result<AnyUrlFetchGrant, AuthorizationError> {
        self.ensure_network_capability()?;

        let grant = AnyUrlFetchGrant::issue();
        self.audit_issued("network.fetch.any_url", grant.id(), grant.scope());

        Ok(grant)
    }

    /// Downgrade a broad network grant into a URL-specific grant.
    pub async fn downgrade_any_url_to_url(
        &self,
        _grant: &AnyUrlFetchGrant,
        url: &str,
    ) -> Result<UrlFetchGrant, CapabilityError> {
        Ok(UrlFetchGrant::issue(url.to_owned()))
    }

    /// Consume a precise URL fetch grant. The URL is carried by the grant.
    pub async fn fetch_url(&self, grant: &UrlFetchGrant) -> Result<Vec<u8>, CapabilityError> {
        self.audit_used("network.fetch.url", grant.id(), grant.scope(), &grant.url);
        self.parent.network.fetch_url(&grant.url).await
    }

    /// Consume a generic network grant for a specific URL.
    pub async fn fetch_any_url(
        &self,
        grant: &AnyUrlFetchGrant,
        url: &str,
    ) -> Result<Vec<u8>, CapabilityError> {
        self.audit_used("network.fetch.any_url", grant.id(), grant.scope(), url);
        self.parent.network.fetch_url(url).await
    }

    fn ensure_network_capability(&self) -> Result<(), AuthorizationError> {
        if !self
            .parent
            .registration
            .spec
            .capabilities
            .allows(Capability::NetworkFetch)
        {
            return Err(AuthorizationError::MissingCapability(
                Capability::NetworkFetch,
            ));
        }

        Ok(())
    }

    fn audit_issued(&self, grant_kind: &str, grant_id: GrantId, scope: NetworkGrantScope) {
        audit::grant_issued(
            &self.parent.registration.spec.name,
            grant_kind,
            grant_id,
            &scope,
        );
    }

    fn audit_used(&self, grant_kind: &str, grant_id: GrantId, scope: NetworkGrantScope, url: &str) {
        audit::grant_used_value(
            &self.parent.registration.spec.name,
            grant_kind,
            grant_id,
            &scope,
            url,
        );
    }
}

impl<'a> ToolPlaneContext<'a> {
    pub fn network(&'a self) -> NetworkContext<'a> {
        NetworkContext { parent: self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::fs::StdFs;
    use crate::tool::test_utils::*;

    use mvp_contract::{Capabilities, Capability};

    #[tokio::test]
    async fn url_fetch_grant_fetches_exact_url() {
        let ws = TempWorkspace::new();
        let network =
            StaticNetwork::new([("https://example.test/hello".to_owned(), b"hello".to_vec())]);
        let fs = StdFs::new();
        let reg = Box::leak(Box::new(registration([Capability::NetworkFetch].into())));
        let workspace_root = std::fs::canonicalize(&ws.root).unwrap();
        let ctx = ToolPlaneContext::new(&fs, &network, reg, workspace_root).unwrap();

        let grant = ctx
            .network()
            .grant_url_fetch("https://example.test/hello")
            .await
            .unwrap();
        let body = ctx.network().fetch_url(&grant).await.unwrap();

        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn network_requires_declared_capability() {
        let ws = TempWorkspace::new();
        let network = StaticNetwork::new([]);
        let fs = StdFs::new();
        let reg = Box::leak(Box::new(registration(Capabilities::empty())));
        let workspace_root = std::fs::canonicalize(&ws.root).unwrap();
        let ctx = ToolPlaneContext::new(&fs, &network, reg, workspace_root).unwrap();

        let denied = ctx
            .network()
            .grant_url_fetch("https://example.test/hello")
            .await;
        assert!(matches!(
            denied,
            Err(AuthorizationError::MissingCapability(
                Capability::NetworkFetch
            ))
        ));
    }
}
