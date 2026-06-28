use std::path::{Path, PathBuf};

use mvp_kernel::error::ExecutionError;
use mvp_kernel::kernel::{Kernel, PolicyContextFor};
use mvp_kernel::policy::PolicyEngine;
use mvp_kernel::tool::ToolContext;

use crate::{MontySessionKey, MontySessionLoadAction, MontySessionSaveAction, MontySessionStore};

/// Tool-context extension used by Monty tools to read and persist REPL state.
pub trait HasMontySessionAccess<K>: ToolContext<K>
where
    K: Kernel + MontySessionStore,
{
    fn monty_sessions(&self) -> MontySessionAccess<'_, K>;
}

pub struct MontySessionAccess<'a, K>
where
    K: Kernel + MontySessionStore + ?Sized,
{
    kernel: &'a K,
    policy_context: PolicyContextFor<'a, K>,
    workspace_root: PathBuf,
}

impl<'a, K> MontySessionAccess<'a, K>
where
    K: Kernel + MontySessionStore,
{
    #[must_use]
    pub fn new(
        kernel: &'a K,
        workspace_root: impl AsRef<Path>,
        policy_context: PolicyContextFor<'a, K>,
    ) -> Self {
        Self {
            kernel,
            policy_context,
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    pub async fn load(&self, session_id: &str) -> Result<Option<Vec<u8>>, ExecutionError> {
        let action =
            MontySessionLoadAction::new(MontySessionKey::new(&self.workspace_root, session_id));
        let granted = self
            .kernel
            .policy_engine()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        let executor: &dyn MontySessionStore = self.kernel;
        granted.execute_with(executor).await
    }

    pub async fn save(&self, session_id: &str, bytes: Vec<u8>) -> Result<(), ExecutionError> {
        let action = MontySessionSaveAction::new(
            MontySessionKey::new(&self.workspace_root, session_id),
            bytes,
        );
        let granted = self
            .kernel
            .policy_engine()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        let executor: &dyn MontySessionStore = self.kernel;
        granted.execute_with(executor).await
    }
}
