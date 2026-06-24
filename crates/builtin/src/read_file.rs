use async_trait::async_trait;
use mvp_contract::{Capability, OutputClassification, ToolOutcome, ToolSpec};
use mvp_kernel::error::{InputError, ToolError};

use mvp_kernel::tool::{ToolImpl, ToolPlaneContext};
use serde_json::{Value, json};

pub struct ReadFileTool;

pub struct ReadFileInput {
    path: String,
}

pub struct ReadFileOutput {
    content: String,
}

impl From<ReadFileOutput> for ToolOutcome {
    fn from(output: ReadFileOutput) -> Self {
        Self {
            payload: json!({ "content": output.content }),
            classification: OutputClassification::WorkspaceLocal,
        }
    }
}

#[async_trait]
impl ToolImpl for ReadFileTool {
    type Input = ReadFileInput;
    type Output = ReadFileOutput;

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_file".into(),
            description: "Read a file".into(),
            capabilities: [Capability::FsRead].into(),
        }
    }

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError> {
        let path = payload
            .get("path")
            .and_then(Value::as_str)
            .ok_or(InputError::MissingField("path"))?
            .to_owned();

        Ok(ReadFileInput { path })
    }

    async fn execute(
        &self,
        ctx: &ToolPlaneContext<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError> {
        let content = ctx
            .fs()
            .read_file(&input.path)
            .await
            .map_err(ToolError::Execution)?;

        Ok(ReadFileOutput { content })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mvp_contract::ToolRequest;
    use mvp_kernel::{
        service::{
            fs::{AllowWorkspaceReadPolicy, StdFs},
            network::DenyNetwork,
        },
        tool::{InvocationParams, ToolPlane},
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn read_file_goes_through_kernel_pipeline() {
        let root = std::env::temp_dir().join(format!(
            "tool-plane-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("hello.txt"), "hello from tool plane").unwrap();

        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane.register(ReadFileTool).unwrap();
        plane.policy.append(AllowWorkspaceReadPolicy);

        let params = InvocationParams::new(&root);
        let outcome = plane
            .invoke(
                &params,
                None,
                ToolRequest {
                    name: "read_file".into(),
                    payload: json!({ "path": "hello.txt" }),
                },
            )
            .await
            .unwrap();

        assert_eq!(outcome.payload["content"], "hello from tool plane");
        assert_eq!(outcome.classification, OutputClassification::WorkspaceLocal);

        std::fs::remove_dir_all(root).unwrap();
    }
}
