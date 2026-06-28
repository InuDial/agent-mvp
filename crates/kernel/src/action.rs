//! Shared action traits used by access facades, policy, and execution.
//!
//! An `Action` is the semantic unit that policy understands. Access facades
//! construct actions from tool requests, the policy engine grants or denies them,
//! and only `Granted<Action>` values can execute through an executor.

use std::any::Any;
use std::path::PathBuf;

use async_trait::async_trait;
use mvp_contract::Capabilities;

use crate::error::ExecutionError;
use crate::policy::Granted;

#[derive(Clone, Debug)]
pub enum AuditResource {
    Path(PathBuf),
    Value(String),
    None,
}

/// A primitive service action.
///
/// Actions are the semantic units understood by policy. Access façades may
/// construct them internally before asking the policy engine for authorization.
pub trait Action: Any + Send + Sync {
    fn capabilities(&self) -> Capabilities;

    /// Stable audit kind for grant / execute records.
    fn audit_kind(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Audit resource attached to grant / execute records.
    fn audit_resource(&self) -> AuditResource {
        AuditResource::None
    }
}

/// An executor that knows how to run a granted domain action.
///
/// Actions stay policy-facing semantic values. Domain execution lives on the
/// backend or store that performs the side effect.
#[async_trait]
pub trait ActionExecutor<A>: Send + Sync
where
    A: Action,
{
    type Output;

    async fn execute(&self, granted: Granted<A>) -> Result<Self::Output, ExecutionError>;
}
