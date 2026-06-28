//! Network access facade.
//!
//! The network facade exposes URL fetches to tools while preserving the shared
//! action -> policy -> grant -> execute pipeline used by other access facades.

pub mod access;
pub mod action;
pub mod backend;
pub mod policy;

pub use access::{HasNetworkAccess, NetworkAccess};
pub use action::NetworkFetchAction;
pub use backend::{DenyNetworkBackend, HasNetworkBackend, NetworkBackend, StaticNetworkBackend};
pub use policy::{AllowDomainFetchPolicy, AllowExactUrlFetchPolicy};

#[cfg(test)]
mod tests;
