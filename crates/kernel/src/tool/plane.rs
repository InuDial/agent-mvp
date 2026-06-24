use crate::error::{AuthorizationError, ToolError};
use crate::service::{fs::FsAccess, network::NetworkAccess};
use crate::tool::{
    InvocationParams, KernelToolAdapter, ToolImpl, ToolPlaneContext, ToolRegistration,
    ToolRegistry,
};
use std::fs;
use mvp_contract::{ToolOutcome, ToolRequest};

/// Tool registry plus kernel-owned service implementations.
pub struct ToolPlane {
    registry: ToolRegistry,
    fs: Box<dyn FsAccess>,
    network: Box<dyn NetworkAccess>,
}

impl ToolPlane {
    pub fn new(fs: impl FsAccess + 'static, network: impl NetworkAccess + 'static) -> Self {
        Self {
            registry: ToolRegistry::new(),
            fs: Box::new(fs),
            network: Box::new(network),
        }
    }

    pub fn register<T: ToolImpl>(&mut self, tool: T) -> Result<(), ToolError> {
        let registration = ToolRegistration::new(tool.spec())?;
        let adapter = Box::new(KernelToolAdapter::new(tool));
        self.registry.insert(registration, adapter)
    }

    /// One-shot invocation API.
    ///
    /// The caller provides only invocation parameters and a request. The kernel
    /// constructs `ToolPlaneContext` internally, so tools cannot receive an
    /// arbitrary externally-created runtime context.
    pub async fn invoke(
        &self,
        params: InvocationParams,
        req: ToolRequest,
    ) -> Result<ToolOutcome, ToolError> {
        let registered = self
            .registry
            .get(&req.name)
            .ok_or_else(|| ToolError::UnknownTool(req.name.clone()))?;
        let workspace_root =
            fs::canonicalize(params.workspace_root).map_err(AuthorizationError::Io).map_err(ToolError::Authorization)?;
        let ctx = ToolPlaneContext::new(
            self.fs.as_ref(),
            self.network.as_ref(),
            registered.registration(),
            workspace_root,
        )
        .map_err(ToolError::Authorization)?;

        registered.invoke(&ctx, req).await
    }
}
