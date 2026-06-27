use std::future::Future;
use std::pin::Pin;

use mvp_contract::Capabilities;
use mvp_kernel::action::{Action, AuditResource, ExecutableAction};
use mvp_kernel::error::ExecutionError;
use mvp_kernel::policy::Granted;

use crate::{MontySessionKey, MontySessionStore};

#[derive(Clone, Debug)]
pub struct MontySessionLoadAction {
    pub(crate) key: MontySessionKey,
}

impl MontySessionLoadAction {
    pub fn new(key: MontySessionKey) -> Self {
        Self { key }
    }
}

impl Action for MontySessionLoadAction {
    fn capabilities(&self) -> Capabilities {
        Capabilities::empty()
    }

    fn audit_kind(&self) -> &'static str {
        "monty.session.load"
    }

    fn audit_resource(&self) -> AuditResource {
        AuditResource::Value(self.key.audit_resource())
    }
}

impl ExecutableAction for MontySessionLoadAction {
    type Executor<'a> = dyn MontySessionStore + 'a;
    type Output = Option<Vec<u8>>;

    fn execute<'a>(
        store: &'a Self::Executor<'a>,
        granted: Granted<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ExecutionError>> + Send + 'a>>
    where
        Self: 'a,
    {
        Box::pin(async move {
            let action = granted.into_action();
            store.load(&action.key)
        })
    }
}

#[derive(Clone, Debug)]
pub struct MontySessionSaveAction {
    pub(crate) key: MontySessionKey,
    pub(crate) bytes: Vec<u8>,
}

impl MontySessionSaveAction {
    pub fn new(key: MontySessionKey, bytes: Vec<u8>) -> Self {
        Self { key, bytes }
    }
}

impl Action for MontySessionSaveAction {
    fn capabilities(&self) -> Capabilities {
        Capabilities::empty()
    }

    fn audit_kind(&self) -> &'static str {
        "monty.session.save"
    }

    fn audit_resource(&self) -> AuditResource {
        AuditResource::Value(self.key.audit_resource())
    }
}

impl ExecutableAction for MontySessionSaveAction {
    type Executor<'a> = dyn MontySessionStore + 'a;
    type Output = ();

    fn execute<'a>(
        store: &'a Self::Executor<'a>,
        granted: Granted<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ExecutionError>> + Send + 'a>>
    where
        Self: 'a,
    {
        Box::pin(async move {
            let action = granted.into_action();
            store.save(action.key, action.bytes)
        })
    }
}
