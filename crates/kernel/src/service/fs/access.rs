use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;

use crate::action::ExecutableAction;
use crate::error::{CapabilityError, ExecutionError};
use crate::policy::Granted;

use super::action::{CanonicalPath, FsReadAction};

mod sealed {
    pub trait SealedFs {}
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

impl ExecutableAction for FsReadAction {
    type Executor<'a> = dyn FsAccess + 'a;
    type Output = String;

    fn execute<'a>(
        fs: &'a Self::Executor<'a>,
        granted: Granted<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Output, ExecutionError>> + Send + 'a>>
    where
        Self: 'a,
    {
        Box::pin(async move {
            fs.read_canonical(&granted.action.path)
                .await
                .map_err(ExecutionError::Capability)
        })
    }
}
