use async_trait::async_trait;

use mvp_kernel::action::ActionExecutor;
use mvp_kernel::error::{CapabilityError, ExecutionError};
use mvp_kernel::policy::Granted;

use super::action::NetworkFetchAction;

#[async_trait]
pub trait NetworkBackend: Send + Sync {
    async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, CapabilityError>;
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
    async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, CapabilityError> {
        self.network_backend().fetch_url(url).await
    }
}

pub struct DenyNetworkBackend;

#[async_trait]
impl NetworkBackend for DenyNetworkBackend {
    async fn fetch_url(&self, _url: &str) -> Result<Vec<u8>, CapabilityError> {
        Err(CapabilityError::Denied)
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

#[async_trait]
impl NetworkBackend for StaticNetworkBackend {
    async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, CapabilityError> {
        self.responses
            .get(url)
            .cloned()
            .ok_or(CapabilityError::Denied)
    }
}

#[async_trait]
impl ActionExecutor<NetworkFetchAction> for dyn NetworkBackend + '_ {
    type Output = Vec<u8>;

    async fn execute(
        &self,
        granted: Granted<NetworkFetchAction>,
    ) -> Result<Self::Output, ExecutionError> {
        self.fetch_url(&granted.action().url)
            .await
            .map_err(ExecutionError::Capability)
    }
}
