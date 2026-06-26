use crate::error::ExecutionError;
use crate::kernel::Kernel;
use crate::policy::{PolicyContextFactory, PolicyEngine};
use crate::tool::ToolContext;
use std::path::Path;

pub mod access;
pub mod action;
pub mod policy;

pub use access::{FsAccess, StdFs};
pub use action::{CanonicalPath, FsReadAction, FsWriteAction};
pub use policy::{
    AllowExactFileReadPolicy, AllowExactFileWritePolicy, AllowFileReadPrefixPolicy,
    AllowFileWritePrefixPolicy, AllowWorkspaceReadPolicy, AllowWorkspaceWritePolicy,
};

pub trait FsService {
    fn fs_access(&self) -> &dyn FsAccess;
}

pub trait FsToolContextExt<K>: ToolContext<K>
where
    K: FsService + Kernel,
{
    fn fs(&self) -> FsContext<'_, K>
    where
        Self: Sized,
    {
        FsContext::new(self.kernel(), self.workspace_root(), self.policy_context())
    }
}

impl<K, T> FsToolContextExt<K> for T
where
    T: ToolContext<K>,
    K: FsService + Kernel,
{
}

/// Filesystem sub-context exposed as `ctx.fs()`.
///
/// Public methods remain natural and function-like. Internally they follow the
/// same pipeline: construct an action, ask policy to grant it, then execute the
/// granted action. Grant / execute audit stays in the shared policy/action core.
type PolicyContextFor<'a, K> =
    <<K as Kernel>::PolicyCxFactory as PolicyContextFactory>::Context<'a>;

pub struct FsContext<'a, K>
where
    K: Kernel + FsService + ?Sized,
{
    kernel: &'a K,
    workspace_root: &'a Path,
    policy_context: PolicyContextFor<'a, K>,
}

impl<'a, K> FsContext<'a, K>
where
    K: Kernel + FsService,
{
    pub fn new(
        kernel: &'a K,
        workspace_root: &'a Path,
        policy_context: PolicyContextFor<'a, K>,
    ) -> Self {
        Self {
            kernel,
            workspace_root,
            policy_context,
        }
    }

    pub async fn read_file(&self, path: &str) -> Result<String, ExecutionError> {
        let path = action::resolve_under_authorization(self.workspace_root, Path::new(path))
            .map_err(ExecutionError::Authorization)?;
        let action = FsReadAction::new(path);
        let granted = self
            .kernel
            .policy_plane()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.kernel.fs_access()).await
    }

    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), ExecutionError> {
        let path = action::resolve_write_under_authorization(self.workspace_root, Path::new(path))
            .map_err(ExecutionError::Authorization)?;
        let action = FsWriteAction::new(path, content.to_owned());
        let granted = self
            .kernel
            .policy_plane()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.kernel.fs_access()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{AuthorizationError, ToolError};
    use crate::kernel::Kernel;
    use crate::policy::{
        CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory, PolicyPlane,
    };
    use crate::test_utils::*;
    use crate::tool::ToolContext;
    use async_trait::async_trait;
    use mvp_contract::{Capabilities, Capability, InvocationParams, ToolOutcome, ToolRequest};

    struct UnusedToolContext<'a> {
        kernel: &'a TestKernel,
        registration: &'a crate::tool::ToolRegistration,
        effective_capabilities: Capabilities,
        workspace_root: std::path::PathBuf,
    }

    #[async_trait]
    impl ToolContext<TestKernel> for UnusedToolContext<'_> {
        fn kernel(&self) -> &TestKernel {
            self.kernel
        }

        fn policy_context(&self) -> KernelPolicyContext<'_> {
            KernelPolicyContext::new(self.effective_capabilities, self.workspace_root())
        }

        fn effective_capabilities(&self) -> Capabilities {
            self.effective_capabilities
        }

        fn registration(&self) -> &crate::tool::ToolRegistration {
            self.registration
        }

        fn workspace_root(&self) -> &Path {
            &self.workspace_root
        }

        async fn invoke_tool(
            &self,
            _capabilities_override: Option<Capabilities>,
            _req: ToolRequest,
        ) -> Result<ToolOutcome, ToolError> {
            panic!("unused in fs service tests")
        }
    }

    struct TestKernel {
        fs: StdFs,
        policy: PolicyPlane<KernelPolicyContextFactory>,
    }

    impl TestKernel {
        fn new() -> Self {
            let mut policy = PolicyPlane::new();
            policy.prepend_inbound(CapabilityEnvelopePolicy);
            Self {
                fs: StdFs::new(),
                policy,
            }
        }
    }

    impl FsService for TestKernel {
        fn fs_access(&self) -> &dyn FsAccess {
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
        type ToolCx<'a>
            = UnusedToolContext<'a>
        where
            Self: 'a;

        fn policy_plane(&self) -> &Self::PolicyPlane<'_> {
            &self.policy
        }

        async fn invoke(
            &self,
            _params: &InvocationParams,
            _req: ToolRequest,
        ) -> Result<ToolOutcome, ToolError> {
            panic!("unused in fs service tests")
        }
    }

    fn tool_ctx<'a>(
        kernel: &'a TestKernel,
        registration: &'a crate::tool::ToolRegistration,
        params: &'a InvocationParams,
        effective_capabilities: Capabilities,
        _workspace_root: &'a Path,
    ) -> UnusedToolContext<'a> {
        UnusedToolContext {
            kernel,
            registration,
            effective_capabilities,
            workspace_root: std::fs::canonicalize(&params.workspace_root).unwrap(),
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
        let ctx = tool_ctx(
            &kernel,
            &registration,
            &params,
            [Capability::FsRead].into(),
            &ws.root,
        );

        let ok = ctx.fs().read_file("allowed.txt").await;
        assert!(ok.is_ok());

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
        let ctx = tool_ctx(
            &kernel,
            &registration,
            &params,
            [Capability::FsRead].into(),
            &ws.root,
        );

        let ok = ctx.fs().read_file("src/main.rs").await;
        assert!(ok.is_ok());

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
        let ctx = tool_ctx(
            &kernel,
            &registration,
            &params,
            [Capability::FsRead].into(),
            &ws.root,
        );

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
        let ctx = tool_ctx(
            &kernel,
            &registration,
            &params,
            Capabilities::empty(),
            &ws.root,
        );

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
            &ws.root,
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
            &ws.root,
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
            &ws.root,
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
        let ctx = tool_ctx(
            &kernel,
            &registration,
            &params,
            Capabilities::empty(),
            &ws.root,
        );

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
            &ws.root,
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
            &ws.root,
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
}
