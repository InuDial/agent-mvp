use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use mvp_contract::{Capabilities, InvocationParams, OutputClassification, ToolOutcome, ToolSpec};
use mvp_kernel::error::{AuthorizationError, InputError, ToolError};
use mvp_kernel::kernel::Kernel;
use mvp_kernel::policy::{
    CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory, PolicyPlane,
};
use mvp_kernel::service::fs::CanonicalRoot;
use mvp_kernel::tool::{RegisteredTool, ToolContext, ToolImpl, ToolRegistration};
use serde_json::{Value, json};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum TestToolPath {
    Primary,
    Alias,
}

struct EnumPathKernel {
    tools: BTreeMap<TestToolPath, RegisteredTool<EnumPathKernel>>,
    policy: PolicyPlane<KernelPolicyContextFactory>,
}

impl EnumPathKernel {
    fn new() -> Self {
        let mut policy = PolicyPlane::new();
        policy.prepend_inbound(CapabilityEnvelopePolicy);
        Self {
            tools: BTreeMap::new(),
            policy,
        }
    }

    fn register<T: ToolImpl<Self>>(
        &mut self,
        path: TestToolPath,
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

struct EnumPathToolContext<'a> {
    kernel: &'a EnumPathKernel,
    tool_path: &'a TestToolPath,
    registration: &'a ToolRegistration,
    effective_capabilities: Capabilities,
    canonical_workspace_root: CanonicalRoot,
}

impl<'a> EnumPathToolContext<'a> {
    fn new(
        kernel: &'a EnumPathKernel,
        tool_path: &'a TestToolPath,
        registration: &'a ToolRegistration,
        params: &'a InvocationParams,
    ) -> Result<Self, AuthorizationError> {
        Ok(Self {
            kernel,
            tool_path,
            registration,
            effective_capabilities: params
                .capabilities_override
                .unwrap_or_else(Capabilities::empty),
            canonical_workspace_root: CanonicalRoot::existing(&params.workspace_root)?,
        })
    }
}

#[async_trait]
impl ToolContext<EnumPathKernel> for EnumPathToolContext<'_> {
    fn policy_context(&self) -> KernelPolicyContext<'_> {
        KernelPolicyContext::new(self.effective_capabilities, &self.canonical_workspace_root)
    }

    fn effective_capabilities(&self) -> Capabilities {
        self.effective_capabilities
    }

    fn tool_path(&self) -> &TestToolPath {
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
        path: TestToolPath,
        capabilities_override: Option<Capabilities>,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        let params = InvocationParams::new(
            self.workspace_root(),
            Some(capabilities_override.unwrap_or(self.effective_capabilities)),
        );
        self.kernel.invoke(&path, &params, payload).await
    }
}

#[async_trait]
impl Kernel for EnumPathKernel {
    type PolicyCxFactory = KernelPolicyContextFactory;
    type PolicyPlane<'a>
        = PolicyPlane<KernelPolicyContextFactory>
    where
        Self: 'a;
    type ToolPath = TestToolPath;
    type ToolCx<'a>
        = EnumPathToolContext<'a>
    where
        Self: 'a;

    fn policy_plane(&self) -> &Self::PolicyPlane<'_> {
        &self.policy
    }

    fn decode_tool_path(value: &Value) -> Result<Self::ToolPath, InputError> {
        match value.as_str() {
            Some("primary") => Ok(TestToolPath::Primary),
            Some("alias") => Ok(TestToolPath::Alias),
            _ => Err(InputError::InvalidField("tool_path")),
        }
    }

    async fn invoke(
        &self,
        path: &Self::ToolPath,
        params: &InvocationParams,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        let (registered_path, registered) = self
            .tools
            .get_key_value(&path)
            .ok_or_else(|| ToolError::UnknownTool(format!("{path:?}")))?;
        let ctx =
            EnumPathToolContext::new(self, registered_path, registered.registration(), params)
                .map_err(ToolError::Authorization)?;
        registered.invoke(&ctx, payload).await
    }
}

struct EchoPathTool;

#[async_trait]
impl ToolImpl<EnumPathKernel> for EchoPathTool {
    type Input = ();
    type Output = ToolOutcome;

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "same_metadata_name".into(),
            description: "Return the runtime path used for invocation.".into(),
            capabilities: Capabilities::empty(),
        }
    }

    fn parse_input(&self, _payload: Value) -> Result<Self::Input, InputError> {
        Ok(())
    }

    async fn execute(
        &self,
        ctx: &EnumPathToolContext<'_>,
        _input: Self::Input,
    ) -> Result<Self::Output, ToolError> {
        Ok(ToolOutcome {
            payload: json!({ "path": format!("{:?}", ctx.tool_path()) }),
            classification: OutputClassification::Public,
        })
    }
}

#[tokio::test]
async fn registry_uses_generic_tool_path_not_tool_spec_name() {
    let mut kernel = EnumPathKernel::new();
    kernel
        .register(TestToolPath::Primary, EchoPathTool)
        .unwrap();
    kernel.register(TestToolPath::Alias, EchoPathTool).unwrap();

    let params = InvocationParams::new(std::env::temp_dir(), None);
    let primary = kernel
        .invoke(&TestToolPath::Primary, &params, json!({}))
        .await
        .unwrap();
    let alias = kernel
        .invoke(&TestToolPath::Alias, &params, json!({}))
        .await
        .unwrap();

    assert_eq!(primary.payload["path"], "Primary");
    assert_eq!(alias.payload["path"], "Alias");
}
