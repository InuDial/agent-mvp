use async_trait::async_trait;
use mvp_contract::{Capability, OutputClassification, ToolOutcome, ToolSpec};
use mvp_kernel::error::{InputError, ToolError};
use mvp_kernel::kernel::Kernel;
use mvp_kernel::service::fs::{HasFsBackend, HasFsService};
use mvp_kernel::tool::ToolImpl;
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
impl<K> ToolImpl<K> for WriteFileTool
where
    K: Kernel + HasFsBackend,
    for<'a> K::ToolCx<'a>: HasFsService<K>,
{
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
        ctx: &K::ToolCx<'_>,
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
    use mvp_contract::InvocationParams;
    use mvp_kernel::{
        error::{AuthorizationError, ToolError},
        service::fs::AllowWorkspaceWritePolicy,
        test_support::{MockKernel, TempWorkspace},
    };

    #[tokio::test]
    async fn write_file_goes_through_kernel_pipeline() {
        let ws = TempWorkspace::with_prefix("builtin-write-file");
        std::fs::create_dir_all(ws.root.join("nested")).unwrap();

        let mut kernel = MockKernel::new();
        kernel.register(WriteFileTool).unwrap();
        kernel.policy.append(AllowWorkspaceWritePolicy);

        let params = InvocationParams::new(&ws.root, None);
        let outcome = mvp_kernel::kernel::Kernel::invoke(
            &kernel,
            "write_file".into(),
            &params,
            json!({
                "path": "nested/hello.txt",
                "content": "hello from write tool"
            }),
        )
        .await
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(ws.root.join("nested/hello.txt")).unwrap(),
            "hello from write tool"
        );
        assert_eq!(outcome.payload["ok"], true);
        assert_eq!(outcome.classification, OutputClassification::Public);
    }

    #[tokio::test]
    async fn write_file_requires_matching_policy() {
        let ws = TempWorkspace::with_prefix("builtin-write-file-denied");

        let mut kernel = MockKernel::new();
        kernel.register(WriteFileTool).unwrap();

        let params = InvocationParams::new(&ws.root, None);
        let denied = mvp_kernel::kernel::Kernel::invoke(
            &kernel,
            "write_file".into(),
            &params,
            json!({
                "path": "hello.txt",
                "content": "blocked"
            }),
        )
        .await;

        assert!(matches!(
            denied,
            Err(ToolError::Execution(
                mvp_kernel::error::ExecutionError::Authorization(AuthorizationError::Denied(_))
            ))
        ));
        assert!(!ws.root.join("hello.txt").exists());
    }
}
