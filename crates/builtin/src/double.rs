use async_trait::async_trait;
use serde_json::Value;

use mvp_contract::{ToolOutcome, ToolRequest, ToolSpec};
use mvp_kernel::error::{InputError, ToolError};
use mvp_kernel::tool::{ToolImpl, ToolPlaneContext};

pub struct Double;

#[async_trait]
impl ToolImpl for Double {
    type Input = ToolRequest;
    type Output = ToolOutcome;

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "double".into(),
            description: "Invoke a tool twice, picking the latter outcome.".into(),
            capabilities: [].into(),
        }
    }

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError> {
        let name = payload
            .get("name")
            .and_then(Value::as_str)
            .ok_or(InputError::MissingField("name"))?
            .to_owned();
        let payload = payload
            .get("payload")
            .cloned()
            .ok_or(InputError::MissingField("payload"))?;

        Ok(ToolRequest { name, payload })
    }

    async fn execute(
        &self,
        ctx: &ToolPlaneContext<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError> {
        let _first = ctx.invoke_tool(input.clone()).await?;
        let second = ctx.invoke_tool(input).await?;
        Ok(second)
    }
}
