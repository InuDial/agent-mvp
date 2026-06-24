use crate::error::{AuthorizationError, OutputAuthorizationError, ToolError};
use crate::policy::KernelPolicyContext;
use crate::service::{fs::FsContext, network::NetworkContext};
use crate::tool::{InvocationParams, ToolPlane, ToolRegistration};
use mvp_contract::{ToolOutcome, ToolRequest};

/// The single runtime context passed to tool implementations.
///
/// It intentionally does not encode phases in the type. Instead, it exposes
/// service namespaces such as `ctx.fs()` and `ctx.network()`. Each service owns
/// its grant issuance, grant consumption, policy checks, and audit records.
pub struct ToolPlaneContext<'a> {
    tool_plane: &'a ToolPlane,
    pub(crate) registration: &'a ToolRegistration,
    pub(crate) params: InvocationParams,
}

impl<'a> ToolPlaneContext<'a> {
    pub(crate) fn new(
        tool_plane: &'a ToolPlane,
        registration: &'a ToolRegistration,
        mut params: InvocationParams,
    ) -> Result<Self, AuthorizationError> {
        params.workspace_root =
            std::fs::canonicalize(&params.workspace_root).map_err(AuthorizationError::Io)?;

        Ok(Self {
            tool_plane,
            registration,
            params,
        })
    }

    pub(crate) async fn finalize_output(
        &self,
        _registration: &ToolRegistration,
        outcome: ToolOutcome,
    ) -> Result<ToolOutcome, OutputAuthorizationError> {
        Ok(outcome)
    }

    /// Output helper kept on the total context for now.
    ///
    /// In a fuller design this can move to `ctx.output()` or `ctx.text()` just
    /// like filesystem and network operations live under their own namespaces.
    pub async fn redact_text(&self, text: String) -> Result<String, OutputAuthorizationError> {
        Ok(text)
    }

    pub(crate) fn policy_context(&self) -> KernelPolicyContext {
        KernelPolicyContext::new(
            self.registration.spec.capabilities,
            self.params.workspace_root.clone(),
        )
    }

    pub fn params(&self) -> &InvocationParams {
        &self.params
    }

    pub async fn invoke_tool(&self, req: ToolRequest) -> Result<ToolOutcome, ToolError> {
        self.tool_plane.invoke(self.params.clone(), req).await
    }

    pub fn fs(&'a self) -> FsContext<'a, KernelPolicyContext> {
        FsContext::new(
            &*self.tool_plane.fs,
            &self.tool_plane.policy,
            self.policy_context(),
            self.params.workspace_root.as_path(),
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
