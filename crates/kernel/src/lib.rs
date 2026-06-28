//! Reusable service runtime primitives for the tool-execution architecture.
//!
//! The kernel crate owns service assembly, policy pipeline, grant/execution
//! audit, and default backend wiring. Core authorization and generic tool traits
//! live in `mvp-core`; protocol data lives in `mvp-contract`.

pub mod audit;
pub mod policy;
pub mod runtime;
