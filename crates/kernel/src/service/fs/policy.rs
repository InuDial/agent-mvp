use async_trait::async_trait;
use std::path::PathBuf;

use crate::policy::{KernelPolicyContext, Policy, PolicyDecision};

use super::action::FsReadAction;

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
impl Policy<KernelPolicyContext, FsReadAction> for AllowExactFileReadPolicy {
    fn name(&self) -> &'static str {
        "fs.allow_exact_file_read"
    }

    async fn grant(&self, _ctx: &KernelPolicyContext, action: &FsReadAction) -> PolicyDecision {
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
impl Policy<KernelPolicyContext, FsReadAction> for AllowFileReadPrefixPolicy {
    fn name(&self) -> &'static str {
        "fs.allow_file_read_prefix"
    }

    async fn grant(&self, _ctx: &KernelPolicyContext, action: &FsReadAction) -> PolicyDecision {
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
impl Policy<KernelPolicyContext, FsReadAction> for AllowWorkspaceReadPolicy {
    fn name(&self) -> &'static str {
        "fs.allow_workspace_read"
    }

    async fn grant(&self, ctx: &KernelPolicyContext, action: &FsReadAction) -> PolicyDecision {
        if action.path.as_path().starts_with(ctx.workspace_root()) {
            PolicyDecision::Allow { reason: None }
        } else {
            PolicyDecision::Abstain
        }
    }
}
