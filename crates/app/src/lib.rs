use std::{collections::BTreeMap, path::Path};

use async_trait::async_trait;
use mvp_access_fs::{
    AllowFileReadPrefixPolicy, AllowFileWritePrefixPolicy, AllowWorkspaceFsPolicy,
    AllowWorkspaceReadPolicy, CanonicalRoot, FsAccess, HasFsAccess, HasFsBackend,
};
use mvp_access_monty::{
    AllowMontySessionPolicy, HasMontySessionAccess, HasMontySessionStore, MontySessionAccess,
    MontySessionLoadAction, MontySessionSaveAction,
};
use mvp_access_network::{
    AllowDomainFetchPolicy, HasNetworkAccess, HasNetworkBackend, NetworkAccess,
};
use mvp_contract::{Capabilities, InvocationParams, ToolOutcome};
use mvp_core::{
    action::{Action, ActionExecutor},
    error::{AuthorizationError, ExecutionError, InputError, ToolError},
    policy::{Granted, HasPolicyEngine},
    tool::{RegisteredTool, ToolContext, ToolHost, ToolImpl, ToolRegistration},
};
use mvp_kernel::{
    audit,
    pipeline::PolicyPipeline,
    policy_context::{KernelPolicyContext, KernelPolicyContextFactory},
    runtime::KernelRuntime,
};
use mvp_tool_builtin::{double::Double, read_file::ReadFileTool, write_file::WriteFileTool};
use mvp_tool_monty::{MontyOsTool, MontyTool};
use serde_json::Value;
use tracing::Instrument;

pub struct App {
    kernel: KernelRuntime,
    tools: BTreeMap<String, RegisteredTool<App>>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            kernel: KernelRuntime::new(),
            tools: BTreeMap::new(),
        }
    }

    pub fn kernel(&self) -> &KernelRuntime {
        &self.kernel
    }

    pub fn kernel_mut(&mut self) -> &mut KernelRuntime {
        &mut self.kernel
    }

    pub fn policy(&self) -> &PolicyPipeline<KernelPolicyContextFactory> {
        &self.kernel.policy
    }

    pub fn policy_mut(&mut self) -> &mut PolicyPipeline<KernelPolicyContextFactory> {
        &mut self.kernel.policy
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

    pub async fn invoke(
        &self,
        path: &<Self as ToolHost>::ToolPath,
        params: &InvocationParams,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        <Self as ToolHost>::invoke(self, path, params, payload).await
    }
}

impl HasFsBackend for App {
    type FsBackend = <KernelRuntime as HasFsBackend>::FsBackend;

    fn fs_backend(&self) -> &Self::FsBackend {
        self.kernel.fs_backend()
    }
}

impl HasNetworkBackend for App {
    type NetworkBackend = <KernelRuntime as HasNetworkBackend>::NetworkBackend;

    fn network_backend(&self) -> &Self::NetworkBackend {
        self.kernel.network_backend()
    }
}

impl HasMontySessionStore for App {
    type MontySessionStore = <KernelRuntime as HasMontySessionStore>::MontySessionStore;

    fn monty_session_store(&self) -> &Self::MontySessionStore {
        self.kernel.monty_session_store()
    }
}

#[async_trait]
impl HasPolicyEngine for App {
    type PolicyCxFactory = KernelPolicyContextFactory;
    type PolicyEngine<'a>
        = PolicyPipeline<KernelPolicyContextFactory>
    where
        Self: 'a;

    fn policy_engine(&self) -> &Self::PolicyEngine<'_> {
        self.kernel.policy_engine()
    }

    async fn execute_granted<A, E>(
        &self,
        granted: Granted<A>,
        executor: &E,
    ) -> Result<E::Output, ExecutionError>
    where
        Self: Sized,
        A: Action,
        E: ActionExecutor<A> + ?Sized,
    {
        self.kernel.execute_granted(granted, executor).await
    }
}

pub struct AppToolContext<'a> {
    app: &'a App,
    tool_path: &'a <App as ToolHost>::ToolPath,
    registration: &'a ToolRegistration,
    effective_capabilities: Capabilities,
    canonical_workspace_root: CanonicalRoot,
}

impl<'a> AppToolContext<'a> {
    fn new(
        app: &'a App,
        tool_path: &'a <App as ToolHost>::ToolPath,
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
            app,
            tool_path,
            registration,
            effective_capabilities,
            canonical_workspace_root,
        })
    }
}

