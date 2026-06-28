use std::fmt::Debug;

use async_trait::async_trait;
use mvp_contract::{InvocationParams, ToolOutcome};
use serde_json::Value;

use crate::{
    error::{InputError, ToolError},
    policy::HasPolicyEngine,
};

use super::ToolContext;

#[async_trait]
pub trait ToolHost: HasPolicyEngine + Sized {
    type ToolPath: Clone + Ord + Debug + Send + Sync + 'static;
    type ToolCx<'a>: ToolContext<Self>
    where
        Self: 'a;

    fn decode_tool_path(value: &Value) -> Result<Self::ToolPath, InputError>;

    async fn invoke(
        &self,
        path: &Self::ToolPath,
        params: &InvocationParams,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError>;
}
