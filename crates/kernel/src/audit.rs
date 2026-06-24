use tracing::{Span, info, info_span};

use crate::action::AuditResource;
use crate::error::ExecutionError;
use crate::policy::{GrantDecision, GrantRecord, GrantSource};
use crate::tool::{GrantId, ToolRegistration};

pub const AUDIT_TARGET: &str = "tool_plane::audit";

pub fn tool_invocation_span(registration: &ToolRegistration) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.invoke",
        tool = %registration.spec().name,
    )
}

pub fn parse_input_span() -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.parse_input",
    )
}

pub fn execution_span() -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.execution",
    )
}

pub fn final_output_span() -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.final_output",
    )
}

pub fn action_grant_span(action_kind: &str) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "action.grant",
        action = action_kind,
    )
}

pub fn action_execute_span(action_kind: &str, grant_id: GrantId) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "action.execute",
        action = action_kind,
        grant_id = grant_id.get(),
    )
}

pub(crate) fn record_grant(record: &GrantRecord) {
    let event = match record.decision() {
        GrantDecision::Allow(_) => "grant_allow",
        GrantDecision::Deny => "grant_deny",
    };
    let grant_id = match record.decision() {
        GrantDecision::Allow(grant_id) => Some(grant_id.get()),
        GrantDecision::Deny => None,
    };
    let (resource_kind, resource_value) = serialize_resource(record.resource());
    let (policy_name, policy_id, grant_source) = match record.source() {
        GrantSource::Policy {
            policy_name,
            policy_id,
        } => (Some(*policy_name), Some(*policy_id), "policy"),
        GrantSource::NoMatchingPolicy => (None, None, "no_matching_policy"),
    };

    info!(
        target: AUDIT_TARGET,
        event = event,
        action = record.action_kind(),
        grant_id = ?grant_id,
        resource_kind = resource_kind,
        resource = %resource_value,
        grant_source = grant_source,
        policy_name = ?policy_name,
        policy_id = ?policy_id,
        reason = ?record.reason(),
    );
}

pub(crate) fn execute_start(action_kind: &str, grant_id: GrantId, resource: &AuditResource) {
    log_action_event(
        "execute_start",
        action_kind,
        Some(grant_id),
        resource,
        None,
        None,
    );
}

pub(crate) fn execute_finish(action_kind: &str, grant_id: GrantId, resource: &AuditResource) {
    log_action_event(
        "execute_finish",
        action_kind,
        Some(grant_id),
        resource,
        None,
        None,
    );
}

pub(crate) fn execute_error(
    action_kind: &str,
    grant_id: GrantId,
    resource: &AuditResource,
    error: &ExecutionError,
) {
    log_action_event(
        "execute_error",
        action_kind,
        Some(grant_id),
        resource,
        None,
        Some(error),
    );
}

fn log_action_event(
    event: &str,
    action_kind: &str,
    grant_id: Option<GrantId>,
    resource: &AuditResource,
    reason: Option<&str>,
    error: Option<&ExecutionError>,
) {
    let grant_id = grant_id.map(GrantId::get);
    let (resource_kind, resource_value) = serialize_resource(resource);

    info!(
        target: AUDIT_TARGET,
        event = event,
        action = action_kind,
        grant_id = ?grant_id,
        resource_kind = resource_kind,
        resource = %resource_value,
        reason = ?reason,
        error = ?error,
    );
}

fn serialize_resource(resource: &AuditResource) -> (&'static str, String) {
    match resource {
        AuditResource::Path(path) => ("path", path.display().to_string()),
        AuditResource::Value(value) => ("value", value.clone()),
        AuditResource::None => ("none", String::new()),
    }
}
