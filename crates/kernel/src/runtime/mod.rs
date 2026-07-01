use async_trait::async_trait;
use mvp_access_fs::{HasFsBackend, StdFsBackend};
use mvp_access_monty::{HasMontySessionStore, MemoryMontySessionStore};
use mvp_access_network::{DenyNetworkBackend, HasNetworkBackend};
use mvp_core::policy::HasPolicyEngine;

use crate::policy::{CapabilityEnvelopePolicy, KernelPolicyContextFactory, PolicyPipeline};

pub struct KernelRuntime {
    fs: StdFsBackend,
    network: DenyNetworkBackend,
    monty_sessions: MemoryMontySessionStore,
    pub policy: PolicyPipeline<KernelPolicyContextFactory>,
}

impl Default for KernelRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelRuntime {
    pub fn new() -> Self {
        let mut policy = PolicyPipeline::new();
        policy.prepend_inbound(CapabilityEnvelopePolicy);

        Self {
            fs: StdFsBackend,
            network: DenyNetworkBackend,
            monty_sessions: MemoryMontySessionStore::new(),
            policy,
        }
    }
}

impl HasFsBackend for KernelRuntime {
    type FsBackend = StdFsBackend;

    fn fs_backend(&self) -> &Self::FsBackend {
        &self.fs
    }
}

impl HasNetworkBackend for KernelRuntime {
    type NetworkBackend = DenyNetworkBackend;

    fn network_backend(&self) -> &Self::NetworkBackend {
        &self.network
    }
}

impl HasMontySessionStore for KernelRuntime {
    type MontySessionStore = MemoryMontySessionStore;

    fn monty_session_store(&self) -> &Self::MontySessionStore {
        &self.monty_sessions
    }
}

#[async_trait]
impl HasPolicyEngine for KernelRuntime {
    type PolicyCxFactory = KernelPolicyContextFactory;
    type PolicyEngine<'a>
        = PolicyPipeline<KernelPolicyContextFactory>
    where
        Self: 'a;

    fn policy_engine(&self) -> &Self::PolicyEngine<'_> {
        &self.policy
    }
}