#[async_trait]
impl ToolContext<App> for AppToolContext<'_> {
    fn policy_context(&self) -> KernelPolicyContext<'_> {
        KernelPolicyContext::new(
            self.effective_capabilities,
            self.canonical_workspace_root.as_path(),
        )
    }

    fn effective_capabilities(&self) -> Capabilities {
        self.effective_capabilities
    }

    fn tool_path(&self) -> &<App as ToolHost>::ToolPath {
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
        path: <App as ToolHost>::ToolPath,
        capabilities_override: Option<Capabilities>,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        let effective_capabilities = match capabilities_override {
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
                capabilities
            }
            None => self.effective_capabilities,
        };

        audit::record_nested_capability_override(
            self.tool_path,
            self.registration,
            &path,
            self.effective_capabilities,
            capabilities_override,
            Some(effective_capabilities),
            false,
        );

        let params = InvocationParams::new(self.workspace_root(), Some(effective_capabilities));
        self.app.invoke(&path, &params, payload).await
    }
}

impl HasFsAccess<App> for AppToolContext<'_> {
    fn fs(&self) -> FsAccess<'_, App> {
        FsAccess::new(self.app, self.workspace_root(), self.policy_context())
    }
}

impl HasNetworkAccess<App> for AppToolContext<'_> {
    fn network(&self) -> NetworkAccess<'_, App> {
        NetworkAccess::new(self.app, self.policy_context())
    }
}

impl HasMontySessionAccess<App> for AppToolContext<'_> {
    fn monty_sessions(&self) -> MontySessionAccess<'_, App> {
        MontySessionAccess::new(self.app, self.workspace_root(), self.policy_context())
    }
}

#[async_trait]
impl ToolHost for App {
    type ToolPath = String;
    type ToolCx<'a>
        = AppToolContext<'a>
    where
        Self: 'a;

    fn decode_tool_path(value: &Value) -> Result<Self::ToolPath, InputError> {
        value
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or(InputError::InvalidField("tool_path"))
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
        let registration = registered.registration();
        let ctx = AppToolContext::new(self, registered_path, registration, params)
            .map_err(ToolError::Authorization)?;

        audit::record_tool_capabilities_override(
            registered_path,
            registration,
            registration.spec().capabilities,
            ctx.effective_capabilities(),
        );

        registered
            .invoke(&ctx, payload)
            .instrument(audit::execution_span())
            .instrument(audit::tool_invocation_span(registered_path, registration))
            .await
    }
}

pub fn register_default_tool<T>(
    app: &mut App,
    path: impl Into<String>,
    tool: T,
) -> Result<(), ToolError>
where
    T: ToolImpl<App>,
{
    app.register(path.into(), tool)
}

pub fn new_default_app() -> Result<App, ToolError> {
    let mut app = App::new();
    register_builtin_tools(&mut app)?;
    register_monty_tools(&mut app)?;
    allow_workspace_defaults(&mut app);
    Ok(app)
}

pub fn register_builtin_tools(app: &mut App) -> Result<(), ToolError> {
    app.register("read_file".to_owned(), ReadFileTool)?;
    app.register("write_file".to_owned(), WriteFileTool)?;
    app.register("double".to_owned(), Double)
}

pub fn register_monty_tools(app: &mut App) -> Result<(), ToolError> {
    app.register(
        "monty".to_owned(),
        MontyTool::new().expose("read_file", "read_file"),
    )?;
    app.register("monty_os".to_owned(), MontyOsTool)
}

pub fn allow_workspace_defaults(app: &mut App) {
    app.policy_mut().append(AllowWorkspaceFsPolicy);
    app.policy_mut().append(AllowWorkspaceReadPolicy);
    app.policy_mut()
        .append::<MontySessionLoadAction, _>(AllowMontySessionPolicy);
    app.policy_mut()
        .append::<MontySessionSaveAction, _>(AllowMontySessionPolicy);
}

pub fn allow_fs_read_prefix(app: &mut App, prefix: impl Into<std::path::PathBuf>) {
    app.policy_mut()
        .append(AllowFileReadPrefixPolicy::new(prefix));
}

pub fn allow_fs_write_prefix(app: &mut App, prefix: impl Into<std::path::PathBuf>) {
    app.policy_mut()
        .append(AllowFileWritePrefixPolicy::new(prefix));
}

pub fn allow_network_domain(app: &mut App, domain: impl Into<String>) {
    app.policy_mut().append(AllowDomainFetchPolicy::new(domain));
}

