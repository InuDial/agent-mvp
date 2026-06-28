use async_trait::async_trait;
use mvp_access_fs::{FsBackend, HasFsAccess};
use mvp_contract::{Capability, OutputClassification, ToolOutcome, ToolSpec};
use mvp_core::error::{InputError, ToolError};
use mvp_core::tool::ToolHost;
use mvp_core::tool::ToolImpl;
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
impl<K> ToolImpl<K> for ReadFileTool
where
    K: ToolHost + FsBackend,
    for<'a> K::ToolCx<'a>: HasFsAccess<K>,
{
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
        ctx: &K::ToolCx<'_>,
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
    use mvp_access_fs::AllowWorkspaceReadPolicy;
    use mvp_contract::InvocationParams;
    use mvp_test_support::{MockKernel, TempWorkspace};

    #[tokio::test]
    async fn read_file_goes_through_kernel_pipeline() {
        let ws = TempWorkspace::with_prefix("builtin-read-file");
        std::fs::write(ws.root.join("hello.txt"), "hello from tool plane").unwrap();

        let mut kernel = MockKernel::new();
        kernel
            .register("read_file".to_owned(), ReadFileTool)
            .unwrap();
        kernel.policy.append(AllowWorkspaceReadPolicy);

        let params = InvocationParams::new(&ws.root, None);
        let outcome = mvp_core::tool::ToolHost::invoke(
            &kernel,
            &"read_file".to_string(),
            &params,
            json!({ "path": "hello.txt" }),
        )
        .await
        .unwrap();

        assert_eq!(outcome.payload["content"], "hello from tool plane");
        assert_eq!(outcome.classification, OutputClassification::WorkspaceLocal);
    }
}
