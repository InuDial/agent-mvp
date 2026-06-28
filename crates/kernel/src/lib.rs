//! Reusable runtime primitives for the tool-execution architecture.
//!
//! The kernel crate owns the generic pieces: tools, contexts, actions, policy,
//! grants, and audit helpers. Concrete access facades and application state
//! live outside `mvp-kernel`.

pub mod action;
pub mod audit;
pub mod error;
pub mod kernel;
pub mod policy;
pub mod tool;
