use mvp_contract::{AuditResource, Capabilities, Capability};
use mvp_core::action::Action;

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
