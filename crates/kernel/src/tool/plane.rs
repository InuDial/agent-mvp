use crate::error::ToolError;
use crate::policy::{CapabilityEnvelopePolicy, KernelPolicyContext, PolicyPlane};
use crate::service::{fs::FsAccess, network::NetworkAccess};
use crate::tool::{
    InvocationParams, KernelToolAdapter, ToolImpl, ToolPlaneContext, ToolRegistration, ToolRegistry,
};
use mvp_contract::{Capabilities, ToolOutcome, ToolRequest};

/// Tool registry plus kernel-owned service implementations.
pub struct ToolPlane {
    registry: ToolRegistry,
    pub(crate) fs: Box<dyn FsAccess>,
    pub(crate) network: Box<dyn NetworkAccess>,
    pub policy: PolicyPlane<KernelPolicyContext>,
}

impl ToolPlane {
    pub fn new(fs: impl FsAccess + 'static, network: impl NetworkAccess + 'static) -> Self {
        let mut policy = PolicyPlane::new();
        policy.prepend_inbound(CapabilityEnvelopePolicy);
        Self {
            registry: ToolRegistry::new(),
            fs: Box::new(fs),
            network: Box::new(network),
            policy,
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
        params: &InvocationParams,
        capabilities_override: Option<Capabilities>,
        req: ToolRequest,
    ) -> Result<ToolOutcome, ToolError> {
        let registered = self
            .registry
            .get(&req.name)
            .ok_or_else(|| ToolError::UnknownTool(req.name.clone()))?;
        let ctx = ToolPlaneContext::new(
            self,
            registered.registration(),
            params,
            capabilities_override,
        )
        .map_err(ToolError::Authorization)?;

        registered.invoke(&ctx, req).await
    }
}
