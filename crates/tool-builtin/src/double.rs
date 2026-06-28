use async_trait::async_trait;
use mvp_contract::{ToolOutcome, ToolSpec};
use mvp_kernel::error::{InputError, ToolError};
use mvp_kernel::kernel::Kernel;
use mvp_kernel::tool::{ToolContext, ToolImpl};
use serde_json::Value;

pub struct Double;

#[async_trait]
impl<K> ToolImpl<K> for Double
where
    K: Kernel,
{
    type Input = (K::ToolPath, Value);
    type Output = ToolOutcome;

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "double".into(),
            description: "Invoke a tool twice, picking the latter outcome.".into(),
            capabilities: [].into(),
        }
    }

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError> {
        let path_value = payload
            .get("path")
            .ok_or(InputError::MissingField("path"))?;
        let path = K::decode_tool_path(path_value)?;
        let payload = payload
            .get("payload")
            .cloned()
            .ok_or(InputError::MissingField("payload"))?;

        Ok((path, payload))
    }

    async fn execute(
        &self,
        ctx: &K::ToolCx<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError> {
        let _first = ctx
            .invoke_tool(input.0.clone(), None, input.1.clone())
            .await?;
        let second = ctx.invoke_tool(input.0, None, input.1).await?;
        Ok(second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_file::ReadFileTool;
    use mvp_access_fs::AllowWorkspaceReadPolicy;
    use mvp_contract::InvocationParams;
    use mvp_contract::{Capabilities, Capability, OutputClassification};
    use mvp_kernel::error::{AuthorizationError, ToolError};
    use mvp_test_support::{MockKernel, TempWorkspace};
    use serde_json::json;

    struct EscalatingInvokeTool;

    #[async_trait]
    impl<K> ToolImpl<K> for EscalatingInvokeTool
    where
        K: Kernel<ToolPath = String>,
        for<'a> K::ToolCx<'a>: ToolContext<K>,
    {
        type Input = (K::ToolPath, Value);
        type Output = ToolOutcome;

        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "escalate_invoke".into(),
                description: "Attempt to expand nested invocation capabilities.".into(),
                capabilities: Capabilities::empty(),
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

            Ok((name, payload))
        }

        async fn execute(
            &self,
            ctx: &K::ToolCx<'_>,
            input: Self::Input,
        ) -> Result<Self::Output, ToolError> {
            ctx.invoke_tool(input.0, Some([Capability::FsRead].into()), input.1)
                .await
        }
    }

    #[tokio::test]
    async fn double_inherits_top_level_capability_override() {
        let ws = TempWorkspace::with_prefix("builtin-double-inherit");
        std::fs::write(ws.root.join("hello.txt"), "hello through double").unwrap();

        let mut kernel = MockKernel::new();
        kernel.register("double".to_owned(), Double).unwrap();
        kernel
            .register("read_file".to_owned(), ReadFileTool)
            .unwrap();
        kernel.policy.append(AllowWorkspaceReadPolicy);

        let params = InvocationParams::new(&ws.root, Some([Capability::FsRead].into()));
        let outcome = mvp_kernel::kernel::Kernel::invoke(
            &kernel,
            &"double".to_string(),
            &params,
            json!({
                "path": "read_file",
                "payload": { "path": "hello.txt" },
            }),
        )
        .await
        .unwrap();

        assert_eq!(outcome.payload["content"], "hello through double");
        assert_eq!(outcome.classification, OutputClassification::WorkspaceLocal);
    }

    #[tokio::test]
    async fn nested_override_cannot_expand_parent_effective_capabilities() {
        let ws = TempWorkspace::with_prefix("builtin-double-escalate");
        std::fs::write(ws.root.join("hello.txt"), "blocked").unwrap();

        let mut kernel = MockKernel::new();
        kernel
            .register("escalate_invoke".to_owned(), EscalatingInvokeTool)
            .unwrap();
        kernel
            .register("read_file".to_owned(), ReadFileTool)
            .unwrap();
        kernel.policy.append(AllowWorkspaceReadPolicy);

        let params = InvocationParams::new(&ws.root, None);
        let denied = mvp_kernel::kernel::Kernel::invoke(
            &kernel,
            &"escalate_invoke".to_string(),
            &params,
            json!({
                "name": "read_file",
                "payload": { "path": "hello.txt" },
            }),
        )
        .await;

        assert!(matches!(
            denied,
            Err(ToolError::Authorization(AuthorizationError::Denied(_)))
        ));
    }
}
