//! Core authorization traits and unforgeable grants.
//!
//! This crate owns the safety model: actions, policy traits, policy contexts,
//! and `Granted<Action>` tokens. Runtime assembly, tracing, tools, and concrete
//! backends live in higher layers.

pub mod action;
pub mod error;
pub mod policy;
#[cfg(feature = "tool")]
pub mod tool;
