use crate::error::AuthorizationError;
use crate::service::{fs::FsAccess, network::DenyNetwork};
use crate::tool::{ToolPlaneContext, ToolRegistration};
use mvp_contract::{Capabilities, Capability, ToolSpec};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEST_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

pub struct TempWorkspace {
    pub root: PathBuf,
}

impl TempWorkspace {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tool-plane-kernel-test-{}-{}-{}",
            std::process::id(),
            NEXT_TEST_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn registration(capabilities: Capabilities) -> ToolRegistration {
    ToolRegistration::new(ToolSpec {
        name: "test_tool".into(),
        description: "A tool for tests.".into(),
        capabilities,
    })
    .unwrap()
}

#[allow(dead_code)]
pub fn context<'a>(
    fs: &'a dyn FsAccess,
    root: &'a Path,
) -> Result<ToolPlaneContext<'a>, AuthorizationError> {
    let registration = Box::leak(Box::new(registration([Capability::FsRead].into())));
    let workspace_root = std::fs::canonicalize(root).map_err(AuthorizationError::Io)?;
    ToolPlaneContext::new(fs, &DenyNetwork, registration, workspace_root)
}
