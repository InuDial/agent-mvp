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

#[derive(Clone, Debug)]
pub struct FsWriteAction {
    pub(crate) path: CanonicalPath,
    pub(crate) content: String,
}

impl FsWriteAction {
    pub(crate) fn new(path: CanonicalPath, content: String) -> Self {
        Self { path, content }
    }
}

impl Action for FsWriteAction {
    fn capabilities(&self) -> Capabilities {
        Capability::FsWrite.into()
    }

    fn audit_kind(&self) -> &'static str {
        "fs.write"
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

    ensure_under_root(root, &canonical)?;

    Ok(CanonicalPath::new(canonical))
}

pub(crate) fn resolve_write_under_authorization(
    root: &Path,
    path: &Path,
) -> Result<CanonicalPath, AuthorizationError> {
    let requested = candidate_path(root, path);

    if requested.exists() {
        let canonical = std::fs::canonicalize(requested).map_err(AuthorizationError::Io)?;
        ensure_under_root(root, &canonical)?;
        return Ok(CanonicalPath::new(canonical));
    }

    let file_name = requested.file_name().ok_or_else(|| {
        AuthorizationError::Denied("write target must include a file name".into())
    })?;
    let parent = requested.parent().ok_or_else(|| {
        AuthorizationError::Denied("write target must have an authorized parent directory".into())
    })?;
    let canonical_parent = std::fs::canonicalize(parent).map_err(AuthorizationError::Io)?;

    ensure_under_root(root, &canonical_parent)?;

    Ok(CanonicalPath::new(canonical_parent.join(file_name)))
}

fn ensure_under_root(root: &Path, candidate: &Path) -> Result<(), AuthorizationError> {
    if candidate.starts_with(root) {
        Ok(())
    } else {
        Err(AuthorizationError::OutsideWorkspace)
    }
}

fn candidate_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}
