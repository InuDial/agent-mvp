use anymap::{Map as AnyMap, any::Any as AnyMapAny};
use async_trait::async_trait;
use std::{any::Any, collections::VecDeque, marker::PhantomData};
use tracing::Instrument;

use crate::action::Action;
use crate::error::AuthorizationError;
use crate::tool::next_grant_id;
use crate::{audit, policy::PolicyContextFactory};

use super::{
    GrantRecord, Granted, Policy, PolicyAny, PolicyContext, PolicyDecision, PolicyEngine,
    PolicyGrant, PolicyId,
};

type SyncAnyMap = AnyMap<dyn AnyMapAny + Send + Sync>;

pub struct PolicyAnyWrapper<F: PolicyContextFactory, A: Action, P: Policy<F, A>> {
    inner: P,
    _phantom_data: PhantomData<(fn() -> F, A)>,
}

impl<F, A, P> PolicyAnyWrapper<F, A, P>
where
    F: PolicyContextFactory,
    A: Action,
    P: Policy<F, A>,
{
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            _phantom_data: PhantomData,
        }
    }
}

#[async_trait]
impl<F, A, P> PolicyAny<F> for PolicyAnyWrapper<F, A, P>
where
    F: PolicyContextFactory,
    A: Action,
    P: Policy<F, A>,
{
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    async fn grant(&self, ctx: &F::Context<'_>, action: &dyn Action) -> PolicyGrant {
        let action_any = action as &dyn Any;
        match action_any.downcast_ref::<A>() {
            Some(action) => self.inner.grant(ctx, action).await,
            None => PolicyGrant::abstain(Some("policy does not apply to this action type".into()))
                .with_predicate(format!(
                    "action downcasts to policy action type: {}",
                    std::any::type_name::<A>()
                )),
        }
    }
}

struct RegisteredPolicy<F: PolicyContextFactory, A: Action> {
    id: PolicyId,
    inner: Box<dyn Policy<F, A>>,
}

struct RegisteredPolicyAny<F: PolicyContextFactory> {
    id: PolicyId,
    inner: Box<dyn PolicyAny<F>>,
}

/// Built-in coarse capability gate.
///
/// This policy only vetoes actions that exceed the current effective capability
/// envelope. If the action is inside the envelope, it abstains and lets more
/// specific policies decide whether to allow it.
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

/// Minimal permissive fallback used by the current MVP tool plane.
pub struct DefaultAllowPolicy;

#[async_trait]
impl<F: PolicyContextFactory> PolicyAny<F> for DefaultAllowPolicy {
    fn name(&self) -> &'static str {
        "policy.default_allow"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, _action: &dyn Action) -> PolicyGrant {
        PolicyGrant::allow(Some("default allow policy granted action".into()))
            .with_predicate("default allow")
    }
}

pub struct PolicyPlane<F: PolicyContextFactory> {
    next_policy_id: PolicyId,
    // A -> VecDeque<RegisteredPolicy<Cx, A>>
    policy_entries: SyncAnyMap,
    global_policies_inbound: VecDeque<RegisteredPolicyAny<F>>,
    global_policies_outbound: VecDeque<RegisteredPolicyAny<F>>,
}

