use async_trait::async_trait;
use mvp_contract::{ToolOutcome, ToolRequest, ToolSpec};
use serde_json::Value;
use tracing::Instrument;

use crate::tool::ToolPlaneContext;
use crate::{audit, error::*};

/// Sealed runtime adapter used by the registry.
///
/// User code implements `ToolImpl`; the kernel wraps it into a `ToolAdapter`.
/// Registration metadata deliberately lives alongside the adapter in
/// `RegisteredTool`, not inside the adapter itself.
#[async_trait]
pub(crate) trait ToolAdapter: super::sealed::SealedToolAdapter + Send + Sync {
    async fn invoke(
        &self,
        ctx: &ToolPlaneContext<'_>,
        req: ToolRequest,
    ) -> Result<ToolOutcome, ToolError>;
}

/// Tool implementation supplied by builtins/plugins.
///
/// The implementation receives only the total `ToolPlaneContext`. It may use
/// capability sub-contexts such as `ctx.fs()` or `ctx.network()`, but it cannot
/// implement the sealed runtime `ToolAdapter` trait directly.
#[async_trait]
pub trait ToolImpl: Send + Sync + 'static {
    type Input: Send + 'static;
    type Output: Send + Into<ToolOutcome> + 'static;

    fn spec(&self) -> ToolSpec;

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError>;

    async fn execute(
        &self,
        ctx: &ToolPlaneContext<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError>;

    // TODO: use schema + serde
}

/// Runtime adapter around a user-provided `ToolImpl`.
///
/// Registration metadata deliberately lives in `RegisteredTool`, not here.
pub struct KernelToolAdapter<T: ToolImpl> {
    inner: T,
}

impl<T: ToolImpl> KernelToolAdapter<T> {
    pub(crate) fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T: ToolImpl> super::sealed::SealedToolAdapter for KernelToolAdapter<T> {}

#[async_trait]
impl<T: ToolImpl> ToolAdapter for KernelToolAdapter<T> {
    async fn invoke(
        &self,
        ctx: &ToolPlaneContext<'_>,
        req: ToolRequest,
    ) -> Result<ToolOutcome, ToolError> {
        let registration = ctx.registration;
        let tool_span = audit::tool_invocation_span(registration);

        async {
            let parse_span = audit::parse_input_span();
            let input = {
                let _parse_enter = parse_span.enter();
                self.inner
                    .parse_input(req.payload)
                    .map_err(ToolError::InvalidInput)?
            };

            let output = self
                .inner
                .execute(ctx, input)
                .instrument(audit::execution_span())
                .await?;

            let outcome = ctx
                .finalize_output(registration, output.into())
                .instrument(audit::final_output_span())
                .await
                .map_err(ToolError::FinalOutput)?;

            Ok(outcome)
        }
        .instrument(tool_span)
        .await
    }
}
