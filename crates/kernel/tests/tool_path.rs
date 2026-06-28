use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use mvp_contract::{
    Capabilities, InvocationParams, OutputClassification, PolicyReport, ToolOutcome, ToolSpec,
};
use mvp_core::error::AuthorizationError;
use mvp_core::error::{InputError, ToolError};
use mvp_core::policy::{HasPolicyEngine, PolicyEngine};
use mvp_core::tool::ToolHost;
use mvp_core::tool::{RegisteredTool, ToolContext, ToolImpl, ToolRegistration};
use mvp_kernel::policy::{KernelPolicyContext, KernelPolicyContextFactory};
use serde_json::{Value, json};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum TestToolPath {
    Primary,
    Alias,
}

struct EnumPathKernel {
    tools: BTreeMap<TestToolPath, RegisteredTool<EnumPathKernel>>,
    policy: AllowAllEngine,
}

impl EnumPathKernel {
    fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
            policy: AllowAllEngine,
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

struct AllowAllEngine;

#[async_trait]
impl PolicyEngine<KernelPolicyContextFactory> for AllowAllEngine {
    async fn decide<A: mvp_core::action::Action>(
        &self,
        _ctx: &KernelPolicyContext<'_>,
        _action: &A,
    ) -> PolicyReport {
        PolicyReport::deny_without_match(Vec::new(), Some("No matching policy.".to_owned()))
    }
}

struct EnumPathToolContext<'a> {
    kernel: &'a EnumPathKernel,
    tool_path: &'a TestToolPath,
    registration: &'a ToolRegistration,
    effective_capabilities: Capabilities,
    canonical_workspace_root: PathBuf,
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
            canonical_workspace_root: std::fs::canonicalize(&params.workspace_root)
                .map_err(AuthorizationError::Io)?,
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
        &self.canonical_workspace_root
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

impl HasPolicyEngine for EnumPathKernel {
    type PolicyCxFactory = KernelPolicyContextFactory;
    type PolicyEngine<'a>
        = AllowAllEngine
    where
        Self: 'a;

    fn policy_engine(&self) -> &Self::PolicyEngine<'_> {
        &self.policy
    }
}

#[async_trait]
impl ToolHost for EnumPathKernel {
    type ToolPath = TestToolPath;
    type ToolCx<'a>
        = EnumPathToolContext<'a>
    where
        Self: 'a;

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
