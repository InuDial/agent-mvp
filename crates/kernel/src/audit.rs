//! Structured audit helpers for invocation, authorization, and execution.
//!
//! Final grant and execution events are emitted at INFO. Per-policy evaluation
//! records are emitted at DEBUG so operators can inspect why a policy allowed,
//! denied, or abstained without making every intermediate decision part of the
//! default audit stream.

use std::fmt::Debug;

use tracing::{Span, debug, info, info_span, warn};

use crate::action::AuditResource;
use crate::error::ExecutionError;
use crate::policy::{
    GrantDecision, GrantRecord, GrantSource, PolicyDecision, PolicyGrant, PolicyId,
};
use crate::tool::{GrantId, ToolRegistration};
use mvp_contract::Capabilities;

pub const AUDIT_TARGET: &str = "mvp::audit";

// Span names describe the nested operation timeline rendered by trace viewers.
const SPAN_TOOL_INVOKE: &str = "tool.invoke";
const SPAN_TOOL_PARSE_INPUT: &str = "tool.parse_input";
const SPAN_TOOL_EXECUTION: &str = "tool.execution";
const SPAN_ACTION_GRANT: &str = "action.grant";
const SPAN_ACTION_EXECUTE: &str = "action.execute";

// Event names describe individual audit facts that log backends query.
const EVENT_TOOL_CAPABILITIES_OVERRIDE: &str = "tool.capabilities_override";
const EVENT_TOOL_NESTED_SCOPE: &str = "tool.nested_scope";
const EVENT_GRANT_ALLOW: &str = "grant.allow";
const EVENT_GRANT_DENY: &str = "grant.deny";
const EVENT_POLICY_EVALUATE: &str = "policy.evaluate";
const EVENT_EXECUTE_START: &str = "execute.start";
const EVENT_EXECUTE_FINISH: &str = "execute.finish";
const EVENT_EXECUTE_ERROR: &str = "execute.error";

// Phases are coarse buckets shared across events and spans.
const PHASE_TOOL: &str = "tool";
const PHASE_POLICY: &str = "policy";
const PHASE_GRANT: &str = "grant";
const PHASE_EXECUTE: &str = "execute";

// Grant sources identify why authorization reached a final allow/deny record.
const GRANT_SOURCE_POLICY: &str = "policy";
const GRANT_SOURCE_NO_MATCHING_POLICY: &str = "no_matching_policy";

// Policy decisions are per-policy outcomes, not final authorization facts.
const POLICY_DECISION_ALLOW: &str = "allow";
const POLICY_DECISION_DENY: &str = "deny";
const POLICY_DECISION_ABSTAIN: &str = "abstain";

// Resource kinds normalize the type of value carried by the `resource` field.
const RESOURCE_KIND_PATH: &str = "path";
const RESOURCE_KIND_VALUE: &str = "value";
const RESOURCE_KIND_NONE: &str = "none";

pub fn tool_invocation_span<P: Debug>(tool_path: &P, registration: &ToolRegistration) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        SPAN_TOOL_INVOKE,
        tool_path = ?tool_path,
        tool_name = %registration.spec().name,
    )
}

pub(crate) fn record_tool_capabilities_override<P: Debug>(
    tool_path: &P,
    registration: &ToolRegistration,
    declared_capabilities: Capabilities,
    effective_capabilities: Capabilities,
) {
    let exceeds_declared = !declared_capabilities.contains(effective_capabilities);

    if exceeds_declared {
        info!(
            target: AUDIT_TARGET,
            event = EVENT_TOOL_CAPABILITIES_OVERRIDE,
            phase = PHASE_TOOL,
            tool_path = ?tool_path,
            tool_name = %registration.spec().name,
            declared_capabilities = declared_capabilities.bits(),
            effective_capabilities = effective_capabilities.bits(),
        );
    }
}

pub fn record_nested_capability_override<P: Debug, C: Debug>(
    parent_tool_path: &P,
    parent_registration: &ToolRegistration,
    child_tool_path: &C,
    parent_effective_capabilities: Capabilities,
    requested_capabilities_override: Option<Capabilities>,
    actual_child_capabilities: Option<Capabilities>,
    attempted_expand: bool,
) {
    let requested_capabilities_override =
        requested_capabilities_override.map(|capabilities| capabilities.bits());
    let actual_child_capabilities =
        actual_child_capabilities.map(|capabilities| capabilities.bits());
    let requested_capabilities_override_present = requested_capabilities_override.is_some();
    let actual_child_capabilities_present = actual_child_capabilities.is_some();
    let requested_capabilities_override = requested_capabilities_override.unwrap_or(0);
    let actual_child_capabilities = actual_child_capabilities.unwrap_or(0);

    if attempted_expand {
        warn!(
            target: AUDIT_TARGET,
            event = EVENT_TOOL_NESTED_SCOPE,
            phase = PHASE_TOOL,
            parent_tool_path = ?parent_tool_path,
            parent_tool_name = %parent_registration.spec().name,
            child_tool_path = ?child_tool_path,
            parent_effective_capabilities = parent_effective_capabilities.bits(),
            requested_capabilities_override = requested_capabilities_override,
            requested_capabilities_override_present = requested_capabilities_override_present,
            actual_child_capabilities = actual_child_capabilities,
            actual_child_capabilities_present = actual_child_capabilities_present,
            attempted_expand = true,
        );
    } else {
        info!(
            target: AUDIT_TARGET,
            event = EVENT_TOOL_NESTED_SCOPE,
            phase = PHASE_TOOL,
            parent_tool_path = ?parent_tool_path,
            parent_tool_name = %parent_registration.spec().name,
            child_tool_path = ?child_tool_path,
            parent_effective_capabilities = parent_effective_capabilities.bits(),
            requested_capabilities_override = requested_capabilities_override,
            requested_capabilities_override_present = requested_capabilities_override_present,
            actual_child_capabilities = actual_child_capabilities,
            actual_child_capabilities_present = actual_child_capabilities_present,
            attempted_expand = false,
        );
    }
}

