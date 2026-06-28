use mvp_core::{
    error::ExecutionError,
    policy::{HasPolicyEngine, PolicyContextFor, PolicyEngine},
};

use crate::{NetworkBackend, NetworkFetchAction};

pub trait HasNetworkAccess {
    type Host: NetworkBackend + HasPolicyEngine;

    fn network(&self) -> NetworkAccess<'_, Self::Host>;
}

/// Network access facade exposed as `ctx.network()`.
///
/// Public methods remain natural and function-like. Internally they follow the
/// same pipeline: construct an action, ask policy to grant it, then execute the
/// granted action. Concrete runtimes can wrap grant / execute with audit.
pub struct NetworkAccess<'a, K>
where
    K: HasPolicyEngine + NetworkBackend + ?Sized,
{
    kernel: &'a K,
    policy_context: PolicyContextFor<'a, K>,
}

impl<'a, K> NetworkAccess<'a, K>
where
    K: HasPolicyEngine + NetworkBackend,
{
    pub fn new(kernel: &'a K, policy_context: PolicyContextFor<'a, K>) -> Self {
        Self {
            kernel,
            policy_context,
        }
    }

    pub async fn fetch_url(&self, url: &str) -> Result<Vec<u8>, ExecutionError> {
        let action = NetworkFetchAction::new(url.to_owned());
        let granted = self
            .kernel
            .policy_engine()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        let executor: &dyn NetworkBackend = self.kernel;
        self.kernel.execute_granted(granted, executor).await
    }
}
