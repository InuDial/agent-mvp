use async_trait::async_trait;

use mvp_kernel::action::ActionExecutor;
use mvp_kernel::error::{CapabilityError, ExecutionError};
use mvp_kernel::policy::Granted;

use super::action::{CanonicalPath, FsReadAction, FsWriteAction};

#[async_trait]
pub trait FsBackend: Send + Sync {
    async fn read_canonical(&self, path: &CanonicalPath) -> Result<String, CapabilityError>;
    async fn write_canonical(
        &self,
        path: &CanonicalPath,
        content: &str,
    ) -> Result<(), CapabilityError>;
}

pub trait HasFsBackend: Send + Sync {
    type FsBackend: FsBackend + ?Sized;

    fn fs_backend(&self) -> &Self::FsBackend;
}

#[async_trait]
impl<T> FsBackend for T
where
    T: HasFsBackend,
{
    async fn read_canonical(&self, path: &CanonicalPath) -> Result<String, CapabilityError> {
        self.fs_backend().read_canonical(path).await
    }

    async fn write_canonical(
        &self,
        path: &CanonicalPath,
        content: &str,
    ) -> Result<(), CapabilityError> {
        self.fs_backend().write_canonical(path, content).await
    }
}

#[derive(Default)]
pub struct StdFsBackend;

impl StdFsBackend {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl FsBackend for StdFsBackend {
    async fn read_canonical(&self, path: &CanonicalPath) -> Result<String, CapabilityError> {
        tokio::fs::read_to_string(path.as_path())
            .await
            .map_err(CapabilityError::Io)
    }

    async fn write_canonical(
        &self,
        path: &CanonicalPath,
        content: &str,
    ) -> Result<(), CapabilityError> {
        tokio::fs::write(path.as_path(), content)
            .await
            .map_err(CapabilityError::Io)
    }
}

#[async_trait]
impl ActionExecutor<FsReadAction> for dyn FsBackend + '_ {
    type Output = String;

    async fn execute(
        &self,
        granted: Granted<FsReadAction>,
    ) -> Result<Self::Output, ExecutionError> {
        self.read_canonical(&granted.action().path)
            .await
            .map_err(ExecutionError::Capability)
    }
}

#[async_trait]
impl ActionExecutor<FsWriteAction> for dyn FsBackend + '_ {
    type Output = ();

    async fn execute(
        &self,
        granted: Granted<FsWriteAction>,
    ) -> Result<Self::Output, ExecutionError> {
        self.write_canonical(&granted.action().path, &granted.action().content)
            .await
            .map_err(ExecutionError::Capability)
    }
}
