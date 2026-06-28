//! Policy traits, grant records, and policy context types.
//!
//! Policies return `PolicyGrant` values that include a decision plus
//! policy-specified explanation fields. Concrete runtimes provide a
//! `PolicyEngine` implementation; the kernel-owned `grant` path emits audit
//! records and creates `Granted<Action>` values.

pub mod context;
pub mod decision;
pub mod grant;
pub mod traits;

pub use context::*;
pub use decision::*;
pub use grant::*;
pub use traits::*;
