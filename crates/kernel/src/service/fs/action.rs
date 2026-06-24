use std::path::{Path, PathBuf};

use mvp_contract::{Capabilities, Capability};

use crate::action::{Action, AuditResource};
use crate::error::AuthorizationError;

#[derive(Clone, Debug)]
pub struct CanonicalPath {
    path: PathBuf,
}

impl CanonicalPath {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug)]
pub struct FsReadAction {
    pub(crate) path: CanonicalPath,
}

impl FsReadAction {
    pub(crate) fn new(path: CanonicalPath) -> Self {
        Self { path }
    }
}

impl Action for FsReadAction {
    fn capabilities(&self) -> Capabilities {
        Capability::FsRead.into()
    }

    fn audit_kind(&self) -> &'static str {
        "fs.read"
    }

    fn audit_resource(&self) -> AuditResource {
        AuditResource::Path(self.path.as_path().to_path_buf())
    }
}

pub(crate) fn resolve_under_authorization(
    root: &Path,
    path: &Path,
) -> Result<CanonicalPath, AuthorizationError> {
    let requested = candidate_path(root, path);
    let canonical = std::fs::canonicalize(requested).map_err(AuthorizationError::Io)?;

    if !canonical.starts_with(root) {
        return Err(AuthorizationError::OutsideWorkspace);
    }

    Ok(CanonicalPath::new(canonical))
}

fn candidate_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}
