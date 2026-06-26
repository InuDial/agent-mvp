use tracing::Instrument;

use crate::action::{AuditResource, ExecutableAction};
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
    ) -> Result<A::Output, ExecutionError>
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
