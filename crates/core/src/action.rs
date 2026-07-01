use std::any::Any;

use mvp_contract::{AuditResource, Capabilities};

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
