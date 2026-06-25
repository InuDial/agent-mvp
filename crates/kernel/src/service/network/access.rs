use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;

use crate::action::ExecutableAction;
use crate::error::{CapabilityError, ExecutionError};
use crate::policy::Granted;

use super::action::NetworkFetchAction;

#[async_trait]
pub trait NetworkAccess: Send + Sync {
    async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, CapabilityError>;
}

pub struct DenyNetwork;

#[async_trait]
impl NetworkAccess for DenyNetwork {
    async fn fetch_url(&self, _url: &str) -> Result<Vec<u8>, CapabilityError> {
        Err(CapabilityError::Denied)
    }
}

pub struct StaticNetwork {
    responses: std::collections::BTreeMap<String, Vec<u8>>,
}

impl StaticNetwork {
    pub fn new(responses: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
        }
    }
}

#[async_trait]
impl NetworkAccess for StaticNetwork {
    async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, CapabilityError> {
        self.responses
            .get(url)
            .cloned()
            .ok_or(CapabilityError::Denied)
    }
}

impl ExecutableAction for NetworkFetchAction {
    type Executor<'a> = dyn NetworkAccess + 'a;
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
                .fetch_url(&granted.action.url)
                .await
                .map_err(ExecutionError::Capability)
        })
    }
}
