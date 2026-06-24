use std::fmt::Debug;
use std::path::Path;

use tracing::{info, info_span, Span};

use crate::tool::{GrantId, ToolRegistration};

pub const AUDIT_TARGET: &str = "tool_plane::audit";

pub fn tool_invocation_span(registration: &ToolRegistration) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.invoke",
        tool = %registration.spec().name,
    )
}

pub fn parse_input_span(registration: &ToolRegistration) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.parse_input",
        tool = %registration.spec().name,
    )
}

pub fn input_auth_span(registration: &ToolRegistration) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.input_auth",
        tool = %registration.spec().name,
    )
}

pub fn execution_span(registration: &ToolRegistration) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.execution",
        tool = %registration.spec().name,
    )
}

pub fn output_auth_span(registration: &ToolRegistration) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.output_auth",
        tool = %registration.spec().name,
    )
}

pub fn final_output_span(registration: &ToolRegistration) -> Span {
    info_span!(
        target: AUDIT_TARGET,
        "tool.final_output",
        tool = %registration.spec().name,
    )
}

/// Record that a kernel capability grant was issued.
///
/// This is intentionally a thin wrapper over `tracing`: tracing owns span
/// mechanics; this helper only keeps ToolPlane's audit field schema stable.
pub(crate) fn grant_issued(tool: &str, grant_kind: &str, grant_id: GrantId, scope: &impl Debug) {
    info!(
        target: AUDIT_TARGET,
        event = "grant_issued",
        tool = %tool,
        grant_kind = %grant_kind,
        grant_id = grant_id.get(),
        scope = ?scope,
    );
}

/// Record that a kernel capability grant was consumed for an operation.
pub(crate) fn grant_used(
    tool: &str,
    grant_kind: &str,
    grant_id: GrantId,
    scope: &impl Debug,
    resource: &Path,
) {
    info!(
        target: AUDIT_TARGET,
        event = "grant_used",
        tool = %tool,
        grant_kind = %grant_kind,
        grant_id = grant_id.get(),
        scope = ?scope,
        resource = %resource.display(),
    );
}

/// Same as `grant_used`, but for non-path resources such as URLs.
pub(crate) fn grant_used_value(
    tool: &str,
    grant_kind: &str,
    grant_id: GrantId,
    scope: &impl Debug,
    resource: &str,
) {
    info!(
        target: AUDIT_TARGET,
        event = "grant_used",
        tool = %tool,
        grant_kind = %grant_kind,
        grant_id = grant_id.get(),
        scope = ?scope,
        resource = %resource,
    );
}
