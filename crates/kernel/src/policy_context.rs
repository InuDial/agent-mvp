use std::path::Path;

use mvp_contract::Capabilities;
use mvp_core::policy::{PolicyContext, PolicyContextFactory, WorkspacePolicyContext};

pub struct KernelPolicyContext<'a> {
    capabilities: Capabilities,
    workspace_root: &'a Path,
}

impl<'a> KernelPolicyContext<'a> {
    pub fn new(capabilities: Capabilities, workspace_root: &'a Path) -> Self {
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
    fn workspace_root(&self) -> &Path {
        self.workspace_root
    }
}

pub struct KernelPolicyContextFactory;

impl PolicyContextFactory for KernelPolicyContextFactory {
    type Context<'a> = KernelPolicyContext<'a>;
}
