use mvp_contract::Capabilities;
use mvp_kernel::action::{Action, AuditResource};

use crate::MontySessionKey;

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
