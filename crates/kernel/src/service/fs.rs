use std::path::{Path, PathBuf};

use async_trait::async_trait;
use mvp_contract::Capability;

use crate::tool::{next_grant_id, GrantId, ToolPlaneContext};
use crate::{audit, error::*};

mod sealed {
    pub trait SealedFs {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadGrantScope {
    File { path: PathBuf },
    Directory { directory: PathBuf },
    Workspace { root: PathBuf },
}

#[derive(Clone, Debug)]
pub struct CanonicalPath {
    path: PathBuf,
}

impl CanonicalPath {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

/// Grant for one precise file.
///
/// The authorized file path is stored inside the grant, so execution does not
/// accept a second path parameter that could disagree with the grant.
pub struct FileReadGrant {
    id: GrantId,
    path: CanonicalPath,
}

impl FileReadGrant {
    fn issue(path: CanonicalPath) -> Self {
        Self {
            id: next_grant_id(),
            path,
        }
    }

    pub fn id(&self) -> GrantId {
        self.id
    }

    fn scope(&self) -> ReadGrantScope {
        ReadGrantScope::File {
            path: self.path.as_path().to_path_buf(),
        }
    }
}

/// Grant for all reads under one precise directory.
pub struct DirectoryReadGrant {
    id: GrantId,
    directory: CanonicalPath,
}

impl DirectoryReadGrant {
    fn issue(directory: CanonicalPath) -> Self {
        Self {
            id: next_grant_id(),
            directory,
        }
    }

    pub fn id(&self) -> GrantId {
        self.id
    }

    fn scope(&self) -> ReadGrantScope {
        ReadGrantScope::Directory {
            directory: self.directory.as_path().to_path_buf(),
        }
    }
}

/// Grant for all reads under the invocation workspace root.
pub struct WorkspaceReadGrant {
    id: GrantId,
    root: CanonicalPath,
}

impl WorkspaceReadGrant {
    fn issue(root: CanonicalPath) -> Self {
        Self {
            id: next_grant_id(),
            root,
        }
    }

    pub fn id(&self) -> GrantId {
        self.id
    }

    fn scope(&self) -> ReadGrantScope {
        ReadGrantScope::Workspace {
            root: self.root.as_path().to_path_buf(),
        }
    }
}

#[async_trait]
pub trait FsAccess: sealed::SealedFs + Send + Sync {
    async fn read_canonical(&self, path: &CanonicalPath) -> Result<String, CapabilityError>;
}

#[derive(Default)]
pub struct StdFs;

impl StdFs {
    pub fn new() -> Self {
        Self
    }
}

impl sealed::SealedFs for StdFs {}

#[async_trait]
impl FsAccess for StdFs {
    async fn read_canonical(&self, path: &CanonicalPath) -> Result<String, CapabilityError> {
        std::fs::read_to_string(path.as_path()).map_err(CapabilityError::Io)
    }
}

/// Filesystem sub-context exposed as `ctx.fs()`.
///
/// This single sub-context handles both issuing grants and consuming them. The
/// top-level `ToolPlaneContext` is not split into auth/execute phases; legality
/// is instead enforced by opaque grant types and kernel-owned capability access.
pub struct FsContext<'a> {
    parent: &'a ToolPlaneContext<'a>,
}

impl<'a> FsContext<'a> {
    /// Authorize reading one exact file and issue a file-scoped grant.
    pub async fn grant_file_read(&self, path: &str) -> Result<FileReadGrant, AuthorizationError> {
        self.ensure_fs_read_capability()?;

        let path = resolve_under_authorization(&self.parent.workspace_root, Path::new(path))?;
        let grant = FileReadGrant::issue(path);
        self.audit_issued("fs.read.file", grant.id(), grant.scope());

        Ok(grant)
    }

    /// Authorize reads under a directory and issue a directory-scoped grant.
    pub async fn grant_directory_read(
        &self,
        directory: &str,
    ) -> Result<DirectoryReadGrant, AuthorizationError> {
        self.ensure_fs_read_capability()?;

        let directory =
            resolve_under_authorization(&self.parent.workspace_root, Path::new(directory))?;
        let grant = DirectoryReadGrant::issue(directory);
        self.audit_issued("fs.read.directory", grant.id(), grant.scope());

        Ok(grant)
    }

    /// Authorize reads under the whole workspace root.
    pub async fn grant_workspace_read(&self) -> Result<WorkspaceReadGrant, AuthorizationError> {
        self.ensure_fs_read_capability()?;

        let root = CanonicalPath::new(self.parent.workspace_root.clone());
        let grant = WorkspaceReadGrant::issue(root);
        self.audit_issued("fs.read.workspace", grant.id(), grant.scope());

        Ok(grant)
    }

    /// Downgrade a directory grant to a precise file grant under the same directory.
    pub async fn downgrade_directory_to_file(
        &self,
        grant: &DirectoryReadGrant,
        path: impl AsRef<Path>,
    ) -> Result<FileReadGrant, CapabilityError> {
        let path = resolve_under_capability(grant.directory.as_path(), path.as_ref())?;
        Ok(FileReadGrant::issue(path))
    }

    /// Downgrade a workspace grant to a directory grant under the workspace root.
    pub async fn downgrade_workspace_to_directory(
        &self,
        grant: &WorkspaceReadGrant,
        directory: impl AsRef<Path>,
    ) -> Result<DirectoryReadGrant, CapabilityError> {
        let directory = resolve_under_capability(grant.root.as_path(), directory.as_ref())?;
        Ok(DirectoryReadGrant::issue(directory))
    }

