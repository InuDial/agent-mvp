//! Reusable runtime primitives for the tool-execution architecture.
//!
//! The kernel crate owns the generic pieces: tools, contexts, actions, policy,
//! grants, service facades, and audit helpers. Concrete application state lives
//! in `mvp-app`.

pub mod action;
pub mod audit;
pub mod error;
pub mod kernel;
pub mod policy;
pub mod service;
pub mod tool;

#[cfg(any(test, feature = "test-support"))]
#[path = "test_utils.rs"]
pub mod test_support;

#[cfg(test)]
pub(crate) use test_support as test_utils;
