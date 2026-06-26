use async_trait::async_trait;
use std::path::PathBuf;

use crate::policy::{Policy, PolicyContextFactory, PolicyDecision, WorkspacePolicyContext};

use super::action::{FsReadAction, FsWriteAction};

/// Policy that only allows reading one exact file path.
pub struct AllowExactFileReadPolicy {
    allowed: PathBuf,
}

impl AllowExactFileReadPolicy {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            allowed: path.into(),
        }
    }
}

#[async_trait]
impl<F> Policy<F, FsReadAction> for AllowExactFileReadPolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "fs.allow_exact_file_read"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, action: &FsReadAction) -> PolicyDecision {
        if action.path.as_path() == self.allowed.as_path() {
            PolicyDecision::Allow { reason: None }
        } else {
            PolicyDecision::Abstain
        }
    }
}

/// Policy that allows file reads under a prefix.
pub struct AllowFileReadPrefixPolicy {
    prefix: PathBuf,
}

impl AllowFileReadPrefixPolicy {
    pub fn new(prefix: impl Into<PathBuf>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

#[async_trait]
impl<F> Policy<F, FsReadAction> for AllowFileReadPrefixPolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "fs.allow_file_read_prefix"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, action: &FsReadAction) -> PolicyDecision {
        if action.path.as_path().starts_with(&self.prefix) {
            PolicyDecision::Allow { reason: None }
        } else {
            PolicyDecision::Abstain
        }
    }
}

/// Policy that allows file reads under the current workspace root.
pub struct AllowWorkspaceReadPolicy;

#[async_trait]
impl<F> Policy<F, FsReadAction> for AllowWorkspaceReadPolicy
where
    F: PolicyContextFactory,
    for<'a> F::Context<'a>: WorkspacePolicyContext,
{
    fn name(&self) -> &'static str {
        "fs.allow_workspace_read"
    }

    async fn grant(&self, ctx: &F::Context<'_>, action: &FsReadAction) -> PolicyDecision {
        if action.path.as_path().starts_with(ctx.workspace_root()) {
            PolicyDecision::Allow { reason: None }
        } else {
            PolicyDecision::Abstain
        }
    }
}

/// Policy that only allows writing one exact file path.
pub struct AllowExactFileWritePolicy {
    allowed: PathBuf,
}

impl AllowExactFileWritePolicy {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            allowed: path.into(),
        }
    }
}

#[async_trait]
impl<F> Policy<F, FsWriteAction> for AllowExactFileWritePolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "fs.allow_exact_file_write"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, action: &FsWriteAction) -> PolicyDecision {
        if action.path.as_path() == self.allowed.as_path() {
            PolicyDecision::Allow { reason: None }
        } else {
            PolicyDecision::Abstain
        }
    }
}

/// Policy that allows file writes under a prefix.
pub struct AllowFileWritePrefixPolicy {
    prefix: PathBuf,
}

impl AllowFileWritePrefixPolicy {
    pub fn new(prefix: impl Into<PathBuf>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

#[async_trait]
impl<F> Policy<F, FsWriteAction> for AllowFileWritePrefixPolicy
where
    F: PolicyContextFactory,
{
    fn name(&self) -> &'static str {
        "fs.allow_file_write_prefix"
    }

    async fn grant(&self, _ctx: &F::Context<'_>, action: &FsWriteAction) -> PolicyDecision {
        if action.path.as_path().starts_with(&self.prefix) {
            PolicyDecision::Allow { reason: None }
        } else {
            PolicyDecision::Abstain
        }
    }
}

/// Policy that allows file writes under the current workspace root.
pub struct AllowWorkspaceWritePolicy;

#[async_trait]
impl<F> Policy<F, FsWriteAction> for AllowWorkspaceWritePolicy
where
    F: PolicyContextFactory,
    for<'a> F::Context<'a>: WorkspacePolicyContext,
{
    fn name(&self) -> &'static str {
        "fs.allow_workspace_write"
    }

    async fn grant(&self, ctx: &F::Context<'_>, action: &FsWriteAction) -> PolicyDecision {
        if action.path.as_path().starts_with(ctx.workspace_root()) {
            PolicyDecision::Allow { reason: None }
        } else {
            PolicyDecision::Abstain
        }
    }
}
