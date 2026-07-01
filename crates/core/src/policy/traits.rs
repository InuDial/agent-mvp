use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use mvp_contract::{GrantRecord, GrantSource, PolicyGrant, PolicyOutcome, PolicyReport};

use crate::{
    action::Action,
    error::AuthorizationError,
    policy::{Granted, PolicyContextFactory},
};

#[async_trait]
pub trait Policy<F: PolicyContextFactory, A: Action>: Send + Sync {
    fn name(&self) -> &'static str;

    async fn grant(&self, ctx: &F::Context<'_>, action: &A) -> PolicyGrant;
}

#[async_trait]
pub trait PolicyAny<F: PolicyContextFactory>: Send + Sync {
    fn name(&self) -> &'static str;

    async fn grant(&self, ctx: &F::Context<'_>, action: &dyn Action) -> PolicyGrant;
}

#[async_trait]
pub trait PolicyEngine<F: PolicyContextFactory>: Sync {
    async fn decide<A: Action>(&self, ctx: &F::Context<'_>, action: &A) -> PolicyReport;

    async fn grant<A: Action>(
        &self,
        ctx: &F::Context<'_>,
        action: A,
    ) -> Result<Granted<A>, AuthorizationError> {
        grant_with_engine(self, ctx, action).await
    }
}

pub async fn grant_with_engine<F, E, A>(
    engine: &E,
    ctx: &F::Context<'_>,
    action: A,
) -> Result<Granted<A>, AuthorizationError>
where
    F: PolicyContextFactory,
    E: PolicyEngine<F> + ?Sized,
    A: Action,
{
    let action_kind = action.audit_kind();
    let resource = action.audit_resource();
    let report = engine.decide(ctx, &action).await;

    match report.into_outcome() {
        PolicyOutcome::Allow {
            policy_name,
            policy_id,
            reason,
        } => {
            let record = GrantRecord::allow(
                next_grant_id(),
                action_kind,
                resource,
                policy_name,
                policy_id,
                reason,
            );
            Ok(Granted::new(record, action))
        }
        PolicyOutcome::Deny {
            source:
                GrantSource::Policy {
                    policy_name,
                    policy_id,
                },
            reason,
        } => {
            let reason = reason.or_else(|| Some("policy denied action".into()));
            Err(AuthorizationError::Denied(
                GrantRecord::deny_from_policy(
                    action_kind,
                    resource,
                    policy_name,
                    policy_id,
                    reason,
                )
                .into(),
            ))
        }
        PolicyOutcome::Deny {
            source: GrantSource::NoMatchingPolicy,
            reason,
        } => {
            let reason = reason.or_else(|| Some("No matching policy.".into()));
            Err(AuthorizationError::Denied(
                GrantRecord::deny_without_match(action_kind, resource, reason).into(),
            ))
        }
    }
}

#[async_trait]
pub trait HasPolicyEngine: Sync {
    type PolicyCxFactory: PolicyContextFactory;
    type PolicyEngine<'a>: super::PolicyEngine<Self::PolicyCxFactory>
    where
        Self: 'a;

    fn policy_engine(&self) -> &Self::PolicyEngine<'_>;
}

static NEXT_GRANT_ID: AtomicU64 = AtomicU64::new(1);

fn next_grant_id() -> mvp_contract::GrantId {
    mvp_contract::GrantId::from_raw(NEXT_GRANT_ID.fetch_add(1, Ordering::Relaxed))
}
