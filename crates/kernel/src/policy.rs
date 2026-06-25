use anymap::{Map as AnyMap, any::Any as AnyMapAny};
use async_trait::async_trait;
use mvp_contract::Capabilities;
use std::{any::Any, collections::VecDeque, marker::PhantomData, path::PathBuf};
use tracing::Instrument;

use crate::action::{Action, AuditResource, ExecutableAction};
use crate::audit;
use crate::error::AuthorizationError;
use crate::tool::{GrantId, next_grant_id};

type SyncAnyMap = AnyMap<dyn AnyMapAny + Send + Sync>; // typed AnyMap for shareable policy buckets

pub type PolicyId = u64;

/// The decision returned by a policy rule.
#[derive(Clone, Debug)]
pub enum PolicyDecision {
    Allow { reason: Option<String> },
    Deny { reason: Option<String> },
    Abstain,
    // Maybe here should be a `Ask` Decision, the composite behaviour may
    // be similar to JavaScript `null` and `undefined`
    //
    // Now simply Allow = Deny > Abstain
}

#[derive(Clone, Debug)]
pub enum GrantDecision {
    Allow(GrantId),
    Deny,
}

#[derive(Clone, Debug)]
pub enum GrantSource {
    Policy {
        policy_name: &'static str,
        policy_id: PolicyId,
    },
    NoMatchingPolicy,
}

#[derive(Clone, Debug)]
pub struct GrantRecord {
    decision: GrantDecision,
    action_kind: &'static str,
    resource: AuditResource,
    source: GrantSource,
    reason: Option<String>,
}

impl GrantRecord {
    pub(crate) fn allow(
        grant_id: GrantId,
        action_kind: &'static str,
        resource: AuditResource,
        policy_name: &'static str,
        policy_id: PolicyId,
        reason: Option<String>,
    ) -> Self {
        Self {
            decision: GrantDecision::Allow(grant_id),
            action_kind,
            resource,
            source: GrantSource::Policy {
                policy_name,
                policy_id,
            },
            reason,
        }
    }

    pub(crate) fn deny_from_policy(
        action_kind: &'static str,
        resource: AuditResource,
        policy_name: &'static str,
        policy_id: PolicyId,
        reason: Option<String>,
    ) -> Self {
        Self {
            decision: GrantDecision::Deny,
            action_kind,
            resource,
            source: GrantSource::Policy {
                policy_name,
                policy_id,
            },
            reason,
        }
    }

    pub(crate) fn deny_without_match(
        action_kind: &'static str,
        resource: AuditResource,
        reason: Option<String>,
    ) -> Self {
        Self {
            decision: GrantDecision::Deny,
            action_kind,
            resource,
            source: GrantSource::NoMatchingPolicy,
            reason,
        }
    }

    pub fn decision(&self) -> &GrantDecision {
        &self.decision
    }

    pub fn action_kind(&self) -> &'static str {
        self.action_kind
    }

    pub fn resource(&self) -> &AuditResource {
        &self.resource
    }

    pub fn source(&self) -> &GrantSource {
        &self.source
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

pub trait PolicyContext: 'static + Send + Sync {}

pub trait CapabilityEnvelopeContext: PolicyContext {
    fn capabilities(&self) -> Capabilities;
}

#[derive(Clone, Debug)]
pub struct KernelPolicyContext {
    capabilities: Capabilities,
    workspace_root: PathBuf,
}

impl KernelPolicyContext {
    pub fn new(capabilities: Capabilities, workspace_root: PathBuf) -> Self {
        Self {
            capabilities,
            workspace_root,
        }
    }

    pub fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }
}

impl PolicyContext for KernelPolicyContext {}

impl CapabilityEnvelopeContext for KernelPolicyContext {
    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
}

#[async_trait]
pub trait Policy<Ctx: PolicyContext, A: Action>: Send + Sync {
    fn name(&self) -> &'static str;

    async fn grant(&self, ctx: &Ctx, action: &A) -> PolicyDecision;
}

#[async_trait]
pub trait PolicyAny<Ctx: PolicyContext>: Send + Sync {
    fn name(&self) -> &'static str;

    async fn grant(&self, ctx: &Ctx, action: &dyn Action) -> PolicyDecision;
}

pub struct PolicyAnyWrapper<Ctx: PolicyContext, A: Action, P: Policy<Ctx, A>> {
    inner: P,
    _phantom_data: PhantomData<(Ctx, A)>,
}

impl<Ctx, A, P> PolicyAnyWrapper<Ctx, A, P>
where
    Ctx: PolicyContext,
    A: Action,
    P: Policy<Ctx, A>,
{
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            _phantom_data: PhantomData,
        }
    }
}

#[async_trait]
impl<Ctx, A, P> PolicyAny<Ctx> for PolicyAnyWrapper<Ctx, A, P>
where
    Ctx: PolicyContext,
    A: Action,
    P: Policy<Ctx, A>,
{
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    async fn grant(&self, ctx: &Ctx, action: &dyn Action) -> PolicyDecision {
        let action_any = action as &dyn Any;
        match action_any.downcast_ref::<A>() {
            Some(action) => self.inner.grant(ctx, action).await,
            None => PolicyDecision::Abstain,
        }
    }
}

struct RegisteredPolicy<Ctx: PolicyContext, A: Action> {
    id: PolicyId,
    inner: Box<dyn Policy<Ctx, A>>,
}

struct RegisteredPolicyAny<Ctx: PolicyContext> {
    id: PolicyId,
    inner: Box<dyn PolicyAny<Ctx>>,
}

pub struct Granted<A> {
    pub grant_id: GrantId,
    pub action: A,
}

