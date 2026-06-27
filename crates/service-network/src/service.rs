use mvp_kernel::error::ExecutionError;
use mvp_kernel::kernel::{Kernel, PolicyContextFor};
use mvp_kernel::policy::PolicyEngine;
use mvp_kernel::tool::ToolContext;

use crate::{NetworkBackend, NetworkFetchAction};

pub trait HasNetworkService<K>: ToolContext<K>
where
    K: NetworkBackend + Kernel,
{
    fn network(&self) -> NetworkService<'_, K>;
}

/// Network service facade exposed as `ctx.network()`.
///
/// Public methods remain natural and function-like. Internally they follow the
/// same pipeline: construct an action, ask policy to grant it, then execute the
/// granted action. Grant / execute audit stays in the shared policy/action core.
pub struct NetworkService<'a, K>
where
    K: Kernel + NetworkBackend + ?Sized,
{
    kernel: &'a K,
    policy_context: PolicyContextFor<'a, K>,
}

impl<'a, K> NetworkService<'a, K>
where
    K: Kernel + NetworkBackend,
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
            .policy_plane()
            .grant(&self.policy_context, action)
            .await
            .map_err(ExecutionError::Authorization)?;

        granted.execute(self.kernel).await
    }
}
