use std::path::Path;

use mvp_kernel::error::ExecutionError;
use mvp_kernel::kernel::{Kernel, PolicyContextFor};
use mvp_kernel::policy::PolicyEngine;
use mvp_kernel::tool::ToolContext;

use crate::{CanonicalRoot, FsAction, FsBackend, FsReadAction, FsWriteAction, action};

pub trait HasFsService<K>: ToolContext<K>
where
    K: FsBackend + Kernel,
{
    fn fs(&self) -> FsService<'_, K>;
}

/// Filesystem service facade exposed as `ctx.fs()`.
///
/// Public methods remain natural and function-like. Internally they follow the
/// same pipeline: construct an action, ask policy to grant it, then execute the
/// granted action. Grant / execute audit stays in the shared policy/action core.
pub struct FsService<'a, K>
where
    K: Kernel + FsBackend + ?Sized,
{
    kernel: &'a K,
    workspace_root: CanonicalRoot,
    policy_context: PolicyContextFor<'a, K>,
}

impl<'a, K> FsService<'a, K>
where
    K: Kernel + FsBackend,
{
    pub fn new(
        kernel: &'a K,
        workspace_root: &'a Path,
        policy_context: PolicyContextFor<'a, K>,
    ) -> Self {
        let workspace_root = CanonicalRoot::existing(workspace_root)
            .expect("tool context workspace root is canonical");
        Self {
            kernel,
            workspace_root,
            policy_context,
        }
    }

    pub async fn read_file(&self, path: &str) -> Result<String, ExecutionError> {
        let path = action::resolve_under_authorization(&self.workspace_root, Path::new(path))
            .map_err(ExecutionError::Authorization)?;

        let parent = FsAction::new(path);
        let granted = self
            .kernel
            .policy_plane()
            .grant(&self.policy_context, parent)
            .await
            .map_err(ExecutionError::Authorization)?;

        let action = FsReadAction::new(granted);
        let granted = self
            .kernel
            .policy_plane()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.kernel).await
    }

    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), ExecutionError> {
        let path = action::resolve_write_under_authorization(&self.workspace_root, Path::new(path))
            .map_err(ExecutionError::Authorization)?;

        let parent = FsAction::new(path);
        let granted = self
            .kernel
            .policy_plane()
            .grant(&self.policy_context, parent)
            .await
            .map_err(ExecutionError::Authorization)?;

        let action = FsWriteAction::new(granted, content.to_owned());
        let granted = self
            .kernel
            .policy_plane()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.kernel).await
    }
}
