use async_trait::async_trait;
use mvp_contract::{InvocationParams, ToolOutcome, ToolRequest};

use crate::{
    error::ToolError,
    policy::{PolicyContextFactory, PolicyEngine},
    tool::ToolContext,
};

#[async_trait]
pub trait Kernel: Sync {
    type PolicyCxFactory: PolicyContextFactory;
    type PolicyPlane<'a>: PolicyEngine<Self::PolicyCxFactory>
    where
        Self: 'a;

    type ToolCx<'a>: ToolContext<Self>
    where
        Self: 'a;

    fn policy_plane(&self) -> &Self::PolicyPlane<'_>;

    async fn invoke(
        &self,
        params: &InvocationParams,
        req: ToolRequest,
    ) -> Result<ToolOutcome, ToolError>;
}