    /// Downgrade a workspace grant to a precise file grant under the workspace root.
    pub async fn downgrade_workspace_to_file(
        &self,
        grant: &WorkspaceReadGrant,
        path: impl AsRef<Path>,
    ) -> Result<FileReadGrant, CapabilityError> {
        let path = resolve_under_capability(grant.root.as_path(), path.as_ref())?;
        Ok(FileReadGrant::issue(path))
    }

    /// Consume a precise file grant.
    pub async fn read_file(&self, grant: &FileReadGrant) -> Result<String, CapabilityError> {
        self.audit_used("fs.read.file", grant.id(), grant.scope(), &grant.path);
        self.parent.fs.read_canonical(&grant.path).await
    }

    /// Consume a directory grant for a path under that directory.
    pub async fn read_in_directory(
        &self,
        grant: &DirectoryReadGrant,
        path: impl AsRef<Path>,
    ) -> Result<String, CapabilityError> {
        let path = resolve_under_capability(grant.directory.as_path(), path.as_ref())?;
        self.audit_used("fs.read.directory", grant.id(), grant.scope(), &path);
        self.parent.fs.read_canonical(&path).await
    }

    /// Consume a workspace-wide read grant.
    pub async fn read_in_workspace(
        &self,
        grant: &WorkspaceReadGrant,
        path: impl AsRef<Path>,
    ) -> Result<String, CapabilityError> {
        let path = resolve_under_capability(grant.root.as_path(), path.as_ref())?;
        self.audit_used("fs.read.workspace", grant.id(), grant.scope(), &path);
        self.parent.fs.read_canonical(&path).await
    }

    fn ensure_fs_read_capability(&self) -> Result<(), AuthorizationError> {
        if !self
            .parent
            .registration
            .spec
            .capabilities
            .allows(Capability::FsRead)
        {
            return Err(AuthorizationError::MissingCapability(Capability::FsRead));
        }

        Ok(())
    }

    fn audit_issued(&self, grant_kind: &str, grant_id: GrantId, scope: ReadGrantScope) {
        audit::grant_issued(
            &self.parent.registration.spec.name,
            grant_kind,
            grant_id,
            &scope,
        );
    }

    fn audit_used(
        &self,
        grant_kind: &str,
        grant_id: GrantId,
        scope: ReadGrantScope,
        path: &CanonicalPath,
    ) {
        audit::grant_used(
            &self.parent.registration.spec.name,
            grant_kind,
            grant_id,
            &scope,
            path.as_path(),
        );
    }
}

impl<'a> ToolPlaneContext<'a> {
    pub fn fs(&'a self) -> FsContext<'a> {
        FsContext { parent: self }
    }
}

fn resolve_under_authorization(
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

fn resolve_under_capability(root: &Path, path: &Path) -> Result<CanonicalPath, CapabilityError> {
    let requested = candidate_path(root, path);
    let canonical = std::fs::canonicalize(requested).map_err(CapabilityError::Io)?;

    if !canonical.starts_with(root) {
        return Err(CapabilityError::GrantMismatch);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::network::DenyNetwork;
    use crate::tool::test_utils::*;
    use mvp_contract::Capability;

    #[tokio::test]
    async fn directory_read_grant_reads_only_under_directory() {
        let ws = TempWorkspace::new();
        std::fs::create_dir_all(ws.root.join("allowed")).unwrap();
        std::fs::write(ws.root.join("allowed/a.txt"), "a").unwrap();
        std::fs::write(ws.root.join("outside.txt"), "outside").unwrap();

        let fs = StdFs::new();
        let reg = Box::leak(Box::new(registration([Capability::FsRead].into())));
        let workspace_root = std::fs::canonicalize(&ws.root).unwrap();
        let ctx = ToolPlaneContext::new(&fs, &DenyNetwork, reg, workspace_root).unwrap();

        let grant = ctx.fs().grant_directory_read("allowed").await.unwrap();
        let content = ctx.fs().read_in_directory(&grant, "a.txt").await.unwrap();
        assert_eq!(content, "a");

        let denied = ctx.fs().read_in_directory(&grant, "../outside.txt").await;
        assert!(matches!(denied, Err(CapabilityError::GrantMismatch)));
    }

    #[tokio::test]
    async fn workspace_read_grant_reads_under_workspace() {
        let ws = TempWorkspace::new();
        std::fs::create_dir_all(ws.root.join("a/b")).unwrap();
        std::fs::write(ws.root.join("a/b/file.txt"), "all").unwrap();

        let fs = StdFs::new();
        let reg = Box::leak(Box::new(registration([Capability::FsRead].into())));
        let workspace_root = std::fs::canonicalize(&ws.root).unwrap();
        let ctx = ToolPlaneContext::new(&fs, &DenyNetwork, reg, workspace_root).unwrap();

        let grant = ctx.fs().grant_workspace_read().await.unwrap();
        let content = ctx
            .fs()
            .read_in_workspace(&grant, "a/b/file.txt")
            .await
            .unwrap();
        assert_eq!(content, "all");
    }

    #[tokio::test]
    async fn workspace_grant_can_downgrade_to_file_grant() {
        let ws = TempWorkspace::new();
        std::fs::write(ws.root.join("file.txt"), "downgraded").unwrap();

        let fs = StdFs::new();
        let reg = Box::leak(Box::new(registration([Capability::FsRead].into())));
        let workspace_root = std::fs::canonicalize(&ws.root).unwrap();
        let ctx = ToolPlaneContext::new(&fs, &DenyNetwork, reg, workspace_root).unwrap();

        let workspace_grant = ctx.fs().grant_workspace_read().await.unwrap();
        let file_grant = ctx
            .fs()
            .downgrade_workspace_to_file(&workspace_grant, "file.txt")
            .await
            .unwrap();
        let content = ctx.fs().read_file(&file_grant).await.unwrap();

        assert_eq!(content, "downgraded");
    }
}
