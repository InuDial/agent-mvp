use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub type ToolName = String;
pub type ToolDescription = String;

#[derive(Clone, Debug, PartialEq)]
pub struct ToolOutcome {
    pub payload: Value,
    pub classification: OutputClassification,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputClassification {
    Public,
    WorkspaceLocal,
    Sensitive,
}

/// Per-tool metadata.
///
/// `capabilities` is the tool's declared default capability set.
///
/// It is used as the default effective capability envelope when a top-level
/// invocation does not provide an override. The invocation's actual effective
/// capabilities are carried by the kernel's runtime context and enforced later
/// through the policy engine during action / grant evaluation.
#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: ToolName,
    pub description: ToolDescription,
    /// Declared default capability set for this tool.
    ///
    /// This is not, by itself, the source of truth for every invocation's final
    /// effective capabilities. The kernel may supply a per-invocation effective
    /// capability envelope, and the policy engine is responsible for enforcing
    /// that envelope during authorization.
    pub capabilities: Capabilities,
    // etc
}

/// A single coarse capability slot.
///
/// The numeric value is the bit index in `Capabilities`
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    FsRead = 0,
    FsWrite = 1,
    FsList = 2,
    FsMetadata = 3,

    ProcessSpawn = 8,
    ProcessSignal = 9,
    ProcessIo = 10,

    SecretList = 16,
    SecretRead = 17,
    SecretMutate = 18,

    NetworkFetch = 24,
    NetworkBind = 25,

    ScheduleList = 32,
    ScheduleRead = 33,
    /// For schedule metadata mutation,
    /// probably coupled with `JobSpawn` for job-scheduling
    ScheduleMutate = 34,

    AgentSpawn = 40,

    JobSpawn = 48,

    TimeRead = 56,
}

impl Capability {
    pub const fn index(self) -> u8 {
        self as u8
    }

    pub const fn bit(self) -> u64 {
        1u64 << (self as u8)
    }
}

bitflags::bitflags! {
    /// A coarse-grained capability set used for broad authorization.
    ///
    /// Each set bit means an invocation may request operations in that exact
    /// coarse slot. Fine-grained authorization is still enforced later by
    /// policy and grants.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct Capabilities: u64 {
        const _ = !0;
    }
}

impl Capabilities {
    pub fn allows(self, capability: Capability) -> bool {
        self.contains(capability.into())
    }
    pub fn allows_all(self, capabilities: Capabilities) -> bool {
        self.contains(capabilities)
    }
}

impl From<Capability> for Capabilities {
    fn from(value: Capability) -> Self {
        Self::from_bits_retain(value.bit())
    }
}

impl<T> From<T> for Capabilities
where
    T: AsRef<[Capability]>,
{
    fn from(value: T) -> Self {
        value.as_ref().iter().map(ToOwned::to_owned).collect()
    }
}

impl std::iter::FromIterator<Capability> for Capabilities {
    fn from_iter<T: IntoIterator<Item = Capability>>(iter: T) -> Self {
        let mut caps = Self::empty();
        for capability in iter {
            caps |= capability.into();
        }
        caps
    }
}

/// Per-call ambient parameters supplied by the caller.
///
/// These parameters describe invocation environment such as workspace root.
/// Runtime authority is still tracked separately by the kernel on each
/// invocation frame.
#[derive(Clone, Debug)]
pub struct InvocationParams {
    pub workspace_root: PathBuf,
    pub capabilities_override: Option<Capabilities>,
}

impl InvocationParams {
    pub fn new(
        workspace_root: impl AsRef<Path>,
        capabilities_override: Option<Capabilities>,
    ) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
            capabilities_override,
        }
    }
}