impl<A> Granted<A>
where
    A: ExecutableAction,
{
    pub async fn execute<'a>(
        self,
        executor: &'a A::Executor<'a>,
    ) -> Result<A::Output, crate::error::ExecutionError>
    where
        A: 'a,
    {
        let action_kind = self.action.audit_kind();
        let resource = self.action.audit_resource();
        let grant_id = self.grant_id;

        async move {
            audit::execute_start(action_kind, grant_id, &resource);

            let result = A::execute(executor, self).await;

            match &result {
                Ok(_) => audit::execute_finish(action_kind, grant_id, &resource),
                Err(error) => audit::execute_error(action_kind, grant_id, &resource, error),
            }

            result
        }
        .instrument(audit::action_execute_span(action_kind, grant_id))
        .await
    }
}

/// Built-in coarse capability gate.
///
/// This policy only vetoes actions that exceed the current effective capability
/// envelope. If the action is inside the envelope, it abstains and lets more
/// specific policies decide whether to allow it.
pub struct CapabilityEnvelopePolicy;

#[async_trait]
impl<Ctx> PolicyAny<Ctx> for CapabilityEnvelopePolicy
where
    Ctx: CapabilityEnvelopeContext,
{
    fn name(&self) -> &'static str {
        "policy.capability_envelope"
    }

    async fn grant(&self, ctx: &Ctx, action: &dyn Action) -> PolicyDecision {
        if ctx.capabilities().contains(action.capabilities()) {
            PolicyDecision::Abstain
        } else {
            PolicyDecision::Deny {
                reason: Some("action exceeds declared capability envelope".into()),
            }
        }
    }
}

/// Minimal permissive fallback used by the current MVP tool plane.
pub struct DefaultAllowPolicy;

#[async_trait]
impl<Ctx: PolicyContext> PolicyAny<Ctx> for DefaultAllowPolicy {
    fn name(&self) -> &'static str {
        "policy.default_allow"
    }

    async fn grant(&self, _ctx: &Ctx, _action: &dyn Action) -> PolicyDecision {
        PolicyDecision::Allow { reason: None }
    }
}

#[async_trait]
pub trait PolicyEngine {
    type Ctx: PolicyContext;

    async fn grant<A: Action>(
        &self,
        ctx: &Self::Ctx,
        action: A,
    ) -> Result<Granted<A>, AuthorizationError>;
}

pub struct PolicyPlane<Ctx: PolicyContext> {
    next_policy_id: PolicyId,
    policy_entries: SyncAnyMap,
    global_policies_inbound: VecDeque<RegisteredPolicyAny<Ctx>>,
    global_policies_outbound: VecDeque<RegisteredPolicyAny<Ctx>>,
}

impl<Ctx: PolicyContext> Default for PolicyPlane<Ctx> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<Ctx> PolicyEngine for PolicyPlane<Ctx>
where
    Ctx: PolicyContext,
{
    type Ctx = Ctx;
    async fn grant<A: Action>(
        &self,
        ctx: &Ctx,
        action: A,
    ) -> Result<Granted<A>, AuthorizationError> {
        let action_kind = action.audit_kind();
        let resource = action.audit_resource();

        async {
            for policy in &self.global_policies_inbound {
                let decision = policy.inner.grant(ctx, &action).await;
                match decision {
                    PolicyDecision::Allow { .. } | PolicyDecision::Abstain => {}
                    PolicyDecision::Deny { reason } => {
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
                .get::<VecDeque<RegisteredPolicy<Ctx, A>>>()
            {
                for policy in entries {
                    let decision = policy.inner.grant(ctx, &action).await;
                    match decision {
                        PolicyDecision::Allow { reason } => {
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
                            return Ok(Granted { grant_id, action });
                        }
                        PolicyDecision::Deny { reason } => {
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
                let decision = policy.inner.grant(ctx, &action).await;
                match decision {
                    PolicyDecision::Allow { reason } => {
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
                        return Ok(Granted { grant_id, action });
                    }
                    PolicyDecision::Deny { reason } => {
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

impl<Ctx: PolicyContext> PolicyPlane<Ctx> {
    pub fn new() -> Self {
        Self {
            next_policy_id: 1,
            policy_entries: AnyMap::new(),
            global_policies_inbound: VecDeque::new(),
            global_policies_outbound: VecDeque::new(),
        }
    }

    fn allocate_policy_id(&mut self) -> PolicyId {
        let id = self.next_policy_id;
        self.next_policy_id += 1;
        id
    }

    fn get_mut_or_default<A>(&mut self) -> &mut VecDeque<RegisteredPolicy<Ctx, A>>
    where
        A: Action,
    {
        self.policy_entries.entry().or_insert_with(VecDeque::new)
    }

    pub fn prepend<A, P>(&mut self, policy: P)
    where
        A: Action,
        P: Policy<Ctx, A> + 'static,
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
        P: Policy<Ctx, A> + 'static,
    {
        let registered = RegisteredPolicy {
            id: self.allocate_policy_id(),
            inner: Box::new(policy),
        };
        self.get_mut_or_default::<A>().push_back(registered);
    }

    pub fn prepend_inbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<Ctx> + 'static,
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
        P: PolicyAny<Ctx> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_inbound.push_back(RegisteredPolicyAny {
            id: policy_id,
            inner: Box::new(policy),
        });
    }

    pub fn prepend_outbound<P>(&mut self, policy: P)
    where
        P: PolicyAny<Ctx> + 'static,
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
        P: PolicyAny<Ctx> + 'static,
    {
        let policy_id = self.allocate_policy_id();
        self.global_policies_outbound
            .push_back(RegisteredPolicyAny {
                id: policy_id,
                inner: Box::new(policy),
            });
    }
}
