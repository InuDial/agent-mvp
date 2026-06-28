use std::marker::PhantomData;

use async_trait::async_trait;
use mvp_contract::{ToolOutcome, ToolSpec};
use serde_json::Value;

use crate::error::{InputError, ToolError};

use super::ToolHost;

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
        let input = self
            .inner
            .parse_input(payload)
            .map_err(ToolError::InvalidInput)?;

        let output = self.inner.execute(ctx, input).await?;
        Ok(output.into())
    }
}
