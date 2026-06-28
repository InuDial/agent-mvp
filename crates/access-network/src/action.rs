use mvp_contract::{Capabilities, Capability};

use mvp_kernel::action::{Action, AuditResource};

#[derive(Clone, Debug)]
pub struct NetworkFetchAction {
    pub(crate) url: String,
}

impl NetworkFetchAction {
    pub(crate) fn new(url: String) -> Self {
        Self { url }
    }
}

impl Action for NetworkFetchAction {
    fn capabilities(&self) -> Capabilities {
        Capability::NetworkFetch.into()
    }

    fn audit_kind(&self) -> &'static str {
        "network.fetch"
    }

    fn audit_resource(&self) -> AuditResource {
        AuditResource::Value(self.url.clone())
    }
}
