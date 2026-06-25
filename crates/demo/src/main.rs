use std::time::{SystemTime, UNIX_EPOCH};

use mvp_builtin::{double::Double, read_file::ReadFileTool, write_file::WriteFileTool};
use mvp_contract::ToolRequest;
use mvp_kernel::{
    service::{
        fs::{AllowWorkspaceReadPolicy, AllowWorkspaceWritePolicy, StdFs},
        network::DenyNetwork,
    },
    tool::{InvocationParams, ToolPlane},
};
use serde_json::json;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("tool_plane::audit=info")),
        )
        .init();

    let root = std::env::temp_dir().join(format!(
        "tool-plane-demo-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    std::fs::create_dir_all(&root).unwrap();

    let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
    plane.register(WriteFileTool).unwrap();
    plane.register(ReadFileTool).unwrap();
    plane.register(Double).unwrap();
    plane.policy.append(AllowWorkspaceWritePolicy);
    plane.policy.append(AllowWorkspaceReadPolicy);

    let params = InvocationParams::new(&root);
    let write_outcome = plane
        .invoke(
            &params,
            Some([mvp_contract::Capability::FsWrite].into()),
            ToolRequest {
                name: "write_file".into(),
                payload: json!({
                    "path": "hello.txt",
                    "content": "hello from demo",
                }),
            },
        )
        .await
        .unwrap();

    let read_outcome = plane
        .invoke(
            &params,
            Some([mvp_contract::Capability::FsRead].into()),
            ToolRequest {
                name: "double".into(),
                payload: json!({
                    "name": "read_file",
                    "payload":{ "path": "hello.txt" },
                }),
            },
        )
        .await
        .unwrap();

    println!("write_outcome:\n{write_outcome:#?}");
    println!("read_outcome:\n{read_outcome:#?}");

    std::fs::remove_dir_all(root).unwrap();
}
