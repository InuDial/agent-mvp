use std::marker::PhantomData;

use async_trait::async_trait;
use mvp_contract::{ToolOutcome, ToolSpec};
use serde_json::Value;
use tracing::Instrument;

use super::context::ToolContext;
use crate::kernel::Kernel;
use crate::{audit, error::*};

/// Sealed runtime adapter used by the registry.
///
/// User code implements `ToolImpl`; the kernel wraps it into a `ToolAdapter`.
/// Registration metadata deliberately lives alongside the adapter in
/// `RegisteredTool`, not inside the adapter itself.
#[async_trait]
pub(crate) trait ToolAdapter<K: Kernel>:
    super::sealed::SealedToolAdapter + Send + Sync
{
    async fn invoke(&self, ctx: &K::ToolCx<'_>, payload: Value) -> Result<ToolOutcome, ToolError>;
}

/// Tool implementation supplied by builtins/plugins.
///
/// The implementation receives only the total `ToolPlaneContext`. It may use
/// capability sub-contexts such as `ctx.fs()` or `ctx.network()`, but it cannot
/// implement the sealed runtime `ToolAdapter` trait directly.
#[async_trait]
pub trait ToolImpl<K: Kernel>: Send + Sync + 'static {
    type Input: Send + 'static;
    type Output: Send + Into<ToolOutcome> + 'static;

    fn spec(&self) -> ToolSpec;

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError>;

    async fn execute(
        &self,
        ctx: &K::ToolCx<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError>;

    // TODO: use schema + serde
}

/// Runtime adapter around a user-provided `ToolImpl`.
///
/// Registration metadata deliberately lives in `RegisteredTool`, not here.
pub struct KernelToolAdapter<K: Kernel, T: ToolImpl<K>> {
    inner: T,
    phantom_data: PhantomData<fn() -> K>,
}

impl<K: Kernel, T: ToolImpl<K>> KernelToolAdapter<K, T> {
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            phantom_data: PhantomData,
        }
    }
}

impl<K: Kernel, T: ToolImpl<K>> super::sealed::SealedToolAdapter for KernelToolAdapter<K, T> {}

#[async_trait]
impl<K: Kernel, T: ToolImpl<K>> ToolAdapter<K> for KernelToolAdapter<K, T> {
    async fn invoke(&self, ctx: &K::ToolCx<'_>, payload: Value) -> Result<ToolOutcome, ToolError> {
        let registration = ctx.registration();
        audit::record_tool_capabilities_override(
            registration,
            registration.spec().capabilities,
            ctx.effective_capabilities(),
        );
        let tool_span = audit::tool_invocation_span(registration);

        async {
            let parse_span = audit::parse_input_span();
            let input = {
                let _parse_enter = parse_span.enter();
                self.inner
                    .parse_input(payload)
                    .map_err(ToolError::InvalidInput)?
            };

            let output = self
                .inner
                .execute(ctx, input)
                .instrument(audit::execution_span())
                .await?;

            Ok(output.into())
        }
        .instrument(tool_span)
        .await
    }
}
