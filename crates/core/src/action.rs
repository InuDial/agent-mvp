use std::any::Any;

use async_trait::async_trait;
use mvp_contract::{AuditResource, Capabilities};

use crate::{error::ExecutionError, policy::Granted};

/// A primitive authority-bearing action.
///
/// Actions are the semantic units understood by policy. Access facades
/// construct actions internally, ask the policy engine for authorization, and
/// only execute after receiving a `Granted<Action>`.
pub trait Action: Any + Send + Sync {
    fn capabilities(&self) -> Capabilities;

    fn audit_kind(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn audit_resource(&self) -> AuditResource {
        AuditResource::None
    }
}

/// Executor for a granted domain action.
#[async_trait]
pub trait ActionExecutor<A>: Send + Sync
where
    A: Action,
{
    type Output;

    async fn execute(&self, granted: Granted<A>) -> Result<Self::Output, ExecutionError>;
}
