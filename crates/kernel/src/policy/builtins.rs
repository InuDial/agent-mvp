use async_trait::async_trait;
use mvp_contract::PolicyGrant;
use mvp_core::{
    action::Action,
    policy::{PolicyAny, PolicyContext, PolicyContextFactory},
};

pub struct CapabilityEnvelopePolicy;

#[async_trait]
impl<F> PolicyAny<F> for CapabilityEnvelopePolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "policy.capability_envelope"
    }

    async fn grant(&self, ctx: &F::Context<'_>, action: &dyn Action) -> PolicyGrant {
        if ctx.capabilities().contains(action.capabilities()) {
            PolicyGrant::abstain(Some("action is within declared capability envelope".into()))
                .with_predicate(format!(
                    "effective_capabilities contains action_capabilities: {} contains {}",
                    ctx.capabilities().bits(),
                    action.capabilities().bits()
                ))
        } else {
            PolicyGrant::deny(Some("action exceeds declared capability envelope".into()))
                .with_predicate(format!(
                    "effective_capabilities contains action_capabilities: {} contains {}",
                    ctx.capabilities().bits(),
                    action.capabilities().bits()
                ))
        }
    }
}

pub struct AllowAllPolicy;

#[async_trait]
impl<F: PolicyContextFactory> PolicyAny<F> for AllowAllPolicy {
    fn name(&self) -> &'static str {
        "policy.default_allow"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, _action: &dyn Action) -> PolicyGrant {
        PolicyGrant::allow(Some("default allow policy granted action".into()))
            .with_predicate("default allow")
    }
}
