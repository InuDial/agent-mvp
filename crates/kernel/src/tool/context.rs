use crate::audit;
use crate::error::{AuthorizationError, ToolError};
use crate::policy::KernelPolicyContext;
use crate::service::{fs::FsContext, network::NetworkContext};
use crate::tool::{InvocationParams, ToolPlane, ToolRegistration};
use mvp_contract::{Capabilities, ToolOutcome, ToolRequest};

/// The single runtime context passed to tool implementations.
///
/// It intentionally does not encode phases in the type. Instead, it exposes
/// service namespaces such as `ctx.fs()` and `ctx.network()`. Each service owns
/// its grant issuance, grant consumption, policy checks, and audit records.
pub struct ToolPlaneContext<'a> {
    tool_plane: &'a ToolPlane,
    pub(crate) registration: &'a ToolRegistration,
    pub(crate) params: &'a InvocationParams,
    effective_capabilities: Capabilities,
    canonical_workspace_root: std::path::PathBuf,
}

impl<'a> ToolPlaneContext<'a> {
    pub(crate) fn new(
        tool_plane: &'a ToolPlane,
        registration: &'a ToolRegistration,
        params: &'a InvocationParams,
        capabilities_override: Option<Capabilities>,
    ) -> Result<Self, AuthorizationError> {
        let canonical_workspace_root =
            std::fs::canonicalize(&params.workspace_root).map_err(AuthorizationError::Io)?;
        let declared_capabilities = registration.spec().capabilities;
        let effective_capabilities = match capabilities_override {
            Some(caps) => {
                audit::record_tool_capabilities_override(registration, declared_capabilities, caps);
                caps
            }
            None => declared_capabilities,
        };

        Ok(Self {
            tool_plane,
            registration,
            params,
            effective_capabilities,
            canonical_workspace_root,
        })
    }

    pub(crate) fn policy_context(&self) -> KernelPolicyContext {
        KernelPolicyContext::new(
            self.effective_capabilities,
            self.canonical_workspace_root.clone(),
        )
    }

    pub fn params(&self) -> &InvocationParams {
        self.params
    }

    pub fn effective_capabilities(&self) -> Capabilities {
        self.effective_capabilities
    }

    pub async fn invoke_tool(
        &self,
        capabilities_override: Option<Capabilities>,
        req: ToolRequest,
    ) -> Result<ToolOutcome, ToolError> {
        let effective_capabilities = match capabilities_override {
            Some(capabilities) => {
                if !self.effective_capabilities.contains(capabilities) {
                    audit::record_nested_capability_override(
                        self.registration,
                        &req.name,
                        self.effective_capabilities,
                        Some(capabilities),
                        None,
                        true,
                    );
                    return Err(ToolError::Authorization(AuthorizationError::Denied(
                        "nested invocation attempted to expand capabilities".into(),
                    )));
                }
                capabilities
            }
            None => self.effective_capabilities,
        };

        audit::record_nested_capability_override(
            self.registration,
            &req.name,
            self.effective_capabilities,
            capabilities_override,
            Some(effective_capabilities),
            false,
        );

        self.tool_plane
            .invoke(self.params, Some(effective_capabilities), req)
            .await
    }

    pub fn fs(&'a self) -> FsContext<'a, KernelPolicyContext> {
        FsContext::new(
            &*self.tool_plane.fs,
            &self.tool_plane.policy,
            self.policy_context(),
            self.canonical_workspace_root.as_path(),
        )
    }

    pub fn network(&'a self) -> NetworkContext<'a, KernelPolicyContext> {
        NetworkContext::new(
            &*self.tool_plane.network,
            &self.tool_plane.policy,
            self.policy_context(),
        )
    }
}
