use crate::error::ExecutionError;
use crate::policy::{PolicyContext, PolicyPlane};
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

/// Filesystem sub-context exposed as `ctx.fs()`.
///
/// Public methods remain natural and function-like. Internally they follow the
/// same pipeline: construct an action, ask policy to grant it, then execute the
/// granted action. Grant / execute audit stays in the shared policy/action core.
pub struct FsContext<'a, Ctx: PolicyContext> {
    fs: &'a dyn FsAccess,
    policy: &'a PolicyPlane<Ctx>,
    policy_ctx: Ctx,
    workspace_root: &'a Path,
}

impl<'a, Ctx: PolicyContext> FsContext<'a, Ctx> {
    pub fn new(
        fs: &'a dyn FsAccess,
        policy: &'a PolicyPlane<Ctx>,
        policy_ctx: Ctx,
        workspace_root: &'a Path,
    ) -> Self {
        Self {
            fs,
            policy,
            policy_ctx,
            workspace_root,
        }
    }

    pub async fn read_file(&self, path: &str) -> Result<String, ExecutionError> {
        let path = action::resolve_under_authorization(self.workspace_root, Path::new(path))
            .map_err(ExecutionError::Authorization)?;
        let action = FsReadAction::new(path);
        let granted = self
            .policy
            .grant(&self.policy_ctx, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.fs).await
    }

    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), ExecutionError> {
        let path = action::resolve_write_under_authorization(self.workspace_root, Path::new(path))
            .map_err(ExecutionError::Authorization)?;
        let action = FsWriteAction::new(path, content.to_owned());
        let granted = self
            .policy
            .grant(&self.policy_ctx, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.fs).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AuthorizationError;
    use crate::service::network::DenyNetwork;
    use crate::tool::{ToolPlane, ToolPlaneContext, test_utils::*};
    use mvp_contract::{Capabilities, Capability};

    #[tokio::test]
    async fn exact_file_policy_only_allows_one_path() {
        let ws = TempWorkspace::new();
        let target = ws.root.join("allowed.txt");
        std::fs::write(&target, "ok").unwrap();
        std::fs::write(ws.root.join("blocked.txt"), "no").unwrap();

        let reg = Box::leak(Box::new(registration([Capability::FsRead].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsReadAction, _>(AllowExactFileReadPolicy::new(target.clone()));
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

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

        let reg = Box::leak(Box::new(registration([Capability::FsRead].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsReadAction, _>(AllowFileReadPrefixPolicy::new(ws.root.join("src")));
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

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

        let reg = Box::leak(Box::new(registration([Capability::FsRead].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsReadAction, _>(AllowWorkspaceReadPolicy);
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

        let content = ctx.fs().read_file("a/b/file.txt").await.unwrap();
        assert_eq!(content, "all");
    }

    #[tokio::test]
    async fn effective_capabilities_can_shrink_read_service_access() {
        let ws = TempWorkspace::new();
        std::fs::write(ws.root.join("hello.txt"), "hello").unwrap();

        let reg = Box::leak(Box::new(registration([Capability::FsRead].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsReadAction, _>(AllowWorkspaceReadPolicy);
        let ctx = ToolPlaneContext::new(&plane, reg, &params, Some(Capabilities::empty())).unwrap();

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

        let reg = Box::leak(Box::new(registration([Capability::FsWrite].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsWriteAction, _>(AllowExactFileWritePolicy::new(target.clone()));
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

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

        let reg = Box::leak(Box::new(registration([Capability::FsWrite].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsWriteAction, _>(AllowFileWritePrefixPolicy::new(ws.root.join("out")));
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

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

        let reg = Box::leak(Box::new(registration([Capability::FsWrite].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsWriteAction, _>(AllowWorkspaceWritePolicy);
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

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

        let reg = Box::leak(Box::new(registration([Capability::FsWrite].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsWriteAction, _>(AllowWorkspaceWritePolicy);
        let ctx = ToolPlaneContext::new(&plane, reg, &params, Some(Capabilities::empty())).unwrap();

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

        let reg = Box::leak(Box::new(registration([Capability::FsWrite].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsWriteAction, _>(AllowWorkspaceWritePolicy);
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

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

        let reg = Box::leak(Box::new(registration([Capability::FsWrite].into())));
        let params = crate::tool::InvocationParams::new(&ws.root);
        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane
            .policy
            .append::<FsWriteAction, _>(AllowWorkspaceWritePolicy);
        let ctx = ToolPlaneContext::new(&plane, reg, &params, None).unwrap();

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
