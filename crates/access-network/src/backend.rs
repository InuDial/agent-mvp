use async_trait::async_trait;

use mvp_core::error::{CapabilityError, ExecutionError};
use mvp_core::policy::Granted;

use super::action::NetworkFetchAction;

#[async_trait]
pub trait NetworkBackend: Send + Sync {
    async fn fetch_url(
        &self,
        granted: Granted<NetworkFetchAction>,
    ) -> Result<Vec<u8>, ExecutionError>;
}

pub trait HasNetworkBackend: Send + Sync {
    type NetworkBackend: NetworkBackend + ?Sized;

    fn network_backend(&self) -> &Self::NetworkBackend;
}

#[async_trait]
impl<T> NetworkBackend for T
where
    T: HasNetworkBackend,
{
    async fn fetch_url(
        &self,
        granted: Granted<NetworkFetchAction>,
    ) -> Result<Vec<u8>, ExecutionError> {
        self.network_backend().fetch_url(granted).await
    }
}

pub struct DenyNetworkBackend;

#[async_trait]
impl NetworkBackend for DenyNetworkBackend {
    async fn fetch_url(
        &self,
        _granted: Granted<NetworkFetchAction>,
    ) -> Result<Vec<u8>, ExecutionError> {
        Err(ExecutionError::Capability(CapabilityError::Denied))
    }
}

pub struct StaticNetworkBackend {
    responses: std::collections::BTreeMap<String, Vec<u8>>,
}

impl StaticNetworkBackend {
    pub fn new(responses: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
        }
    }
}

impl StaticNetworkBackend {
    fn fetch_raw(&self, url: &str) -> Result<Vec<u8>, CapabilityError> {
        self.responses
            .get(url)
            .cloned()
            .ok_or(CapabilityError::Denied)
    }
}

#[async_trait]
impl NetworkBackend for StaticNetworkBackend {
    async fn fetch_url(
        &self,
        granted: Granted<NetworkFetchAction>,
    ) -> Result<Vec<u8>, ExecutionError> {
        let action = granted.into_action();
        self.fetch_raw(&action.url)
            .map_err(ExecutionError::Capability)
    }
}
