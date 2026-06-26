use mvp_contract::Capabilities;

use crate::service::fs::CanonicalRoot;

pub trait PolicyContext: Send + Sync {
    fn capabilities(&self) -> &Capabilities;
}

/// A wrapper over PolicyContext to make it 'static
pub trait PolicyContextFactory: 'static {
    type Context<'a>: PolicyContext;
}

pub trait WorkspacePolicyContext: PolicyContext {
    fn workspace_root(&self) -> &CanonicalRoot;
}

pub struct KernelPolicyContext<'a> {
    capabilities: Capabilities,
    workspace_root: &'a CanonicalRoot,
}

impl<'a> KernelPolicyContext<'a> {
    pub fn new(capabilities: Capabilities, workspace_root: &'a CanonicalRoot) -> Self {
        Self {
            capabilities,
            workspace_root,
        }
    }
}

impl PolicyContext for KernelPolicyContext<'_> {
    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
}

impl WorkspacePolicyContext for KernelPolicyContext<'_> {
    fn workspace_root(&self) -> &CanonicalRoot {
        self.workspace_root
    }
}

pub struct KernelPolicyContextFactory;

impl PolicyContextFactory for KernelPolicyContextFactory {
    type Context<'a> = KernelPolicyContext<'a>;
}
