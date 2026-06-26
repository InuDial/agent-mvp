//! Kernel-owned service facades.
//!
//! Services expose side-effect domains, such as filesystem and network access,
//! to tools without letting tools perform those effects directly. A service
//! facade resolves user-facing requests into typed actions, asks the policy
//! plane for a grant, and executes only the granted action against the domain
//! backend.
//!
//! Backends perform direct domain operations. They should stay behind service
//! facades in tool-facing APIs so callers cannot bypass authorization.

pub mod fs;
pub mod network;
