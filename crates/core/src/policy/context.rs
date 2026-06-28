use std::path::Path;

use async_trait::async_trait;
use mvp_contract::Capabilities;

use crate::{
    action::{Action, ActionExecutor},
    error::ExecutionError,
};

use super::Granted;

pub trait PolicyContext: Send + Sync {
    fn capabilities(&self) -> &Capabilities;
}

pub trait PolicyContextFactory: 'static {
    type Context<'a>: PolicyContext;
}

pub type PolicyContextFor<'a, P> =
    <<P as HasPolicyEngine>::PolicyCxFactory as PolicyContextFactory>::Context<'a>;

pub trait WorkspacePolicyContext: PolicyContext {
    fn workspace_root(&self) -> &Path;
}

#[async_trait]
pub trait HasPolicyEngine: Sync {
    type PolicyCxFactory: PolicyContextFactory;
    type PolicyEngine<'a>: super::PolicyEngine<Self::PolicyCxFactory>
    where
        Self: 'a;

    fn policy_engine(&self) -> &Self::PolicyEngine<'_>;

    async fn execute_granted<A, E>(
        &self,
        granted: Granted<A>,
        executor: &E,
    ) -> Result<E::Output, ExecutionError>
    where
        Self: Sized,
        A: Action,
        E: ActionExecutor<A> + ?Sized,
    {
        granted.execute_with(executor).await
    }
}