impl<F: PolicyContextFactory> Default for PolicyPlane<F> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<F> PolicyEngine<F> for PolicyPlane<F>
where
    F: PolicyContextFactory,
{
    async fn grant<A: Action>(
        &self,
        ctx: &F::Context<'_>,
        action: A,
    ) -> Result<Granted<A>, AuthorizationError> {
        let action_kind = action.audit_kind();
        let resource = action.audit_resource();

        async {
            for policy in &self.global_policies_inbound {
                let policy_grant = policy.inner.grant(ctx, &action).await;
                audit::record_policy_grant(
                    action_kind,
                    &resource,
                    policy.inner.name(),
                    policy.id,
                    "inbound",
                    &policy_grant,
                );
                let (decision, reason) = policy_grant.into_decision_and_reason();
                match decision {
                    PolicyDecision::Allow | PolicyDecision::Abstain => {}
                    PolicyDecision::Deny => {
                        let reason = reason.unwrap_or_else(|| "policy denied action".into());
                        let record = GrantRecord::deny_from_policy(
                            action_kind,
                            resource.clone(),
                            policy.inner.name(),
                            policy.id,
                            Some(reason.clone()),
                        );
                        audit::record_grant(&record);
                        return Err(AuthorizationError::Denied(reason));
                    }
                }
            }

            if let Some(entries) = self
                .policy_entries
                .get::<VecDeque<RegisteredPolicy<F, A>>>()
            {
                for policy in entries {
                    let policy_grant = policy.inner.grant(ctx, &action).await;
                    audit::record_policy_grant(
                        action_kind,
                        &resource,
                        policy.inner.name(),
                        policy.id,
                        "action",
                        &policy_grant,
                    );
                    let (decision, reason) = policy_grant.into_decision_and_reason();
                    match decision {
                        PolicyDecision::Allow => {
                            let grant_id = next_grant_id();
                            let record = GrantRecord::allow(
                                grant_id,
                                action_kind,
                                resource.clone(),
                                policy.inner.name(),
                                policy.id,
                                reason,
                            );
                            audit::record_grant(&record);
                            return Ok(Granted::new(grant_id, action));
                        }
                        PolicyDecision::Deny => {
                            let reason = reason.unwrap_or_else(|| "policy denied action".into());
                            let record = GrantRecord::deny_from_policy(
                                action_kind,
                                resource.clone(),
                                policy.inner.name(),
                                policy.id,
                                Some(reason.clone()),
                            );
                            audit::record_grant(&record);
                            return Err(AuthorizationError::Denied(reason));
                        }
                        PolicyDecision::Abstain => {}
                    }
                }
            }

            for policy in &self.global_policies_outbound {
                let policy_grant = policy.inner.grant(ctx, &action).await;
                audit::record_policy_grant(
                    action_kind,
                    &resource,
                    policy.inner.name(),
                    policy.id,
                    "outbound",
                    &policy_grant,
                );
                let (decision, reason) = policy_grant.into_decision_and_reason();
                match decision {
                    PolicyDecision::Allow => {
                        let grant_id = next_grant_id();
                        let record = GrantRecord::allow(
                            grant_id,
                            action_kind,
                            resource.clone(),
                            policy.inner.name(),
                            policy.id,
                            reason,
                        );
                        audit::record_grant(&record);
                        return Ok(Granted::new(grant_id, action));
                    }
                    PolicyDecision::Deny => {
                        let reason = reason.unwrap_or_else(|| "policy denied action".into());
                        let record = GrantRecord::deny_from_policy(
                            action_kind,
                            resource.clone(),
                            policy.inner.name(),
                            policy.id,
                            Some(reason.clone()),
                        );
                        audit::record_grant(&record);
                        return Err(AuthorizationError::Denied(reason));
                    }
                    PolicyDecision::Abstain => {}
                }
            }

            let reason = "No matching policy.".to_owned();
            let record =
                GrantRecord::deny_without_match(action_kind, resource, Some(reason.clone()));
            audit::record_grant(&record);
            Err(AuthorizationError::Denied(reason))
        }
        .instrument(audit::action_grant_span(action_kind))
        .await
    }
}

impl<F: PolicyContextFactory> PolicyPlane<F> {
    pub fn new() -> Self {
        Self {
            next_policy_id: 1,
            policy_entries: SyncAnyMap::new(),
            global_policies_inbound: VecDeque::new(),
            global_policies_outbound: VecDeque::new(),
        }
    }

    fn allocate_policy_id(&mut self) -> PolicyId {
        let id = self.next_policy_id;
        self.next_policy_id += 1;
        id
    }

    fn get_mut_or_default<A>(&mut self) -> &mut VecDeque<RegisteredPolicy<F, A>>
    where
        A: Action,
    {
        self.policy_entries.entry().or_insert_with(VecDeque::new)
    }

    pub fn prepend<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<F, A> + 'static,
    {
        let registered = RegisteredPolicy {
            id: self.allocate_policy_id(),
            inner: Box::new(policy),
        };
        self.get_mut_or_default::<A>().push_front(registered);
    }

    pub fn append<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<F, A> + 'static,
    {
        let registered = RegisteredPolicy {
            id: self.allocate_policy_id(),
            inner: Box::new(policy),
        };
        self.get_mut_or_default::<A>().push_back(registered);
    }

    pub fn prepend_inbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_inbound
            .push_front(RegisteredPolicyAny {
                id: policy_id,
                inner: Box::new(policy),
            });
    }

    pub fn append_inbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_inbound.push_back(RegisteredPolicyAny {
            id: policy_id,
            inner: Box::new(policy),
        });
    }

    pub fn prepend_outbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_outbound
            .push_front(RegisteredPolicyAny {
                id: policy_id,
                inner: Box::new(policy),
            });
    }

    pub fn append_outbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<F> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_outbound
            .push_back(RegisteredPolicyAny {
                id: policy_id,
                inner: Box::new(policy),
            });
    }
}
