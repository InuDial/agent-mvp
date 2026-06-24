use crate::error::ExecutionError;
use crate::policy::{PolicyContext, PolicyPlane};
use std::path::Path;

pub mod access;
pub mod action;
pub mod policy;

pub use access::{FsAccess, StdFs};
pub use action::{CanonicalPath, FsReadAction};
pub use policy::{AllowExactFileReadPolicy, AllowFileReadPrefixPolicy, AllowWorkspaceReadPolicy};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AuthorizationError;
    use crate::service::network::DenyNetwork;
    use crate::tool::{ToolPlane, ToolPlaneContext, test_utils::*};
    use mvp_contract::Capability;

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
        let ctx = ToolPlaneContext::new(&plane, reg, params).unwrap();

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
        let ctx = ToolPlaneContext::new(&plane, reg, params).unwrap();

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
        let ctx = ToolPlaneContext::new(&plane, reg, params).unwrap();

        let content = ctx.fs().read_file("a/b/file.txt").await.unwrap();
        assert_eq!(content, "all");
    }
}
