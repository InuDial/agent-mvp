//! Policy evaluation, grant records, and policy context types.
//!
//! Policies return `PolicyGrant` values that include a decision plus
//! policy-specified explanation fields. `PolicyPlane` owns evaluation order,
//! audit emission, grant creation, and default deny behavior.

pub mod context;
pub mod decision;
pub mod grant;
pub mod plane;
pub mod traits;

pub use context::*;
pub use decision::*;
pub use grant::*;
pub use plane::*;
pub use traits::*;
