use async_trait::async_trait;

use crate::action::Action;
use crate::error::AuthorizationError;
use crate::policy::PolicyContextFactory;

use super::{Granted, PolicyDecision};

#[async_trait]
pub trait Policy<F: PolicyContextFactory, A: Action>: Send + Sync {
    fn name(&self) -> &'static str;

    async fn grant(&self, ctx: &F::Context<'_>, action: &A) -> PolicyDecision;
}

#[async_trait]
pub trait PolicyAny<F: PolicyContextFactory>: Send + Sync {
    fn name(&self) -> &'static str;

    async fn grant(&self, ctx: &F::Context<'_>, action: &dyn Action) -> PolicyDecision;
}

#[async_trait]
pub trait PolicyEngine<F: PolicyContextFactory>: Sync {
    async fn grant<A: Action>(
        &self,
        ctx: &F::Context<'_>,
        action: A,
    ) -> Result<Granted<A>, AuthorizationError>;
}
