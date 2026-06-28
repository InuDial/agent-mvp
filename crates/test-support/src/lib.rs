use async_trait::async_trait;
use mvp_access_fs::{
    AllowWorkspaceFsPolicy, CanonicalRoot, FsAccess, FsAction, HasFsAccess, HasFsBackend,
    StdFsBackend,
};
use mvp_access_network::{
    HasNetworkAccess, HasNetworkBackend, NetworkAccess, StaticNetworkBackend,
};
use mvp_contract::{Capabilities, InvocationParams, ToolOutcome, ToolSpec};
use mvp_core::tool::{RegisteredTool, ToolContext, ToolImpl, ToolRegistration};
use mvp_core::{
    error::{AuthorizationError, ToolError},
    policy::HasPolicyEngine,
    tool::ToolHost,
};
use mvp_kernel::audit;
use mvp_kernel::policy::{
    CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory, PolicyPipeline,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub type TestPolicyPipeline<F> = PolicyPipeline<F>;

static NEXT_TEST_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

pub struct TempWorkspace {
    pub root: PathBuf,
}

impl Default for TempWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl TempWorkspace {
    pub fn new() -> Self {
        Self::with_prefix("kernel-test")
    }

    pub fn with_prefix(prefix: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "tool-plane-{}-{}-{}-{}",
            prefix,
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

#[allow(dead_code)]
pub fn registration(capabilities: Capabilities) -> ToolRegistration {
    ToolRegistration::new(ToolSpec {
        name: "test_tool".into(),
        description: "A tool for tests.".into(),
        capabilities,
    })
    .unwrap()
}

pub struct MockKernel {
    tools: BTreeMap<String, RegisteredTool<MockKernel>>,
    fs: StdFsBackend,
    network: StaticNetworkBackend,
    pub policy: TestPolicyPipeline<KernelPolicyContextFactory>,
}

impl MockKernel {
    pub fn new() -> Self {
        let mut policy = TestPolicyPipeline::new();
        policy.prepend_inbound(CapabilityEnvelopePolicy);
        policy.append::<FsAction, _>(AllowWorkspaceFsPolicy);

        Self {
            tools: BTreeMap::new(),
            fs: StdFsBackend,
            network: StaticNetworkBackend::new(std::iter::empty::<(String, Vec<u8>)>()),
            policy,
        }
    }

    pub fn with_network(responses: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self {
            network: StaticNetworkBackend::new(responses),
            ..Self::new()
        }
    }

    pub fn register<T: ToolImpl<Self>>(
        &mut self,
        path: <Self as ToolHost>::ToolPath,
        tool: T,
    ) -> Result<(), ToolError> {
        if self.tools.contains_key(&path) {
            return Err(ToolError::DuplicateTool(format!("{path:?}")));
        }

        let registered = RegisteredTool::from_tool(tool)?;
        self.tools.insert(path, registered);
        Ok(())
    }
}

impl Default for MockKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl HasFsBackend for MockKernel {
    type FsBackend = StdFsBackend;

    fn fs_backend(&self) -> &Self::FsBackend {
        &self.fs
    }
}

impl HasNetworkBackend for MockKernel {
    type NetworkBackend = StaticNetworkBackend;

    fn network_backend(&self) -> &Self::NetworkBackend {
        &self.network
    }
}

pub struct MockToolContext<'a> {
    kernel: &'a MockKernel,
    tool_path: &'a <MockKernel as ToolHost>::ToolPath,
    registration: &'a ToolRegistration,
    effective_capabilities: Capabilities,
    canonical_workspace_root: CanonicalRoot,
}

impl<'a> MockToolContext<'a> {
    fn new(
        kernel: &'a MockKernel,
        tool_path: &'a <MockKernel as ToolHost>::ToolPath,
        registration: &'a ToolRegistration,
        params: &'a InvocationParams,
    ) -> Result<Self, AuthorizationError> {
        let canonical_workspace_root = CanonicalRoot::existing(&params.workspace_root)?;
        let declared_capabilities = registration.spec().capabilities;
        let effective_capabilities = match params.capabilities_override {
            Some(caps) => caps,
            None => declared_capabilities,
        };

        Ok(Self {
            kernel,
            tool_path,
            registration,
            effective_capabilities,
            canonical_workspace_root,
        })
    }
}

#[async_trait]
impl ToolContext<MockKernel> for MockToolContext<'_> {
    fn policy_context(&self) -> KernelPolicyContext<'_> {
        KernelPolicyContext::new(
            self.effective_capabilities,
            self.canonical_workspace_root.as_path(),
        )
    }

    fn effective_capabilities(&self) -> Capabilities {
        self.effective_capabilities
    }

    fn tool_path(&self) -> &<MockKernel as ToolHost>::ToolPath {
        self.tool_path
    }

    fn registration(&self) -> &ToolRegistration {
        self.registration
    }

    fn workspace_root(&self) -> &Path {
        self.canonical_workspace_root.as_path()
    }

    async fn invoke_tool(
        &self,
        path: <MockKernel as ToolHost>::ToolPath,
        capabilities_override: Option<Capabilities>,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        let (effective_capabilities, attempted_expand) = match capabilities_override {
            Some(capabilities) => {
                let attempted_expand = !self.effective_capabilities.contains(capabilities);
                if attempted_expand {
                    audit::record_nested_capability_override(
                        self.tool_path,
                        self.registration,
                        &path,
                        self.effective_capabilities,
                        Some(capabilities),
                        None,
                        true,
                    );
                    return Err(ToolError::Authorization(AuthorizationError::Denied(
                        "nested invocation attempted to expand capabilities".into(),
                    )));
                }
                (capabilities, false)
            }
            None => (self.effective_capabilities, false),
        };

        audit::record_nested_capability_override(
            self.tool_path,
            self.registration,
            &path,
            self.effective_capabilities,
            capabilities_override,
            Some(effective_capabilities),
            attempted_expand,
        );

        let params = InvocationParams::new(self.workspace_root(), Some(effective_capabilities));
        self.kernel.invoke(&path, &params, payload).await
    }
}

