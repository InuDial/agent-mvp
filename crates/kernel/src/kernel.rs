use async_trait::async_trait;
use mvp_contract::{InvocationParams, ToolOutcome};
use serde_json::Value;

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

    type ToolPath;
    type ToolCx<'a>: ToolContext<Self>
    where
        Self: 'a;

    fn policy_plane(&self) -> &Self::PolicyPlane<'_>;

    async fn invoke(
        &self,
        path: Self::ToolPath,
        params: &InvocationParams,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError>;
}
