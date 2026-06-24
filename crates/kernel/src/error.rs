use mvp_contract::{Capability, ToolName};

#[derive(Debug)]
pub enum ToolError {
    UnknownTool(ToolName),
    DuplicateTool(ToolName),
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

#[derive(Debug)]
pub enum AuthorizationError {
    MissingCapability(Capability),
    Denied(String),
    OutsideWorkspace,
    Io(std::io::Error),
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
