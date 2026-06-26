//! Filesystem resource policies.
//!
//! These policies compare canonical action paths with canonical policy operands:
//! exact paths use `CanonicalPath`, prefix policies use `CanonicalPrefix`, and
//! workspace policies use the `CanonicalRoot` carried by the policy context.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::policy::{Policy, PolicyContextFactory, PolicyGrant, WorkspacePolicyContext};

use super::action::{CanonicalPath, CanonicalPrefix, FsAction, FsReadAction, FsWriteAction};

/// Policy that allows the shared filesystem parent action inside the workspace.
pub struct AllowWorkspaceFsPolicy;

#[async_trait]
impl<F> Policy<F, FsAction> for AllowWorkspaceFsPolicy
where
    F: PolicyContextFactory,
    for<'a> F::Context<'a>: WorkspacePolicyContext,
{
    fn name(&self) -> &'static str {
        "fs.allow_path"
    }

    async fn grant(&self, ctx: &F::Context<'_>, action: &FsAction) -> PolicyGrant {
        let predicate = format!(
            "path starts_with workspace_root: {} starts_with {}",
            action.path.as_path().display(),
            ctx.workspace_root().as_path().display()
        );

        if ctx.workspace_root().contains(&action.path) {
            PolicyGrant::allow(Some("filesystem path is under workspace root".into()))
                .with_predicate(predicate)
        } else {
            PolicyGrant::deny(Some("filesystem path is outside workspace root".into()))
                .with_predicate(predicate)
        }
    }
}

/// Policy that only allows reading one exact file path.
pub struct AllowExactFileReadPolicy {
    allowed: CanonicalPath,
}

impl AllowExactFileReadPolicy {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            allowed: CanonicalPath::existing(path.into()).expect("read policy path must exist"),
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

    async fn grant(&self, _ctx: &F::Context<'_>, action: &FsReadAction) -> PolicyGrant {
        let predicate = format!(
            "path == allowed: {} == {}",
            action.path.as_path().display(),
            self.allowed.as_path().display()
        );

        if action.path.as_path() == self.allowed.as_path() {
            PolicyGrant::allow(Some("read path matches exact allowed file".into()))
                .with_predicate(predicate)
        } else {
            PolicyGrant::abstain(Some("read path does not match exact allowed file".into()))
                .with_predicate(predicate)
        }
    }
}

/// Policy that allows file reads under a prefix.
pub struct AllowFileReadPrefixPolicy {
    prefix: CanonicalPrefix,
}

impl AllowFileReadPrefixPolicy {
    pub fn new(prefix: impl Into<PathBuf>) -> Self {
        Self {
            prefix: CanonicalPrefix::existing(prefix.into()).expect("read prefix must exist"),
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

    async fn grant(&self, _ctx: &F::Context<'_>, action: &FsReadAction) -> PolicyGrant {
        let predicate = format!(
            "path starts_with prefix: {} starts_with {}",
            action.path.as_path().display(),
            self.prefix.as_path().display()
        );

        if self.prefix.contains(&action.path) {
            PolicyGrant::allow(Some("read path is under allowed prefix".into()))
                .with_predicate(predicate)
        } else {
            PolicyGrant::abstain(Some("read path is outside allowed prefix".into()))
                .with_predicate(predicate)
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

    async fn grant(&self, ctx: &F::Context<'_>, action: &FsReadAction) -> PolicyGrant {
        let predicate = format!(
            "path starts_with workspace_root: {} starts_with {}",
            action.path.as_path().display(),
            ctx.workspace_root().as_path().display()
        );

        if ctx.workspace_root().contains(&action.path) {
            PolicyGrant::allow(Some("read path is under workspace root".into()))
                .with_predicate(predicate)
        } else {
            PolicyGrant::abstain(Some("read path is outside workspace root".into()))
                .with_predicate(predicate)
        }
    }
}

/// Policy that only allows writing one exact file path.
pub struct AllowExactFileWritePolicy {
    allowed: CanonicalPath,
}

impl AllowExactFileWritePolicy {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            allowed: CanonicalPath::existing_or_parent(path.into())
                .expect("write policy path parent must exist"),
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

    async fn grant(&self, _ctx: &F::Context<'_>, action: &FsWriteAction) -> PolicyGrant {
        let predicate = format!(
            "path == allowed: {} == {}",
            action.path.as_path().display(),
            self.allowed.as_path().display()
        );

        if action.path.as_path() == self.allowed.as_path() {
            PolicyGrant::allow(Some("write path matches exact allowed file".into()))
                .with_predicate(predicate)
        } else {
            PolicyGrant::abstain(Some("write path does not match exact allowed file".into()))
                .with_predicate(predicate)
        }
    }
}

/// Policy that allows file writes under a prefix.
pub struct AllowFileWritePrefixPolicy {
    prefix: CanonicalPrefix,
}

impl AllowFileWritePrefixPolicy {
    pub fn new(prefix: impl Into<PathBuf>) -> Self {
        Self {
            prefix: CanonicalPrefix::existing(prefix.into()).expect("write prefix must exist"),
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

    async fn grant(&self, _ctx: &F::Context<'_>, action: &FsWriteAction) -> PolicyGrant {
        let predicate = format!(
            "path starts_with prefix: {} starts_with {}",
            action.path.as_path().display(),
            self.prefix.as_path().display()
        );

        if self.prefix.contains(&action.path) {
            PolicyGrant::allow(Some("write path is under allowed prefix".into()))
                .with_predicate(predicate)
        } else {
            PolicyGrant::abstain(Some("write path is outside allowed prefix".into()))
                .with_predicate(predicate)
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

    async fn grant(&self, ctx: &F::Context<'_>, action: &FsWriteAction) -> PolicyGrant {
        let predicate = format!(
            "path starts_with workspace_root: {} starts_with {}",
            action.path.as_path().display(),
            ctx.workspace_root().as_path().display()
        );

        if ctx.workspace_root().contains(&action.path) {
            PolicyGrant::allow(Some("write path is under workspace root".into()))
                .with_predicate(predicate)
        } else {
            PolicyGrant::abstain(Some("write path is outside workspace root".into()))
                .with_predicate(predicate)
        }
    }
}
