use mvp_contract::{Capability, GrantRecord};

#[derive(Debug)]
pub struct AuthorizationDeny {
    reason: String,
    record: Option<GrantRecord>,
}

impl AuthorizationDeny {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            record: None,
        }
    }

    pub fn from_record(record: GrantRecord) -> Self {
        let reason = record.reason().unwrap_or("policy denied action").to_owned();
        Self {
            reason,
            record: Some(record),
        }
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn record(&self) -> Option<&GrantRecord> {
        self.record.as_ref()
    }
}

impl From<String> for AuthorizationDeny {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for AuthorizationDeny {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<GrantRecord> for AuthorizationDeny {
    fn from(value: GrantRecord) -> Self {
        Self::from_record(value)
    }
}

#[derive(Debug)]
pub enum AuthorizationError {
    MissingCapability(Capability),
    Denied(AuthorizationDeny),
    OutsideWorkspace,
    Io(std::io::Error),
}

impl AuthorizationError {
    pub fn denied_reason(&self) -> Option<&str> {
        match self {
            Self::Denied(deny) => Some(deny.reason()),
            _ => None,
        }
    }

    pub fn deny_record(&self) -> Option<&GrantRecord> {
        match self {
            Self::Denied(deny) => deny.record(),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum ExecutionError {
    Authorization(AuthorizationError),
    Capability(CapabilityError),
    Other(String),
}

#[derive(Debug)]
pub enum CapabilityError {
    GrantMismatch,
    Denied,
    Io(std::io::Error),
}

#[derive(Debug)]
pub enum ToolError {
    UnknownTool(String),
    DuplicateTool(String),
    InvalidSpec,
    InvalidInput(InputError),
    Authorization(AuthorizationError),
    Execution(ExecutionError),
}

#[derive(Debug)]
pub enum InputError {
    MissingField(&'static str),
    InvalidField(&'static str),
}
