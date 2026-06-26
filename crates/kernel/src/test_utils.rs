use crate::kernel::Kernel;
use crate::service::fs::{FsAccess, FsService, StdFs};
use crate::service::network::{NetworkAccess, NetworkService, StaticNetwork};
use crate::tool::{RegisteredTool, ToolContext, ToolImpl, ToolRegistration};
use crate::{
    audit,
    error::{AuthorizationError, ToolError},
    policy::{
        CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory, PolicyPlane,
    },
};
use async_trait::async_trait;
use mvp_contract::{Capabilities, InvocationParams, ToolName, ToolOutcome, ToolSpec};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEST_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

pub struct TempWorkspace {
    pub root: PathBuf,
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
    tools: BTreeMap<ToolName, RegisteredTool<MockKernel>>,
    fs: StdFs,
    network: StaticNetwork,
    pub policy: PolicyPlane<KernelPolicyContextFactory>,
}

impl MockKernel {
    pub fn new() -> Self {
        let mut policy = PolicyPlane::new();
        policy.prepend_inbound(CapabilityEnvelopePolicy);

        Self {
            tools: BTreeMap::new(),
            fs: StdFs,
            network: StaticNetwork::new(std::iter::empty::<(String, Vec<u8>)>()),
            policy,
        }
    }

    pub fn with_network(responses: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self {
            network: StaticNetwork::new(responses),
            ..Self::new()
        }
    }

    pub fn register<T: ToolImpl<Self>>(&mut self, tool: T) -> Result<(), ToolError> {
        let registered = RegisteredTool::from_tool(tool)?;
        let name = registered.spec().name.clone();
        if self.tools.contains_key(&name) {
            return Err(ToolError::DuplicateTool(name));
        }
        self.tools.insert(name, registered);
        Ok(())
    }
}

impl Default for MockKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl FsService for MockKernel {
    fn fs_access(&self) -> &dyn FsAccess {
        &self.fs
    }
}

impl NetworkService for MockKernel {
    fn network_access(&self) -> &dyn NetworkAccess {
        &self.network
    }
}

pub struct MockToolContext<'a> {
    kernel: &'a MockKernel,
    registration: &'a ToolRegistration,
    effective_capabilities: Capabilities,
    canonical_workspace_root: PathBuf,
}

impl<'a> MockToolContext<'a> {
    fn new(
        kernel: &'a MockKernel,
        registration: &'a ToolRegistration,
        params: &'a InvocationParams,
    ) -> Result<Self, AuthorizationError> {
        let canonical_workspace_root =
            std::fs::canonicalize(&params.workspace_root).map_err(AuthorizationError::Io)?;
        let declared_capabilities = registration.spec().capabilities;
        let effective_capabilities = match params.capabilities_override {
            Some(caps) => caps,
            None => declared_capabilities,
        };

        Ok(Self {
            kernel,
            registration,
            effective_capabilities,
            canonical_workspace_root,
        })
    }
}

#[async_trait]
impl ToolContext<MockKernel> for MockToolContext<'_> {
    fn kernel(&self) -> &MockKernel {
        self.kernel
    }

    fn policy_context(&self) -> KernelPolicyContext<'_> {
        KernelPolicyContext::new(self.effective_capabilities, self.workspace_root())
    }

    fn effective_capabilities(&self) -> Capabilities {
        self.effective_capabilities
    }

    fn registration(&self) -> &ToolRegistration {
        self.registration
    }

    fn workspace_root(&self) -> &Path {
        &self.canonical_workspace_root
    }

    async fn invoke_tool(
        &self,
        path: <MockKernel as Kernel>::ToolPath,
        capabilities_override: Option<Capabilities>,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        let (effective_capabilities, attempted_expand) = match capabilities_override {
            Some(capabilities) => {
                let attempted_expand = !self.effective_capabilities.contains(capabilities);
                if attempted_expand {
                    audit::record_nested_capability_override(
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
            self.registration,
            &path,
            self.effective_capabilities,
            capabilities_override,
            Some(effective_capabilities),
            attempted_expand,
        );

        let params = InvocationParams::new(self.workspace_root(), Some(effective_capabilities));
        self.kernel.invoke(path, &params, payload).await
    }
}

#[async_trait]
impl Kernel for MockKernel {
    type PolicyCxFactory = KernelPolicyContextFactory;
    type PolicyPlane<'a>
        = PolicyPlane<KernelPolicyContextFactory>
    where
        Self: 'a;
    type ToolPath = String;
    type ToolCx<'a>
        = MockToolContext<'a>
    where
        Self: 'a;

    fn policy_plane(&self) -> &Self::PolicyPlane<'_> {
        &self.policy
    }

    async fn invoke(
        &self,
        path: Self::ToolPath,
        params: &InvocationParams,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        let registered = self
            .tools
            .get(&path)
            .ok_or_else(|| ToolError::UnknownTool(path.clone()))?;
        let ctx = MockToolContext::new(self, registered.registration(), params)
            .map_err(ToolError::Authorization)?;
        registered.invoke(&ctx, payload).await
    }
}
