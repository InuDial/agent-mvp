use async_trait::async_trait;
use mvp_contract::{Capabilities, Capability, InvocationParams, ToolOutcome, ToolSpec};
use mvp_kernel::error::{AuthorizationError, ExecutionError, InputError, ToolError};
use mvp_kernel::kernel::Kernel;
use mvp_kernel::policy::{
    CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory, PolicyPlane,
};
use mvp_kernel::tool::{ToolContext, ToolRegistration};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::*;

static NEXT_TEST_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "mvp-service-network-test-{}-{}-{}",
            std::process::id(),
            NEXT_TEST_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn registration(capabilities: Capabilities) -> ToolRegistration {
    ToolRegistration::new(ToolSpec {
        name: "test_tool".into(),
        description: "A tool for tests.".into(),
        capabilities,
    })
    .unwrap()
}

struct UnusedToolContext<'a> {
    kernel: &'a TestKernel,
    tool_path: String,
    registration: &'a ToolRegistration,
    effective_capabilities: Capabilities,
    workspace_root: PathBuf,
}

#[async_trait]
impl ToolContext<TestKernel> for UnusedToolContext<'_> {
    fn policy_context(&self) -> KernelPolicyContext<'_> {
        KernelPolicyContext::new(self.effective_capabilities, &self.workspace_root)
    }

    fn registration(&self) -> &ToolRegistration {
        self.registration
    }

    fn tool_path(&self) -> &<TestKernel as Kernel>::ToolPath {
        &self.tool_path
    }

    fn effective_capabilities(&self) -> Capabilities {
        self.effective_capabilities
    }

    fn workspace_root(&self) -> &Path {
        &self.workspace_root
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
    type NetworkBackend = StaticNetworkBackend;

    fn network_backend(&self) -> &Self::NetworkBackend {
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

    fn decode_tool_path(value: &Value) -> Result<Self::ToolPath, InputError> {
        value
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or(InputError::InvalidField("tool_path"))
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
    registration: &'a ToolRegistration,
    params: &'a InvocationParams,
    effective_capabilities: Capabilities,
) -> UnusedToolContext<'a> {
    UnusedToolContext {
        kernel,
        tool_path: "test_tool".to_owned(),
        registration,
        effective_capabilities,
        workspace_root: std::fs::canonicalize(&params.workspace_root).unwrap(),
    }
}

#[tokio::test]
async fn url_fetch_grant_fetches_exact_url() {
    let ws = TempWorkspace::new();
    let network =
        StaticNetworkBackend::new([("https://example.test/hello".to_owned(), b"hello".to_vec())]);
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
    let ctx = tool_ctx(&kernel, &registration, &params, Capabilities::empty());

    let denied = ctx.network().fetch_url("https://example.test/hello").await;
    assert!(matches!(
        denied,
        Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
    ));
}
