//! Reusable runtime primitives for the tool-execution architecture.
//!
//! The kernel crate owns runtime assembly: tools, contexts, policy pipeline,
//! grants audit, and invocation helpers. Core authorization traits live in
//! `mvp-core`; protocol data lives in `mvp-contract`.

pub mod audit;
pub mod pipeline;
pub mod policy_context;
pub mod runtime;
