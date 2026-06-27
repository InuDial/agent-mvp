//! Kernel trait implemented by concrete runtimes.
//!
//! A kernel wires registered tools, service backends, policy, and invocation
//! context creation. Tools are generic over this trait so builtins can run on
//! any compatible runtime.

use std::fmt::Debug;

use async_trait::async_trait;
use mvp_contract::{InvocationParams, ToolOutcome};
use serde_json::Value;

use crate::{
    error::{InputError, ToolError},
    policy::{PolicyContextFactory, PolicyEngine},
    tool::ToolContext,
};

pub type PolicyContextFor<'a, K> =
    <<K as Kernel>::PolicyCxFactory as PolicyContextFactory>::Context<'a>;

#[async_trait]
pub trait Kernel: Sync {
    type PolicyCxFactory: PolicyContextFactory;
    type PolicyPlane<'a>: PolicyEngine<Self::PolicyCxFactory>
    where
        Self: 'a;

    type ToolPath: Clone + Ord + Debug + Send + Sync + 'static;
    type ToolCx<'a>: ToolContext<Self>
    where
        Self: 'a;

    fn policy_plane(&self) -> &Self::PolicyPlane<'_>;

    fn decode_tool_path(value: &Value) -> Result<Self::ToolPath, InputError>;

    async fn invoke(
        &self,
        path: &Self::ToolPath,
        params: &InvocationParams,
        payload: Value,
    ) -> Result<ToolOutcome, ToolError>;
}
