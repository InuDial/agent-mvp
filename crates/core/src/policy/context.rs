use std::path::Path;

use mvp_contract::Capabilities;

use crate::policy::HasPolicyEngine;

pub trait PolicyContext: Send + Sync {
    fn capabilities(&self) -> &Capabilities;
}

pub trait PolicyContextFactory: 'static {
    type Context<'a>: PolicyContext;
}

pub type PolicyContextFor<'a, P> =
    <<P as HasPolicyEngine>::PolicyCxFactory as PolicyContextFactory>::Context<'a>;

pub trait WorkspacePolicyContext: PolicyContext {
    fn workspace_root(&self) -> &Path;
}
