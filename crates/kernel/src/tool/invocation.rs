use std::path::{Path, PathBuf};

/// Per-call parameters supplied by the caller.
///
/// This is intentionally cheap and one-shot: `ToolPlane` builds the internal
/// `ToolPlaneContext` for each invocation instead of exposing a reusable scope.
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
