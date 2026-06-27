use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use mvp_contract::{Capabilities, InvocationParams, ToolOutcome};
use mvp_kernel::{
    audit,
    error::{AuthorizationError, InputError, ToolError},
    kernel::Kernel,
    policy::{
        CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory, PolicyPlane,
    },
    tool::{RegisteredTool, ToolContext, ToolImpl, ToolRegistration},
};
use mvp_service_fs::{CanonicalRoot, FsService, HasFsBackend, HasFsService, StdFsBackend};
use mvp_service_monty::{
    AllowMontySessionPolicy, HasMontySessionService, HasMontySessionStore, MemoryMontySessionStore,
    MontySessionLoadAction, MontySessionSaveAction, MontySessionService,
};
use mvp_service_network::{
    DenyNetworkBackend, HasNetworkBackend, HasNetworkService, NetworkService,
};
use serde_json::Value;

pub struct App {
    tools: BTreeMap<String, RegisteredTool<App>>,
    fs: StdFsBackend,
    network: DenyNetworkBackend,
    monty_sessions: MemoryMontySessionStore,
    pub policy: PolicyPlane<KernelPolicyContextFactory>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let mut policy = PolicyPlane::new();
        policy.prepend_inbound(CapabilityEnvelopePolicy);
        policy.append::<MontySessionLoadAction, _>(AllowMontySessionPolicy);
        policy.append::<MontySessionSaveAction, _>(AllowMontySessionPolicy);

        Self {
            tools: BTreeMap::new(),
            fs: StdFsBackend,
            network: DenyNetworkBackend,
            monty_sessions: MemoryMontySessionStore::new(),
            policy,
        }
    }

    pub fn register<T: ToolImpl<Self>>(
        &mut self,
        path: <Self as Kernel>::ToolPath,
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

impl HasFsBackend for App {
    type FsBackend = StdFsBackend;

    fn fs_backend(&self) -> &Self::FsBackend {
        &self.fs
    }
}

impl HasNetworkBackend for App {
    type NetworkBackend = DenyNetworkBackend;

    fn network_backend(&self) -> &Self::NetworkBackend {
        &self.network
    }
}

impl HasMontySessionStore for App {
    type MontySessionStore = MemoryMontySessionStore;

    fn monty_session_store(&self) -> &Self::MontySessionStore {
        &self.monty_sessions
    }
}

pub struct AppToolContext<'a> {
    app: &'a App,
    tool_path: &'a <App as Kernel>::ToolPath,
    registration: &'a ToolRegistration,
    effective_capabilities: Capabilities,
    canonical_workspace_root: CanonicalRoot,
}

impl<'a> AppToolContext<'a> {
    fn new(
        app: &'a App,
        tool_path: &'a <App as Kernel>::ToolPath,
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
    fn tool_path(&self) -> &<App as Kernel>::ToolPath {
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
        path: <App as Kernel>::ToolPath,
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

impl HasFsService<App> for AppToolContext<'_> {
    fn fs(&self) -> FsService<'_, App> {
        FsService::new(self.app, self.workspace_root(), self.policy_context())
    }
}

impl HasNetworkService<App> for AppToolContext<'_> {
    fn network(&self) -> NetworkService<'_, App> {
        NetworkService::new(self.app, self.policy_context())
    }
}

impl HasMontySessionService<App> for AppToolContext<'_> {
    fn monty_sessions(&self) -> MontySessionService<'_, App> {
        MontySessionService::new(self.app, self.workspace_root(), self.policy_context())
    }
}

#[async_trait]
impl Kernel for App {
    type PolicyCxFactory = KernelPolicyContextFactory;
    type PolicyPlane<'a>
        = PolicyPlane<KernelPolicyContextFactory>
    where
        Self: 'a;

    type ToolPath = String;
    type ToolCx<'a>
        = AppToolContext<'a>
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

        registered.invoke(&ctx, payload).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use async_trait::async_trait;
    use mvp_contract::{Capabilities, Capability, OutputClassification, ToolSpec};
    use mvp_kernel::error::{AuthorizationError, ExecutionError, InputError};
    use mvp_service_fs::{AllowWorkspaceFsPolicy, AllowWorkspaceReadPolicy, FsBackend};
    use serde_json::{Value, json};

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
    impl<K> ToolImpl<K> for NoopTool
    where
        K: Kernel,
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
            _ctx: &K::ToolCx<'_>,
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
    impl<K> ToolImpl<K> for ReadWorkspaceFileTool
    where
        K: Kernel + FsBackend,
        for<'a> K::ToolCx<'a>: HasFsService<K>,
    {
        type Input = String;
        type Output = ToolOutcome;

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "read_workspace_file".into(),
                description: "Read a workspace file through the fs service.".into(),
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
            ctx: &K::ToolCx<'_>,
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
        app.policy.append(AllowWorkspaceFsPolicy);
        app.policy.append(AllowWorkspaceReadPolicy);

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
        app.policy.append(AllowWorkspaceFsPolicy);
        app.policy.append(AllowWorkspaceReadPolicy);

        let err = app
            .invoke(
                &"read_workspace_file".to_string(),
                &InvocationParams::new(&ws.root, Some(Capabilities::empty())),
                json!({ "path": "hello.txt" }),
            )
            .await;

        assert!(matches!(
            err,
            Err(ToolError::Execution(ExecutionError::Authorization(
                AuthorizationError::Denied(reason)
            ))) if reason == "action exceeds declared capability envelope"
        ));
    }
}
