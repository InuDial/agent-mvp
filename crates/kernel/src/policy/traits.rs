use async_trait::async_trait;
use tracing::Instrument;

use crate::action::Action;
use crate::audit;
use crate::error::AuthorizationError;
use crate::policy::PolicyContextFactory;
use crate::tool::next_grant_id;

use super::{GrantRecord, GrantSource, Granted, PolicyGrant, PolicyOutcome, PolicyReport};

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

    /// The auto implemented method
    async fn grant<A: Action>(
        &self,
        ctx: &F::Context<'_>,
        action: A,
    ) -> Result<Granted<A>, AuthorizationError> {
        let action_kind = action.audit_kind();
        let resource = action.audit_resource();

        async {
            let report = self.decide(ctx, &action).await;

            for evaluation in report.evaluations() {
                audit::record_policy_grant(
                    action_kind,
                    &resource,
                    evaluation.policy_name(),
                    evaluation.policy_id(),
                    evaluation.policy_stage(),
                    evaluation.grant(),
                );
            }

            match report.into_outcome() {
                PolicyOutcome::Allow {
                    policy_name,
                    policy_id,
                    reason,
                } => {
                    let grant_id = next_grant_id();
                    let record = GrantRecord::allow(
                        grant_id,
                        action_kind,
                        resource.clone(),
                        policy_name,
                        policy_id,
                        reason,
                    );
                    audit::record_grant(&record);
                    Ok(Granted::new(grant_id, action))
                }
                PolicyOutcome::Deny {
                    source:
                        GrantSource::Policy {
                            policy_name,
                            policy_id,
                        },
                    reason,
                } => {
                    let reason = reason.unwrap_or_else(|| "policy denied action".into());
                    let record = GrantRecord::deny_from_policy(
                        action_kind,
                        resource,
                        policy_name,
                        policy_id,
                        Some(reason.clone()),
                    );
                    audit::record_grant(&record);
                    Err(AuthorizationError::Denied(reason))
                }
                PolicyOutcome::Deny {
                    source: GrantSource::NoMatchingPolicy,
                    reason,
                } => {
                    let reason = reason.unwrap_or_else(|| "No matching policy.".into());
                    let record = GrantRecord::deny_without_match(
                        action_kind,
                        resource,
                        Some(reason.clone()),
                    );
                    audit::record_grant(&record);
                    Err(AuthorizationError::Denied(reason))
                }
            }
        }
        .instrument(audit::action_grant_span(action_kind))
        .await
    }
}
