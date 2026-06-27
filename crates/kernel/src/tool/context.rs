use std::path::Path;

use async_trait::async_trait;
use mvp_contract::{Capabilities, ToolOutcome};
use serde_json::Value;

use crate::{error::ToolError, kernel::Kernel, policy::PolicyContextFactory};

#[async_trait]
pub trait ToolContext<K: Kernel + ?Sized>: Sync {
    fn policy_context(&self) -> <K::PolicyCxFactory as PolicyContextFactory>::Context<'_>;
    fn effective_capabilities(&self) -> Capabilities;
    fn tool_path(&self) -> &K::ToolPath;
    fn registration(&self) -> &super::registration::ToolRegistration;
    fn workspace_root(&self) -> &Path;

    async fn invoke_tool(
        &self,
        path: K::ToolPath,
        capabilities_override: Option<Capabilities>,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError>;
}
