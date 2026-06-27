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
            "mvp-service-fs-test-{}-{}-{}",
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
    workspace_root: CanonicalRoot,
}

#[async_trait]
impl ToolContext<TestKernel> for UnusedToolContext<'_> {
    fn policy_context(&self) -> KernelPolicyContext<'_> {
        KernelPolicyContext::new(self.effective_capabilities, self.workspace_root.as_path())
    }

    fn effective_capabilities(&self) -> Capabilities {
        self.effective_capabilities
    }

    fn tool_path(&self) -> &<TestKernel as Kernel>::ToolPath {
        &self.tool_path
    }

    fn registration(&self) -> &ToolRegistration {
        self.registration
    }

    fn workspace_root(&self) -> &Path {
        self.workspace_root.as_path()
    }

    async fn invoke_tool(
        &self,
        _path: <TestKernel as Kernel>::ToolPath,
        _capabilities_override: Option<Capabilities>,
        _payload: Value,
    ) -> Result<ToolOutcome, ToolError> {
        panic!("unused in fs service tests")
    }
}

impl HasFsService<TestKernel> for UnusedToolContext<'_> {
    fn fs(&self) -> FsService<'_, TestKernel> {
        FsService::new(self.kernel, self.workspace_root(), self.policy_context())
    }
}

struct TestKernel {
    fs: StdFsBackend,
    policy: PolicyPlane<KernelPolicyContextFactory>,
}

impl TestKernel {
    fn new() -> Self {
        let mut policy = PolicyPlane::new();
        policy.prepend_inbound(CapabilityEnvelopePolicy);
        policy.append::<FsAction, _>(AllowWorkspaceFsPolicy);
        Self {
            fs: StdFsBackend::new(),
            policy,
        }
    }
}

impl HasFsBackend for TestKernel {
    type FsBackend = StdFsBackend;

    fn fs_backend(&self) -> &Self::FsBackend {
        &self.fs
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
        panic!("unused in fs service tests")
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
        workspace_root: CanonicalRoot::existing(&params.workspace_root).unwrap(),
    }
}

