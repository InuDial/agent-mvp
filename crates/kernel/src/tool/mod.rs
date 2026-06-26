mod adapter;
mod context;
mod registration;

pub use adapter::ToolImpl;
pub use context::ToolContext;
pub use registration::{RegisteredTool, ToolRegistration};

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