pub fn parse_input_span() -> Span {
    info_span!(
        target: AUDIT_TARGET,
        SPAN_TOOL_PARSE_INPUT,
    )
}

pub fn execution_span() -> Span {
    info_span!(
        target: AUDIT_TARGET,
        SPAN_TOOL_EXECUTION,
    )
}

pub fn action_grant_span(action_kind: &str) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        SPAN_ACTION_GRANT,
        phase = PHASE_GRANT,
        action = action_kind,
    )
}

pub fn action_execute_span(action_kind: &str, grant_id: GrantId, resource: &AuditResource) -> Span {
    let (resource_kind, resource_value) = serialize_resource(resource);
    info_span!(
        target: AUDIT_TARGET,
        SPAN_ACTION_EXECUTE,
        phase = PHASE_EXECUTE,
        action = action_kind,
        grant_id = grant_id.get(),
        resource_kind = resource_kind,
        resource = %resource_value,
    )
}

pub(crate) fn record_grant(record: &GrantRecord) {
    let event = match record.decision() {
        GrantDecision::Allow(_) => EVENT_GRANT_ALLOW,
        GrantDecision::Deny => EVENT_GRANT_DENY,
    };
    let grant_id = match record.decision() {
        GrantDecision::Allow(grant_id) => Some(grant_id.get()),
        GrantDecision::Deny => None,
    };
    let grant_id_present = grant_id.is_some();
    let grant_id = grant_id.unwrap_or(0);
    let (resource_kind, resource_value) = serialize_resource(record.resource());
    let (policy_name, policy_id, grant_source) = match record.source() {
        GrantSource::Policy {
            policy_name,
            policy_id,
        } => (Some(*policy_name), Some(*policy_id), GRANT_SOURCE_POLICY),
        GrantSource::NoMatchingPolicy => (None, None, GRANT_SOURCE_NO_MATCHING_POLICY),
    };

    info!(
        target: AUDIT_TARGET,
        event = event,
        phase = PHASE_GRANT,
        action = record.action_kind(),
        grant_id = grant_id,
        grant_id_present = grant_id_present,
        resource_kind = resource_kind,
        resource = %resource_value,
        grant_source = grant_source,
        policy_name = policy_name.unwrap_or(""),
        policy_id = policy_id.unwrap_or(0),
        policy_present = policy_name.is_some(),
        reason = record.reason().unwrap_or(""),
        reason_present = record.reason().is_some(),
    );
}

pub(crate) fn record_policy_grant(
    action_kind: &str,
    resource: &AuditResource,
    policy_name: &'static str,
    policy_id: PolicyId,
    policy_stage: &'static str,
    policy_grant: &PolicyGrant,
) {
    let decision = match policy_grant.decision() {
        PolicyDecision::Allow => POLICY_DECISION_ALLOW,
        PolicyDecision::Deny => POLICY_DECISION_DENY,
        PolicyDecision::Abstain => POLICY_DECISION_ABSTAIN,
    };
    let (resource_kind, resource_value) = serialize_resource(resource);

    debug!(
        target: AUDIT_TARGET,
        event = EVENT_POLICY_EVALUATE,
        phase = PHASE_POLICY,
        action = action_kind,
        resource_kind = resource_kind,
        resource = %resource_value,
        policy_name = policy_name,
        policy_id = policy_id,
        policy_stage = policy_stage,
        decision = decision,
        reason = policy_grant.reason().unwrap_or(""),
        reason_present = policy_grant.reason().is_some(),
        predicate = policy_grant.predicate().unwrap_or(""),
        predicate_present = policy_grant.predicate().is_some(),
    );
}

pub(crate) fn execute_start(action_kind: &str, grant_id: GrantId, resource: &AuditResource) {
    log_action_event(
        EVENT_EXECUTE_START,
        action_kind,
        Some(grant_id),
        resource,
        None,
        None,
    );
}

pub(crate) fn execute_finish(action_kind: &str, grant_id: GrantId, resource: &AuditResource) {
    log_action_event(
        EVENT_EXECUTE_FINISH,
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
        EVENT_EXECUTE_ERROR,
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
    let grant_id_present = grant_id.is_some();
    let grant_id = grant_id.unwrap_or(0);
    let (resource_kind, resource_value) = serialize_resource(resource);
    let error = error.map(|error| format!("{error:?}"));

    info!(
        target: AUDIT_TARGET,
        event = event,
        phase = PHASE_EXECUTE,
        action = action_kind,
        grant_id = grant_id,
        grant_id_present = grant_id_present,
        resource_kind = resource_kind,
        resource = %resource_value,
        reason = reason.unwrap_or(""),
        reason_present = reason.is_some(),
        error = error.as_deref().unwrap_or(""),
        error_present = error.is_some(),
    );
}

fn serialize_resource(resource: &AuditResource) -> (&'static str, String) {
    match resource {
        AuditResource::Path(path) => (RESOURCE_KIND_PATH, path.display().to_string()),
        AuditResource::Value(value) => (RESOURCE_KIND_VALUE, value.clone()),
        AuditResource::None => (RESOURCE_KIND_NONE, String::new()),
    }
}
