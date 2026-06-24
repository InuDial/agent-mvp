use std::path::{Path, PathBuf};

/// Per-call ambient parameters supplied by the caller.
///
/// These parameters describe invocation environment such as workspace root.
/// Runtime authority, such as effective capabilities, is tracked separately by
/// the kernel on each invocation frame.
#[derive(Debug)]
pub struct InvocationParams {
    pub(crate) workspace_root: PathBuf,
}

impl InvocationParams {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }
}
