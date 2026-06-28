use async_trait::async_trait;
use mvp_kernel::policy::{Policy, PolicyContextFactory, PolicyGrant};

use crate::{MontySessionLoadAction, MontySessionSaveAction};

pub struct AllowMontySessionPolicy;

#[async_trait]
impl<F> Policy<F, MontySessionLoadAction> for AllowMontySessionPolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "monty.allow_session_load"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, _action: &MontySessionLoadAction) -> PolicyGrant {
        PolicyGrant::allow(Some("Monty session load allowed".into()))
            .with_predicate("session load policy present")
    }
}

#[async_trait]
impl<F> Policy<F, MontySessionSaveAction> for AllowMontySessionPolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "monty.allow_session_save"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, _action: &MontySessionSaveAction) -> PolicyGrant {
        PolicyGrant::allow(Some("Monty session save allowed".into()))
            .with_predicate("session save policy present")
    }
}
