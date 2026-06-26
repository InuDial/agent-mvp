//! Shared action traits used by service facades, policy, and execution.
//!
//! An `Action` is the semantic unit that policy understands. Service facades
//! construct actions from tool requests, the policy plane grants or denies them,
//! and only `Granted<Action>` values can execute through `ExecutableAction`.

use std::any::Any;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

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
/// Actions are the semantic units understood by policy. Service façades may
/// construct them internally before asking the policy plane for authorization.
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

/// An action that knows how to execute itself against a domain-local executor.
///
/// The action stays domain-specific by declaring its own executor type and
/// output type, while `Granted<A>` can uniformly forward to this trait.
pub trait ExecutableAction: Action + Sized {
    type Executor<'a>: ?Sized + Send + Sync
    where
        Self: 'a;

    type Output;

    fn execute<'a>(
        executor: &'a Self::Executor<'a>,
        granted: Granted<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ExecutionError>> + Send + 'a>>
    where
        Self: 'a;
}
