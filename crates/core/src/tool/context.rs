use std::path::Path;

use async_trait::async_trait;
use mvp_contract::{Capabilities, ToolOutcome};
use serde_json::Value;

use crate::{
    error::ToolError,
    policy::PolicyContextFor,
    tool::{ToolHost, ToolRegistration},
};

#[async_trait]
pub trait ToolContext<H: ToolHost>: Sync {
    fn policy_context(&self) -> PolicyContextFor<'_, H>;
    fn effective_capabilities(&self) -> Capabilities;
    fn tool_path(&self) -> &H::ToolPath;
    fn registration(&self) -> &ToolRegistration;
    fn workspace_root(&self) -> &Path;

    async fn invoke_tool(
        &self,
        path: H::ToolPath,
        capabilities_override: Option<Capabilities>,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError>;
}
