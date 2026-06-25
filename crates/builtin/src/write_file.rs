use async_trait::async_trait;
use mvp_contract::{Capability, OutputClassification, ToolOutcome, ToolSpec};
use mvp_kernel::error::{InputError, ToolError};
use mvp_kernel::tool::{ToolImpl, ToolPlaneContext};
use serde_json::{Value, json};

pub struct WriteFileTool;

pub struct WriteFileInput {
    path: String,
    content: String,
}

pub struct WriteFileOutput;

impl From<WriteFileOutput> for ToolOutcome {
    fn from(_output: WriteFileOutput) -> Self {
        Self {
            payload: json!({ "ok": true }),
            classification: OutputClassification::Public,
        }
    }
}

#[async_trait]
impl ToolImpl for WriteFileTool {
    type Input = WriteFileInput;
    type Output = WriteFileOutput;

    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write_file".into(),
            description: "Write a file".into(),
            capabilities: [Capability::FsWrite].into(),
        }
    }

    fn parse_input(&self, payload: Value) -> Result<Self::Input, InputError> {
        let path = payload
            .get("path")
            .and_then(Value::as_str)
            .ok_or(InputError::MissingField("path"))?
            .to_owned();
        let content = payload
            .get("content")
            .and_then(Value::as_str)
            .ok_or(InputError::MissingField("content"))?
            .to_owned();

        Ok(WriteFileInput { path, content })
    }

    async fn execute(
        &self,
        ctx: &ToolPlaneContext<'_>,
        input: Self::Input,
    ) -> Result<Self::Output, ToolError> {
        ctx.fs()
            .write_file(&input.path, &input.content)
            .await
            .map_err(ToolError::Execution)?;

        Ok(WriteFileOutput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mvp_contract::ToolRequest;
    use mvp_kernel::{
        error::{AuthorizationError, ToolError},
        service::{
            fs::{AllowWorkspaceWritePolicy, StdFs},
            network::DenyNetwork,
        },
        tool::{InvocationParams, ToolPlane},
    };
    use std::time::{SystemTime, UNIX_EPOCH};

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
    async fn write_file_goes_through_kernel_pipeline() {
        let root = temp_root("write-file");
        std::fs::create_dir_all(root.join("nested")).unwrap();

        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane.register(WriteFileTool).unwrap();
        plane.policy.append(AllowWorkspaceWritePolicy);

        let params = InvocationParams::new(&root);
        let outcome = plane
            .invoke(
                &params,
                None,
                ToolRequest {
                    name: "write_file".into(),
                    payload: json!({
                        "path": "nested/hello.txt",
                        "content": "hello from write tool"
                    }),
                },
            )
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(root.join("nested/hello.txt")).unwrap(),
            "hello from write tool"
        );
        assert_eq!(outcome.payload["ok"], true);
        assert_eq!(outcome.classification, OutputClassification::Public);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn write_file_requires_matching_policy() {
        let root = temp_root("write-file-denied");
        std::fs::create_dir_all(&root).unwrap();

        let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
        plane.register(WriteFileTool).unwrap();

        let params = InvocationParams::new(&root);
        let denied = plane
            .invoke(
                &params,
                None,
                ToolRequest {
                    name: "write_file".into(),
                    payload: json!({
                        "path": "hello.txt",
                        "content": "blocked"
                    }),
                },
            )
            .await;

        assert!(matches!(
            denied,
            Err(ToolError::Execution(
                mvp_kernel::error::ExecutionError::Authorization(AuthorizationError::Denied(_))
            ))
        ));
        assert!(!root.join("hello.txt").exists());

        std::fs::remove_dir_all(root).unwrap();
    }
}