impl HasFsAccess<MockKernel> for MockToolContext<'_> {
    fn fs(&self) -> FsAccess<'_, MockKernel> {
        FsAccess::new(self.kernel, self.workspace_root(), self.policy_context())
    }
}

impl HasNetworkAccess<MockKernel> for MockToolContext<'_> {
    fn network(&self) -> NetworkAccess<'_, MockKernel> {
        NetworkAccess::new(self.kernel, self.policy_context())
    }
}

impl HasPolicyEngine for MockKernel {
    type PolicyCxFactory = KernelPolicyContextFactory;
    type PolicyEngine<'a>
        = TestPolicyPipeline<KernelPolicyContextFactory>
    where
        Self: 'a;

    fn policy_engine(&self) -> &Self::PolicyEngine<'_> {
        &self.policy
    }
}

#[async_trait]
impl ToolHost for MockKernel {
    type ToolPath = String;
    type ToolCx<'a>
        = MockToolContext<'a>
    where
        Self: 'a;

    fn decode_tool_path(value: &Value) -> Result<Self::ToolPath, mvp_core::error::InputError> {
        value
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or(mvp_core::error::InputError::InvalidField("tool_path"))
    }

    async fn invoke(
        &self,
        path: &Self::ToolPath,
        params: &InvocationParams,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        let (registered_path, registered) = self
            .tools
            .get_key_value(path)
            .ok_or_else(|| ToolError::UnknownTool(format!("{path:?}")))?;
        let ctx = MockToolContext::new(self, registered_path, registered.registration(), params)
            .map_err(ToolError::Authorization)?;
        registered.invoke(&ctx, payload).await
    }
}
