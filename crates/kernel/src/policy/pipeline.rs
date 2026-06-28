use async_trait::async_trait;
use mvp_contract::{PolicyDecision, PolicyEvaluation, PolicyReport};
use mvp_core::{
    action::Action,
    error::AuthorizationError,
    policy::{Granted, Policy, PolicyAny, PolicyContextFactory, PolicyEngine},
};
use tracing::Instrument;

use crate::audit;

use super::registry::PolicyRegistry;

pub struct PolicyPipeline<F: PolicyContextFactory> {
    registry: PolicyRegistry<F>,
}

impl<F: PolicyContextFactory> Default for PolicyPipeline<F> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<F> PolicyEngine<F> for PolicyPipeline<F>
where
    F: PolicyContextFactory,
{
    async fn decide<A: Action>(&self, ctx: &F::Context<'_>, action: &A) -> PolicyReport {
        let mut evaluations = Vec::new();
        let action_kind = action.audit_kind();
        let resource = action.audit_resource();

        for policy in self.registry.inbound_policies() {
            let policy_grant = policy.inner.grant(ctx, action).await;
            audit::record_policy_grant(
                action_kind,
                &resource,
                policy.inner.name(),
                policy.id,
                "inbound",
                &policy_grant,
            );
            let (decision, reason) = policy_grant.clone().into_decision_and_reason();
            evaluations.push(PolicyEvaluation::new(
                policy.inner.name(),
                policy.id,
                "inbound",
                policy_grant,
            ));
            match decision {
                PolicyDecision::Allow | PolicyDecision::Abstain => {}
                PolicyDecision::Deny => {
                    return PolicyReport::deny_from_policy(
                        evaluations,
                        policy.inner.name(),
                        policy.id,
                        reason,
                    );
                }
            }
        }

        if let Some(entries) = self.registry.action_policies::<A>() {
            for policy in entries {
                let policy_grant = policy.inner.grant(ctx, action).await;
                audit::record_policy_grant(
                    action_kind,
                    &resource,
                    policy.inner.name(),
                    policy.id,
                    "action",
                    &policy_grant,
                );
                let (decision, reason) = policy_grant.clone().into_decision_and_reason();
                evaluations.push(PolicyEvaluation::new(
                    policy.inner.name(),
                    policy.id,
                    "action",
                    policy_grant,
                ));
                match decision {
                    PolicyDecision::Allow => {
                        return PolicyReport::allow(
                            evaluations,
                            policy.inner.name(),
                            policy.id,
                            reason,
                        );
                    }
                    PolicyDecision::Deny => {
                        return PolicyReport::deny_from_policy(
                            evaluations,
                            policy.inner.name(),
                            policy.id,
                            reason,
                        );
                    }
                    PolicyDecision::Abstain => {}
                }
            }
        }

        for policy in self.registry.outbound_policies() {
            let policy_grant = policy.inner.grant(ctx, action).await;
            audit::record_policy_grant(
                action_kind,
                &resource,
                policy.inner.name(),
                policy.id,
                "outbound",
                &policy_grant,
            );
            let (decision, reason) = policy_grant.clone().into_decision_and_reason();
            evaluations.push(PolicyEvaluation::new(
                policy.inner.name(),
                policy.id,
                "outbound",
                policy_grant,
            ));
            match decision {
                PolicyDecision::Allow => {
                    return PolicyReport::allow(
                        evaluations,
                        policy.inner.name(),
                        policy.id,
                        reason,
                    );
                }
                PolicyDecision::Deny => {
                    return PolicyReport::deny_from_policy(
                        evaluations,
                        policy.inner.name(),
                        policy.id,
                        reason,
                    );
                }
                PolicyDecision::Abstain => {}
            }
        }

        PolicyReport::deny_without_match(evaluations, Some("No matching policy.".to_owned()))
    }

    async fn grant<A: Action>(
        &self,
        ctx: &F::Context<'_>,
        action: A,
    ) -> Result<Granted<A>, AuthorizationError> {
        let span = crate::action_grant_span!(action.audit_kind());
        async {
            let granted = mvp_core::policy::grant_with_engine(self, ctx, action).await;
            match &granted {
                Ok(granted) => audit::record_grant(granted.record()),
                Err(error) => {
                    if let Some(record) = error.deny_record() {
                        audit::record_grant(record);
                    }
                }
            }
            granted
        }
        .instrument(span)
        .await
    }
}

impl<F: PolicyContextFactory> PolicyPipeline<F> {
    pub fn new() -> Self {
        Self {
            registry: PolicyRegistry::new(),
        }
    }

    pub fn prepend<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<F, A> + 'static,
    {
        self.registry.prepend_action::<A, P>(policy);
    }

    pub fn append<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<F, A> + 'static,
    {
        self.registry.append_action::<A, P>(policy);
    }

    pub fn prepend_inbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        self.registry.prepend_inbound(policy);
    }

    pub fn append_inbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        self.registry.append_inbound(policy);
    }

    pub fn prepend_outbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        self.registry.prepend_outbound(policy);
    }

    pub fn append_outbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        self.registry.append_outbound(policy);
    }
}
