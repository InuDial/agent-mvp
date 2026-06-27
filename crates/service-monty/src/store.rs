use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use async_trait::async_trait;
use mvp_kernel::action::ActionExecutor;
use mvp_kernel::error::ExecutionError;
use mvp_kernel::policy::Granted;

use crate::{MontySessionLoadAction, MontySessionSaveAction};

/// Kernel-side storage for serialized Monty REPL sessions.
pub trait MontySessionStore: Send + Sync {
    fn load(&self, key: &MontySessionKey) -> Result<Option<Vec<u8>>, ExecutionError>;
    fn save(&self, key: MontySessionKey, bytes: Vec<u8>) -> Result<(), ExecutionError>;
}

/// Kernel extension for implementations that own Monty session storage.
pub trait HasMontySessionStore: Send + Sync {
    type MontySessionStore: MontySessionStore + ?Sized;

    fn monty_session_store(&self) -> &Self::MontySessionStore;
}

impl<T> MontySessionStore for T
where
    T: HasMontySessionStore,
{
    fn load(&self, key: &MontySessionKey) -> Result<Option<Vec<u8>>, ExecutionError> {
        self.monty_session_store().load(key)
    }

    fn save(&self, key: MontySessionKey, bytes: Vec<u8>) -> Result<(), ExecutionError> {
        self.monty_session_store().save(key, bytes)
    }
}

#[async_trait]
impl ActionExecutor<MontySessionLoadAction> for dyn MontySessionStore + '_ {
    type Output = Option<Vec<u8>>;

    async fn execute(
        &self,
        granted: Granted<MontySessionLoadAction>,
    ) -> Result<Self::Output, ExecutionError> {
        let action = granted.into_action();
        self.load(&action.key)
    }
}

#[async_trait]
impl ActionExecutor<MontySessionSaveAction> for dyn MontySessionStore + '_ {
    type Output = ();

    async fn execute(
        &self,
        granted: Granted<MontySessionSaveAction>,
    ) -> Result<Self::Output, ExecutionError> {
        let action = granted.into_action();
        self.save(action.key, action.bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MontySessionKey {
    workspace_root: PathBuf,
    session_id: String,
}

impl MontySessionKey {
    #[must_use]
    pub(crate) fn new(workspace_root: impl AsRef<Path>, session_id: impl Into<String>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
            session_id: session_id.into(),
        }
    }

    pub fn audit_resource(&self) -> String {
        format!("{}#{}", self.workspace_root.display(), self.session_id)
    }
}

#[derive(Default)]
pub struct MemoryMontySessionStore {
    sessions: Mutex<BTreeMap<MontySessionKey, Vec<u8>>>,
}

impl MemoryMontySessionStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl MontySessionStore for MemoryMontySessionStore {
    fn load(&self, key: &MontySessionKey) -> Result<Option<Vec<u8>>, ExecutionError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|err| ExecutionError::Other(format!("Monty session store poisoned: {err}")))?;
        Ok(sessions.get(key).cloned())
    }

    fn save(&self, key: MontySessionKey, bytes: Vec<u8>) -> Result<(), ExecutionError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|err| ExecutionError::Other(format!("Monty session store poisoned: {err}")))?;
        sessions.insert(key, bytes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mvp_contract::{Capabilities, InvocationParams, ToolOutcome};
    use mvp_kernel::error::{InputError, ToolError};
    use mvp_kernel::kernel::Kernel;
    use mvp_kernel::policy::{
        CapabilityEnvelopePolicy, KernelPolicyContext, KernelPolicyContextFactory,
        PolicyContextFactory, PolicyPlane,
    };
    use mvp_kernel::tool::{ToolContext, ToolRegistration};
    use serde_json::Value;
    use std::path::{Path, PathBuf};

    use crate::{
        AllowMontySessionPolicy, MontySessionLoadAction, MontySessionSaveAction,
        MontySessionService,
    };

    struct TestKernel {
        store: MemoryMontySessionStore,
        policy: PolicyPlane<KernelPolicyContextFactory>,
    }

    struct UnusedToolContext {
        workspace_root: PathBuf,
    }

    #[async_trait]
    impl ToolContext<TestKernel> for UnusedToolContext {
        fn policy_context(
            &self,
        ) -> <KernelPolicyContextFactory as PolicyContextFactory>::Context<'_> {
            KernelPolicyContext::new(Capabilities::empty(), &self.workspace_root)
        }

        fn effective_capabilities(&self) -> Capabilities {
            Capabilities::empty()
        }

        fn tool_path(&self) -> &<TestKernel as Kernel>::ToolPath {
            panic!("unused in Monty session store tests")
        }

        fn registration(&self) -> &ToolRegistration {
            panic!("unused in Monty session store tests")
        }

        fn workspace_root(&self) -> &Path {
            &self.workspace_root
        }

        async fn invoke_tool(
            &self,
            _path: <TestKernel as Kernel>::ToolPath,
            _capabilities_override: Option<Capabilities>,
            _payload: Value,
        ) -> Result<ToolOutcome, ToolError> {
            panic!("unused in Monty session store tests")
        }
    }

    impl TestKernel {
        fn new() -> Self {
            let mut policy = PolicyPlane::new();
            policy.prepend_inbound(CapabilityEnvelopePolicy);
            policy.append::<MontySessionLoadAction, _>(AllowMontySessionPolicy);
            policy.append::<MontySessionSaveAction, _>(AllowMontySessionPolicy);
            Self {
                store: MemoryMontySessionStore::new(),
                policy,
            }
        }
    }

    impl HasMontySessionStore for TestKernel {
        type MontySessionStore = MemoryMontySessionStore;

        fn monty_session_store(&self) -> &Self::MontySessionStore {
            &self.store
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
            = UnusedToolContext
        where
            Self: 'a;

        fn policy_plane(&self) -> &Self::PolicyPlane<'_> {
            &self.policy
        }

        fn decode_tool_path(_value: &Value) -> Result<Self::ToolPath, InputError> {
            panic!("unused in Monty session store tests")
        }

        async fn invoke(
            &self,
            _path: &Self::ToolPath,
            _params: &InvocationParams,
            _payload: Value,
        ) -> Result<ToolOutcome, ToolError> {
            panic!("unused in Monty session store tests")
        }
    }

    fn service<'a>(
        kernel: &'a TestKernel,
        workspace_root: &'a Path,
    ) -> MontySessionService<'a, TestKernel> {
        MontySessionService::new(
            kernel,
            workspace_root,
            KernelPolicyContext::new(Capabilities::empty(), workspace_root),
        )
    }

    #[tokio::test]
    async fn memory_store_round_trips_session_bytes_through_action_pipeline() {
        let kernel = TestKernel::new();
        let workspace = PathBuf::from("/tmp/workspace-a");
        let service = service(&kernel, &workspace);

        service.save("default", b"state".to_vec()).await.unwrap();

        assert_eq!(
            service.load("default").await.unwrap(),
            Some(b"state".to_vec())
        );
    }

    #[tokio::test]
    async fn session_keys_are_scoped_by_workspace_through_action_pipeline() {
        let kernel = TestKernel::new();
        let workspace_a_root = PathBuf::from("/tmp/workspace-a");
        let workspace_b_root = PathBuf::from("/tmp/workspace-b");
        let workspace_a = service(&kernel, &workspace_a_root);
        let workspace_b = service(&kernel, &workspace_b_root);

        workspace_a.save("default", b"a".to_vec()).await.unwrap();

        assert_eq!(workspace_b.load("default").await.unwrap(), None);
    }
}