#[derive(Clone, Debug)]
pub enum AuditResource {
    Path(PathBuf),
    Value(String),
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantId(u64);

impl GrantId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

pub type PolicyId = u64;

#[derive(Clone, Debug)]
pub enum PolicyDecision {
    Allow,
    Deny,
    Abstain,
}

#[derive(Clone, Debug)]
pub struct PolicyGrant {
    decision: PolicyDecision,
    reason: Option<String>,
    predicate: Option<String>,
}

impl PolicyGrant {
    pub fn allow(reason: Option<String>) -> Self {
        Self {
            decision: PolicyDecision::Allow,
            reason,
            predicate: None,
        }
    }

    pub fn deny(reason: Option<String>) -> Self {
        Self {
            decision: PolicyDecision::Deny,
            reason,
            predicate: None,
        }
    }

    pub fn abstain(reason: Option<String>) -> Self {
        Self {
            decision: PolicyDecision::Abstain,
            reason,
            predicate: None,
        }
    }

    pub fn decision(&self) -> &PolicyDecision {
        &self.decision
    }

    pub fn with_predicate(mut self, predicate: impl Into<String>) -> Self {
        self.predicate = Some(predicate.into());
        self
    }

    pub fn into_decision_and_reason(self) -> (PolicyDecision, Option<String>) {
        (self.decision, self.reason)
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn predicate(&self) -> Option<&str> {
        self.predicate.as_deref()
    }
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
pub struct PolicyEvaluation {
    policy_name: &'static str,
    policy_id: PolicyId,
    policy_stage: &'static str,
    grant: PolicyGrant,
}

impl PolicyEvaluation {
    pub fn new(
        policy_name: &'static str,
        policy_id: PolicyId,
        policy_stage: &'static str,
        grant: PolicyGrant,
    ) -> Self {
        Self {
            policy_name,
            policy_id,
            policy_stage,
            grant,
        }
    }

    pub fn policy_name(&self) -> &'static str {
        self.policy_name
    }

    pub fn policy_id(&self) -> PolicyId {
        self.policy_id
    }

    pub fn policy_stage(&self) -> &'static str {
        self.policy_stage
    }

    pub fn grant(&self) -> &PolicyGrant {
        &self.grant
    }
}

#[derive(Clone, Debug)]
pub enum PolicyOutcome {
    Allow {
        policy_name: &'static str,
        policy_id: PolicyId,
        reason: Option<String>,
    },
    Deny {
        source: GrantSource,
        reason: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct PolicyReport {
    evaluations: Vec<PolicyEvaluation>,
    outcome: PolicyOutcome,
}

impl PolicyReport {
    pub fn allow(
        evaluations: Vec<PolicyEvaluation>,
        policy_name: &'static str,
        policy_id: PolicyId,
        reason: Option<String>,
    ) -> Self {
        Self {
            evaluations,
            outcome: PolicyOutcome::Allow {
                policy_name,
                policy_id,
                reason,
            },
        }
    }

    pub fn deny_from_policy(
        evaluations: Vec<PolicyEvaluation>,
        policy_name: &'static str,
        policy_id: PolicyId,
        reason: Option<String>,
    ) -> Self {
        Self {
            evaluations,
            outcome: PolicyOutcome::Deny {
                source: GrantSource::Policy {
                    policy_name,
                    policy_id,
                },
                reason,
            },
        }
    }

    pub fn deny_without_match(evaluations: Vec<PolicyEvaluation>, reason: Option<String>) -> Self {
        Self {
            evaluations,
            outcome: PolicyOutcome::Deny {
                source: GrantSource::NoMatchingPolicy,
                reason,
            },
        }
    }

    pub fn evaluations(&self) -> &[PolicyEvaluation] {
        &self.evaluations
    }

    pub fn outcome(&self) -> &PolicyOutcome {
        &self.outcome
    }

    pub fn into_outcome(self) -> PolicyOutcome {
        self.outcome
    }
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
    pub fn allow(
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

    pub fn deny_from_policy(
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

    pub fn deny_without_match(
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

    pub fn grant_id(&self) -> Option<GrantId> {
        match self.decision {
            GrantDecision::Allow(grant_id) => Some(grant_id),
            GrantDecision::Deny => None,
        }
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
