//! Network service facade.
//!
//! The network facade exposes URL fetches to tools while preserving the shared
//! action -> policy -> grant -> execute pipeline used by other services.

use crate::error::ExecutionError;
use crate::kernel::{Kernel, PolicyContextFor};
use crate::policy::PolicyEngine;
use crate::tool::ToolContext;

pub mod action;
pub mod backend;
pub mod policy;

pub use action::NetworkFetchAction;
pub use backend::{DenyNetworkBackend, NetworkBackend, StaticNetworkBackend};
pub use policy::{AllowDomainFetchPolicy, AllowExactUrlFetchPolicy};

pub trait HasNetworkBackend {
    fn network_backend(&self) -> &dyn NetworkBackend;
}

pub trait HasNetworkService<K>: ToolContext<K>
where
    K: HasNetworkBackend + Kernel,
{
    fn network(&self) -> NetworkService<'_, K>;
}

/// Network service facade exposed as `ctx.network()`.
///
/// Public methods remain natural and function-like. Internally they follow the
/// same pipeline: construct an action, ask policy to grant it, then execute the
/// granted action. Grant / execute audit stays in the shared policy/action core.
pub struct NetworkService<'a, K>
where
    K: Kernel + HasNetworkBackend + ?Sized,
{
    kernel: &'a K,
    policy_context: PolicyContextFor<'a, K>,
}

impl<'a, K> NetworkService<'a, K>
where
    K: Kernel + HasNetworkBackend,
{
    pub fn new(kernel: &'a K, policy_context: PolicyContextFor<'a, K>) -> Self {
        Self {
            kernel,
            policy_context,
        }
    }

    pub async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, ExecutionError> {
        let action = NetworkFetchAction::new(url.to_owned());
        let granted = self
            .kernel
            .policy_plane()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.kernel.network_backend()).await
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
    use crate::error::{AuthorizationError, ToolError};
    use crate::kernel::Kernel;
    use crate::policy::{
        CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory, PolicyPlane,
    };
    use crate::service::fs::CanonicalRoot;
    use crate::test_utils::{TempWorkspace, registration};
    use crate::tool::ToolContext;
    use async_trait::async_trait;
    use mvp_contract::{Capabilities, Capability, InvocationParams, ToolOutcome};
    use serde_json::Value;

    struct UnusedToolContext<'a> {
        kernel: &'a TestKernel,
        tool_path: String,
        registration: &'a crate::tool::ToolRegistration,
        effective_capabilities: Capabilities,
        workspace_root: CanonicalRoot,
    }

    #[async_trait]
    impl ToolContext<TestKernel> for UnusedToolContext<'_> {
        fn policy_context(&self) -> KernelPolicyContext<'_> {
            KernelPolicyContext::new(self.effective_capabilities, &self.workspace_root)
        }

        fn registration(&self) -> &crate::tool::ToolRegistration {
            self.registration
        }

        fn tool_path(&self) -> &<TestKernel as Kernel>::ToolPath {
            &self.tool_path
        }

        fn effective_capabilities(&self) -> Capabilities {
            self.effective_capabilities
        }

        fn workspace_root(&self) -> &std::path::Path {
            self.workspace_root.as_path()
        }

        async fn invoke_tool(
            &self,
            _path: <TestKernel as Kernel>::ToolPath,
            _capabilities_override: Option<Capabilities>,
            _payload: Value,
        ) -> Result<ToolOutcome, ToolError> {
            panic!("unused in network service tests")
        }
    }

    impl HasNetworkService<TestKernel> for UnusedToolContext<'_> {
        fn network(&self) -> NetworkService<'_, TestKernel> {
            NetworkService::new(self.kernel, self.policy_context())
        }
    }

    struct TestKernel {
        network: StaticNetworkBackend,
        policy: PolicyPlane<KernelPolicyContextFactory>,
    }

    impl TestKernel {
        fn new(network: StaticNetworkBackend) -> Self {
            let mut policy = PolicyPlane::new();
            policy.prepend_inbound(CapabilityEnvelopePolicy);
            Self { network, policy }
        }
    }

    impl HasNetworkBackend for TestKernel {
        fn network_backend(&self) -> &dyn NetworkBackend {
            &self.network
        }
    }

    #[async_trait]
    impl Kernel for TestKernel {
        type PolicyCxFactory = KernelPolicyContextFactory;
        type PolicyPlane<'a>
            = PolicyPlane<KernelPolicyContextFactory>
        where
            Self: 'a;

        type ToolPath = String;
        type ToolCx<'a>
            = UnusedToolContext<'a>
        where
            Self: 'a;

        fn policy_plane(&self) -> &Self::PolicyPlane<'_> {
            &self.policy
        }

        fn decode_tool_path(value: &Value) -> Result<Self::ToolPath, crate::error::InputError> {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or(crate::error::InputError::InvalidField("tool_path"))
        }

        async fn invoke(
            &self,
            _path: &Self::ToolPath,
            _params: &InvocationParams,
            _payload: Value,
        ) -> Result<ToolOutcome, ToolError> {
            panic!("unused in network service tests")
        }
    }

    fn tool_ctx<'a>(
        kernel: &'a TestKernel,
        registration: &'a crate::tool::ToolRegistration,
        params: &'a InvocationParams,
        effective_capabilities: Capabilities,
        _workspace_root: &'a std::path::Path,
    ) -> UnusedToolContext<'a> {
        UnusedToolContext {
            kernel,
            tool_path: "test_tool".to_owned(),
            registration,
            effective_capabilities,
            workspace_root: CanonicalRoot::existing(&params.workspace_root).unwrap(),
        }
    }

    #[tokio::test]
    async fn url_fetch_grant_fetches_exact_url() {
        let ws = TempWorkspace::new();
        let network = StaticNetworkBackend::new([(
            "https://example.test/hello".to_owned(),
            b"hello".to_vec(),
        )]);
        let mut kernel = TestKernel::new(network);
        kernel
            .policy
            .append::<NetworkFetchAction, _>(AllowExactUrlFetchPolicy::new(
                "https://example.test/hello",
            ));
        let registration = registration([Capability::NetworkFetch].into());
        let params = InvocationParams::new(&ws.root, None);
        let ctx = tool_ctx(
            &kernel,
            &registration,
            &params,
            [Capability::NetworkFetch].into(),
            &ws.root,
        );

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
        let network = StaticNetworkBackend::new([(
            "https://docs.example.test/index".to_owned(),
            b"docs".to_vec(),
        )]);
        let mut kernel = TestKernel::new(network);
        kernel
            .policy
            .append::<NetworkFetchAction, _>(AllowDomainFetchPolicy::new("example.test"));
        let registration = registration([Capability::NetworkFetch].into());
        let params = InvocationParams::new(&ws.root, None);
        let ctx = tool_ctx(
            &kernel,
            &registration,
            &params,
            [Capability::NetworkFetch].into(),
            &ws.root,
        );

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
        let network = StaticNetworkBackend::new([]);
        let kernel = TestKernel::new(network);
        let registration = registration(Capabilities::empty());
        let params = InvocationParams::new(&ws.root, None);
        let ctx = tool_ctx(
            &kernel,
            &registration,
            &params,
            Capabilities::empty(),
            &ws.root,
        );

        let denied = ctx.network().fetch_url("https://example.test/hello").await;
        assert!(matches!(
            denied,
            Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
        ));
    }
}
