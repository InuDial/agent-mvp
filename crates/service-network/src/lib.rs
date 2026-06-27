//! Network service facade.
//!
//! The network facade exposes URL fetches to tools while preserving the shared
//! action -> policy -> grant -> execute pipeline used by other services.

pub mod action;
pub mod backend;
pub mod policy;
pub mod service;

pub use action::NetworkFetchAction;
pub use backend::{DenyNetworkBackend, HasNetworkBackend, NetworkBackend, StaticNetworkBackend};
pub use policy::{AllowDomainFetchPolicy, AllowExactUrlFetchPolicy};
pub use service::{HasNetworkService, NetworkService};

#[cfg(test)]
mod tests;
