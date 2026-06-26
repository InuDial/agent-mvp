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
/// through the policy plane during action / grant evaluation.
#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: ToolName,
    pub description: ToolDescription,
    /// Declared default capability set for this tool.
    ///
    /// This is not, by itself, the source of truth for every invocation's final
    /// effective capabilities. The kernel may supply a per-invocation effective
    /// capability envelope, and the policy plane is responsible for enforcing
    /// that envelope during authorization.
    pub capabilities: Capabilities,
    // etc
}

/// A single coarse capability slot.
///
/// The numeric value is the bit index in `Capabilities`
///
/// ref: See crates/kernel/service/mod.rs
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
