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
        let _first = ctx.invoke_tool(None, input.clone()).await?;
        let second = ctx.invoke_tool(None, input).await?;
        Ok(second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_file::ReadFileTool;
    use mvp_contract::{Capabilities, Capability, OutputClassification, ToolRequest};
    use mvp_kernel::{
        error::{AuthorizationError, ToolError},
        service::{
            fs::{AllowWorkspaceReadPolicy, StdFs},
            network::DenyNetwork,
        },
        tool::{InvocationParams, ToolPlane},
    };
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EscalatingInvokeTool;

    #[async_trait]
    impl ToolImpl for EscalatingInvokeTool {
        type Input = ToolRequest;
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

            Ok(ToolRequest { name, payload })
        }

        async fn execute(
            &self,
            ctx: &ToolPlaneContext<'_>,
            input: Self::Input,
        ) -> Result<Self::Output, ToolError> {
            ctx.invoke_tool(Some([Capability::FsRead].into()), input)
                .await
        }
    }

    fn temp_root(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "tool-plane-{}-{}-{}",
            prefix,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn double_inherits_top_level_capability_override() {
        let root = temp_root("double-inherit");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("hello.txt"), "hello through double").unwrap();

        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane.register(Double).unwrap();
        plane.register(ReadFileTool).unwrap();
        plane.policy.append(AllowWorkspaceReadPolicy);

        let params = InvocationParams::new(&root);
        let outcome = plane
            .invoke(
                &params,
                Some([Capability::FsRead].into()),
                ToolRequest {
                    name: "double".into(),
                    payload: json!({
                        "name": "read_file",
                        "payload": { "path": "hello.txt" },
                    }),
                },
            )
            .await
            .unwrap();

        assert_eq!(outcome.payload["content"], "hello through double");
        assert_eq!(outcome.classification, OutputClassification::WorkspaceLocal);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn nested_override_cannot_expand_parent_effective_capabilities() {
        let root = temp_root("double-escalate");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("hello.txt"), "blocked").unwrap();

        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane.register(EscalatingInvokeTool).unwrap();
        plane.register(ReadFileTool).unwrap();
        plane.policy.append(AllowWorkspaceReadPolicy);

        let params = InvocationParams::new(&root);
        let denied = plane
            .invoke(
                &params,
                None,
                ToolRequest {
                    name: "escalate_invoke".into(),
                    payload: json!({
                        "name": "read_file",
                        "payload": { "path": "hello.txt" },
                    }),
                },
            )
            .await;

        assert!(matches!(
            denied,
            Err(ToolError::Authorization(AuthorizationError::Denied(_)))
        ));

        std::fs::remove_dir_all(root).unwrap();
    }
}
