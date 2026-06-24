use std::path::PathBuf;

use crate::error::{AuthorizationError, OutputAuthorizationError};
use crate::service::{fs::FsAccess, network::NetworkAccess};
use crate::tool::ToolRegistration;
use mvp_contract::ToolOutcome;

/// The single runtime context passed to tool implementations.
///
/// It intentionally does not encode phases in the type. Instead, it exposes
/// service namespaces such as `ctx.fs()` and `ctx.network()`. Each service owns
/// its grant issuance, grant consumption, policy checks, and audit records.
pub struct ToolPlaneContext<'a> {
    pub(crate) fs: &'a dyn FsAccess,
    pub(crate) network: &'a dyn NetworkAccess,
    pub(crate) registration: &'a ToolRegistration,
    pub(crate) workspace_root: PathBuf,
}

impl<'a> ToolPlaneContext<'a> {
    pub(crate) fn new(
        fs: &'a dyn FsAccess,
        network: &'a dyn NetworkAccess,
        registration: &'a ToolRegistration,
        workspace_root: PathBuf,
    ) -> Result<Self, AuthorizationError> {
        Ok(Self {
            fs,
            network,
            registration,
            workspace_root,
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
}
