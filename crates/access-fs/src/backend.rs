use async_trait::async_trait;

use mvp_core::error::{CapabilityError, ExecutionError};
use mvp_core::policy::Granted;

use super::action::{CanonicalPath, FsReadAction, FsWriteAction};

#[async_trait]
pub trait FsBackend: Send + Sync {
    async fn read_file(&self, granted: Granted<FsReadAction>) -> Result<String, ExecutionError>;
    async fn write_file(&self, granted: Granted<FsWriteAction>) -> Result<(), ExecutionError>;
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
    async fn read_file(&self, granted: Granted<FsReadAction>) -> Result<String, ExecutionError> {
        self.fs_backend().read_file(granted).await
    }

    async fn write_file(&self, granted: Granted<FsWriteAction>) -> Result<(), ExecutionError> {
        self.fs_backend().write_file(granted).await
    }
}

#[derive(Default)]
pub struct StdFsBackend;

impl StdFsBackend {
    pub fn new() -> Self {
        Self
    }
}

impl StdFsBackend {
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
impl FsBackend for StdFsBackend {
    async fn read_file(&self, granted: Granted<FsReadAction>) -> Result<String, ExecutionError> {
        let action = granted.into_action();
        self.read_canonical(&action.path)
            .await
            .map_err(ExecutionError::Capability)
    }

    async fn write_file(&self, granted: Granted<FsWriteAction>) -> Result<(), ExecutionError> {
        let action = granted.into_action();
        self.write_canonical(&action.path, &action.content)
            .await
            .map_err(ExecutionError::Capability)
    }
}
