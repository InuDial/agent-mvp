use std::path::{Path, PathBuf};

use mvp_core::{
    error::ExecutionError,
    policy::{HasPolicyEngine, PolicyContextFor, PolicyEngine},
};

use crate::{MontySessionKey, MontySessionLoadAction, MontySessionSaveAction, MontySessionStore};

/// Tool-context extension used by Monty tools to read and persist REPL state.
pub trait HasMontySessionAccess {
    type Host: HasPolicyEngine + MontySessionStore;

    fn monty_sessions(&self) -> MontySessionAccess<'_, Self::Host>;
}

pub struct MontySessionAccess<'a, K>
where
    K: HasPolicyEngine + MontySessionStore + ?Sized,
{
    kernel: &'a K,
    policy_context: PolicyContextFor<'a, K>,
    workspace_root: PathBuf,
}

impl<'a, K> MontySessionAccess<'a, K>
where
    K: HasPolicyEngine + MontySessionStore,
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
        self.kernel.execute_granted(granted, executor).await
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
        self.kernel.execute_granted(granted, executor).await
    }
}
