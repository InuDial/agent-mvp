use std::path::Path;

use async_trait::async_trait;
use mvp_contract::{Capabilities, ToolOutcome, ToolRequest};

use crate::{error::ToolError, kernel::Kernel, policy::PolicyContextFactory};

#[async_trait]
pub trait ToolContext<K: Kernel + ?Sized>: Sync {
    fn kernel(&self) -> &K;
    fn policy_context(&self) -> <K::PolicyCxFactory as PolicyContextFactory>::Context<'_>;
    fn effective_capabilities(&self) -> Capabilities;
    fn registration(&self) -> &super::registration::ToolRegistration;
    fn workspace_root(&self) -> &Path;

    async fn invoke_tool(
        &self,
        capabilities_override: Option<Capabilities>,
        req: ToolRequest,
    ) -> Result<ToolOutcome, ToolError>;
}
