use std::marker::PhantomData;

use crate::error::{InputError, ToolError};
use async_trait::async_trait;
use mvp_contract::{ToolOutcome, ToolSpec};
use serde_json::Value;
use tracing::{Instrument, Span};

use super::{ToolContext, ToolHost};

#[async_trait]
pub(crate) trait ToolAdapter<H: ToolHost>:
    super::sealed::SealedToolAdapter + Send + Sync
{
    async fn invoke(&self, ctx: &H::ToolCx<'_>, payload: Value) -> Result<ToolOutcome, ToolError>;
}

#[async_trait]
pub trait ToolImpl<H: ToolHost>: Send + Sync + 'static {
    type Input: Send + 'static;
    type Output: Send + Into<ToolOutcome> + 'static;

    fn spec(&self) -> ToolSpec;

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError>;

    async fn execute(
        &self,
        ctx: &H::ToolCx<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError>;
}

pub(crate) struct CoreToolAdapter<H: ToolHost, T: ToolImpl<H>> {
    inner: T,
    phantom_data: PhantomData<fn() -> H>,
}

impl<H: ToolHost, T: ToolImpl<H>> CoreToolAdapter<H, T> {
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            phantom_data: PhantomData,
        }
    }
}

impl<H: ToolHost, T: ToolImpl<H>> super::sealed::SealedToolAdapter for CoreToolAdapter<H, T> {}

#[async_trait]
impl<H, T> ToolAdapter<H> for CoreToolAdapter<H, T>
where
    H: ToolHost,
    T: ToolImpl<H>,
{
    async fn invoke(&self, ctx: &H::ToolCx<'_>, payload: Value) -> Result<ToolOutcome, ToolError> {
        let tool_name = ctx.registration().spec().name.as_str();
        let input = {
            let span = H::parse_input_span(tool_name);
            let _enter = span.enter();
            match self.inner.parse_input(payload) {
                Ok(input) => {
                    record_span_result(Ok(()));
                    input
                }
                Err(error) => {
                    record_span_result(Err(format!("{error:?}")));
                    return Err(ToolError::InvalidInput(error));
                }
            }
        };

        let output = async {
            match self.inner.execute(ctx, input).await {
                Ok(output) => {
                    record_span_result(Ok(()));
                    Ok(output)
                }
                Err(error) => {
                    record_span_result(Err(format!("{error:?}")));
                    Err(error)
                }
            }
        }
        .instrument(H::execution_span(tool_name))
        .await?;

        Ok(output.into())
    }
}

fn record_span_result(result: Result<(), String>) {
    let span = Span::current();
    match result {
        Ok(()) => {
            span.record("result", "ok");
            span.record("otel.status_code", "ok");
        }
        Err(error) => {
            span.record("result", "error");
            span.record("otel.status_code", "error");
            span.record("otel.status_description", error.as_str());
            span.record("error", error.as_str());
        }
    }
}
