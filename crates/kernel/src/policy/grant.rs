use tracing::Instrument;

use crate::action::{Action, ActionExecutor, AuditResource};
use crate::audit;
use crate::error::ExecutionError;
use crate::tool::GrantId;

use super::{GrantDecision, GrantSource, PolicyId};

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

pub struct Granted<A> {
    grant_id: GrantId,
    action: A,
}

impl<A> Granted<A> {
    pub(crate) fn new(grant_id: GrantId, action: A) -> Self {
        Self { grant_id, action }
    }

    pub fn grant_id(&self) -> GrantId {
        self.grant_id
    }

    pub fn action(&self) -> &A {
        &self.action
    }

    pub fn into_action(self) -> A {
        self.action
    }
}

impl<A> Granted<A>
where
    A: Action,
{
    pub async fn execute_with<E>(self, executor: &E) -> Result<E::Output, ExecutionError>
    where
        E: ActionExecutor<A> + ?Sized,
    {
        let action_kind = self.action.audit_kind();
        let resource = self.action.audit_resource();
        let grant_id = self.grant_id;

        async move {
            audit::execute_start(action_kind, grant_id, &resource);

            let result = executor.execute(self).await;

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