#[tokio::test]
async fn exact_file_policy_only_allows_one_path() {
    let ws = TempWorkspace::new();
    let target = ws.root.join("allowed.txt");
    std::fs::write(&target, "ok").unwrap();
    std::fs::write(ws.root.join("blocked.txt"), "no").unwrap();
    let canonical_target = std::fs::canonicalize(&target).unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsReadAction, _>(AllowExactFileReadPolicy::new(canonical_target));
    let registration = registration([Capability::FsRead].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(&kernel, &registration, &params, [Capability::FsRead].into());

    assert!(ctx.fs().read_file("allowed.txt").await.is_ok());

    let denied = ctx.fs().read_file("blocked.txt").await;
    assert!(matches!(
        denied,
        Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
    ));
}

#[tokio::test]
async fn prefix_policy_allows_reads_under_prefix() {
    let ws = TempWorkspace::new();
    std::fs::create_dir_all(ws.root.join("src")).unwrap();
    std::fs::write(ws.root.join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(ws.root.join("root.txt"), "no").unwrap();
    let canonical_src = std::fs::canonicalize(ws.root.join("src")).unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsReadAction, _>(AllowFileReadPrefixPolicy::new(canonical_src));
    let registration = registration([Capability::FsRead].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(&kernel, &registration, &params, [Capability::FsRead].into());

    assert!(ctx.fs().read_file("src/main.rs").await.is_ok());

    let denied = ctx.fs().read_file("root.txt").await;
    assert!(matches!(
        denied,
        Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
    ));
}

#[tokio::test]
async fn workspace_policy_allows_reads_inside_workspace() {
    let ws = TempWorkspace::new();
    std::fs::create_dir_all(ws.root.join("a/b")).unwrap();
    std::fs::write(ws.root.join("a/b/file.txt"), "all").unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsReadAction, _>(AllowWorkspaceReadPolicy);
    let registration = registration([Capability::FsRead].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(&kernel, &registration, &params, [Capability::FsRead].into());

    let content = ctx.fs().read_file("a/b/file.txt").await.unwrap();
    assert_eq!(content, "all");
}

#[tokio::test]
async fn effective_capabilities_can_shrink_read_service_access() {
    let ws = TempWorkspace::new();
    std::fs::write(ws.root.join("hello.txt"), "hello").unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsReadAction, _>(AllowWorkspaceReadPolicy);
    let registration = registration([Capability::FsRead].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(&kernel, &registration, &params, Capabilities::empty());

    let denied = ctx.fs().read_file("hello.txt").await;
    assert!(matches!(
        denied,
        Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
    ));
}

#[tokio::test]
async fn exact_file_write_policy_only_allows_one_path() {
    let ws = TempWorkspace::new();
    let target = ws.root.join("allowed.txt");
    std::fs::write(&target, "before").unwrap();
    std::fs::write(ws.root.join("blocked.txt"), "blocked").unwrap();
    let canonical_target = std::fs::canonicalize(&target).unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsWriteAction, _>(AllowExactFileWritePolicy::new(canonical_target));
    let registration = registration([Capability::FsWrite].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(
        &kernel,
        &registration,
        &params,
        [Capability::FsWrite].into(),
    );

    ctx.fs().write_file("allowed.txt", "ok").await.unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "ok");

    let denied = ctx.fs().write_file("blocked.txt", "no").await;
    assert!(matches!(
        denied,
        Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
    ));
}

#[tokio::test]
async fn exact_file_write_policy_normalizes_missing_target_parent() {
    let ws = TempWorkspace::new();
    let requested_target = ws.root.join("created.txt");

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsWriteAction, _>(AllowExactFileWritePolicy::new(&requested_target));
    let registration = registration([Capability::FsWrite].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(
        &kernel,
        &registration,
        &params,
        [Capability::FsWrite].into(),
    );

    ctx.fs().write_file("created.txt", "ok").await.unwrap();
    assert_eq!(std::fs::read_to_string(requested_target).unwrap(), "ok");
}

#[tokio::test]
async fn prefix_policy_allows_writes_under_prefix() {
    let ws = TempWorkspace::new();
    std::fs::create_dir_all(ws.root.join("out")).unwrap();
    std::fs::write(ws.root.join("out/ok.txt"), "before").unwrap();
    std::fs::write(ws.root.join("root.txt"), "blocked").unwrap();
    let canonical_out = std::fs::canonicalize(ws.root.join("out")).unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsWriteAction, _>(AllowFileWritePrefixPolicy::new(canonical_out));
    let registration = registration([Capability::FsWrite].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(
        &kernel,
        &registration,
        &params,
        [Capability::FsWrite].into(),
    );

    ctx.fs().write_file("out/ok.txt", "ok").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(ws.root.join("out/ok.txt")).unwrap(),
        "ok"
    );

    let denied = ctx.fs().write_file("root.txt", "no").await;
    assert!(matches!(
        denied,
        Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
    ));
}

#[tokio::test]
async fn workspace_policy_allows_writes_inside_workspace() {
    let ws = TempWorkspace::new();
    std::fs::create_dir_all(ws.root.join("a/b")).unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsWriteAction, _>(AllowWorkspaceWritePolicy);
    let registration = registration([Capability::FsWrite].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(
        &kernel,
        &registration,
        &params,
        [Capability::FsWrite].into(),
    );

    ctx.fs()
        .write_file("a/b/file.txt", "written")
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(ws.root.join("a/b/file.txt")).unwrap(),
        "written"
    );
}

#[tokio::test]
async fn effective_capabilities_can_shrink_write_service_access() {
    let ws = TempWorkspace::new();
    std::fs::write(ws.root.join("hello.txt"), "hello").unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsWriteAction, _>(AllowWorkspaceWritePolicy);
    let registration = registration([Capability::FsWrite].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(&kernel, &registration, &params, Capabilities::empty());

    let denied = ctx.fs().write_file("hello.txt", "nope").await;
    assert!(matches!(
        denied,
        Err(ExecutionError::Authorization(AuthorizationError::Denied(_)))
    ));
}

#[tokio::test]
async fn write_can_create_new_file_inside_workspace() {
    let ws = TempWorkspace::new();
    std::fs::create_dir_all(ws.root.join("nested")).unwrap();

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsWriteAction, _>(AllowWorkspaceWritePolicy);
    let registration = registration([Capability::FsWrite].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(
        &kernel,
        &registration,
        &params,
        [Capability::FsWrite].into(),
    );

    ctx.fs()
        .write_file("nested/new.txt", "created")
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(ws.root.join("nested/new.txt")).unwrap(),
        "created"
    );
}

#[tokio::test]
async fn write_rejects_targets_outside_workspace() {
    let ws = TempWorkspace::new();
    let outside = std::env::temp_dir().join("tool-plane-outside-write.txt");
    let _ = std::fs::remove_file(&outside);

    let mut kernel = TestKernel::new();
    kernel
        .policy
        .append::<FsWriteAction, _>(AllowWorkspaceWritePolicy);
    let registration = registration([Capability::FsWrite].into());
    let params = InvocationParams::new(&ws.root, None);
    let ctx = tool_ctx(
        &kernel,
        &registration,
        &params,
        [Capability::FsWrite].into(),
    );

    let denied = ctx
        .fs()
        .write_file(outside.to_str().unwrap(), "blocked")
        .await;
    assert!(matches!(
        denied,
        Err(ExecutionError::Authorization(
            AuthorizationError::OutsideWorkspace
        ))
    ));
    assert!(!outside.exists());
}