pub fn allow_action<A, P>(app: &mut App, policy: P)
where
    A: Action,
    P: mvp_core::policy::Policy<KernelPolicyContextFactory, A> + 'static,
{
    app.policy_mut().append::<A, P>(policy);
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use mvp_access_fs::{AllowWorkspaceFsPolicy, AllowWorkspaceReadPolicy, FsBackend};
    use mvp_contract::{
        Capabilities, Capability, InvocationParams, OutputClassification, ToolOutcome, ToolSpec,
    };
    use mvp_core::{
        error::{ExecutionError, InputError, ToolError},
        tool::{ToolHost, ToolImpl},
    };
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    static NEXT_TEST_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

    struct TempWorkspace {
        root: PathBuf,
    }

    impl TempWorkspace {
        fn new(prefix: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "mvp-app-test-{}-{}-{}-{}",
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

    struct NoopTool;

    #[async_trait]
    impl<H> ToolImpl<H> for NoopTool
    where
        H: ToolHost,
    {
        type Input = ();
        type Output = ToolOutcome;

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "noop".into(),
                description: "Return a public no-op outcome.".into(),
                capabilities: Capabilities::empty(),
            }
        }

        fn parse_input(&self, _payload: Value) -> Result<Self::Input, InputError> {
            Ok(())
        }

        async fn execute(
            &self,
            _ctx: &H::ToolCx<'_>,
            _input: Self::Input,
        ) -> Result<Self::Output, ToolError> {
            Ok(ToolOutcome {
                payload: json!({ "ok": true }),
                classification: OutputClassification::Public,
            })
        }
    }

    struct ReadWorkspaceFileTool;

    #[async_trait]
    impl<H> ToolImpl<H> for ReadWorkspaceFileTool
    where
        H: ToolHost + FsBackend,
        for<'a> H::ToolCx<'a>: HasFsAccess<H>,
    {
        type Input = String;
        type Output = ToolOutcome;

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "read_workspace_file".into(),
                description: "Read a workspace file through fs access.".into(),
                capabilities: [Capability::FsRead].into(),
            }
        }

        fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError> {
            payload
                .get("path")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or(InputError::MissingField("path"))
        }

        async fn execute(
            &self,
            ctx: &H::ToolCx<'_>,
            input: Self::Input,
        ) -> Result<Self::Output, ToolError> {
            let content = ctx
                .fs()
                .read_file(&input)
                .await
                .map_err(ToolError::Execution)?;

            Ok(ToolOutcome {
                payload: json!({ "content": content }),
                classification: OutputClassification::WorkspaceLocal,
            })
        }
    }

    #[test]
    fn register_rejects_duplicate_tool_names() {
        let mut app = App::new();
        app.register("noop".to_owned(), NoopTool).unwrap();

        let duplicate = app.register("noop".to_owned(), NoopTool);
        assert!(matches!(duplicate, Err(ToolError::DuplicateTool(path)) if path == "\"noop\""));
    }

    #[tokio::test]
    async fn invoke_returns_unknown_tool_for_unregistered_name() {
        let app = App::new();
        let ws = TempWorkspace::new("unknown-tool");

        let err = app
            .invoke(
                &"missing".to_string(),
                &InvocationParams::new(&ws.root, None),
                json!({}),
            )
            .await;

        assert!(matches!(err, Err(ToolError::UnknownTool(path)) if path == "\"missing\""));
    }

    #[tokio::test]
    async fn declared_capabilities_apply_when_override_is_absent() {
        let ws = TempWorkspace::new("default-capabilities");
        std::fs::write(ws.root.join("hello.txt"), "hello from app").unwrap();

        let mut app = App::new();
        app.register("read_workspace_file".to_owned(), ReadWorkspaceFileTool)
            .unwrap();
        app.policy_mut().append(AllowWorkspaceFsPolicy);
        app.policy_mut().append(AllowWorkspaceReadPolicy);

        let outcome = app
            .invoke(
                &"read_workspace_file".to_string(),
                &InvocationParams::new(&ws.root, None),
                json!({ "path": "hello.txt" }),
            )
            .await
            .unwrap();

        assert_eq!(outcome.payload["content"], "hello from app");
        assert_eq!(outcome.classification, OutputClassification::WorkspaceLocal);
    }

    #[tokio::test]
    async fn top_level_override_can_shrink_effective_capabilities() {
        let ws = TempWorkspace::new("capability-shrink");
        std::fs::write(ws.root.join("hello.txt"), "blocked by envelope").unwrap();

        let mut app = App::new();
        app.register("read_workspace_file".to_owned(), ReadWorkspaceFileTool)
            .unwrap();
        app.policy_mut().append(AllowWorkspaceFsPolicy);
        app.policy_mut().append(AllowWorkspaceReadPolicy);

        let err = app
            .invoke(
                &"read_workspace_file".to_string(),
                &InvocationParams::new(&ws.root, Some(Capabilities::empty())),
                json!({ "path": "hello.txt" }),
            )
            .await
            .unwrap_err();

        let ToolError::Execution(ExecutionError::Authorization(error)) = err else {
            panic!("expected authorization error");
        };
        assert_eq!(
            error.denied_reason(),
            Some("action exceeds declared capability envelope")
        );
    }
}
