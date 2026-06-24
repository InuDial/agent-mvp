mod adapter;
mod context;
mod invocation;
mod plane;
mod registration;
mod registry;
#[cfg(test)]
pub(crate) mod test_utils;

pub(crate) use adapter::KernelToolAdapter;
pub use adapter::ToolImpl;
pub use context::ToolPlaneContext;
pub use invocation::InvocationParams;
pub use plane::ToolPlane;
pub use registration::{RegisteredTool, ToolRegistration};
pub use registry::ToolRegistry;

use std::sync::atomic::{AtomicU64, Ordering};

mod sealed {
    pub trait SealedToolAdapter {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrantId(u64);

impl GrantId {
    pub fn get(self) -> u64 {
        self.0
    }
}

static NEXT_GRANT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_grant_id() -> GrantId {
    GrantId(NEXT_GRANT_ID.fetch_add(1, Ordering::Relaxed))
}
