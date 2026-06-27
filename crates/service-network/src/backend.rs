use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;

use mvp_kernel::action::ExecutableAction;
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

impl ExecutableAction for NetworkFetchAction {
    type Executor<'a> = dyn NetworkBackend + 'a;
    type Output = Vec<u8>;

    fn execute<'a>(
        network: &'a Self::Executor<'a>,
        granted: Granted<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ExecutionError>> + Send + 'a>>
    where
        Self: 'a,
    {
        Box::pin(async move {
            network
                .fetch_url(&granted.action().url)
                .await
                .map_err(ExecutionError::Capability)
        })
    }
}
