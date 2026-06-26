use std::time::{SystemTime, UNIX_EPOCH};

use mvp_app::App;
use mvp_builtin::{double::Double, read_file::ReadFileTool, write_file::WriteFileTool};
use mvp_contract::{Capability, InvocationParams, ToolRequest};
use mvp_kernel::kernel::Kernel;
use mvp_kernel::service::fs::{AllowWorkspaceReadPolicy, AllowWorkspaceWritePolicy};
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

    let mut plane = App::new();
    plane.register(WriteFileTool).unwrap();
    plane.register(ReadFileTool).unwrap();
    plane.register(Double).unwrap();
    plane.policy.append(AllowWorkspaceWritePolicy);
    plane.policy.append(AllowWorkspaceReadPolicy);

    let write_params = InvocationParams::new(&root, Some([Capability::FsWrite].into()));
    let write_outcome = plane
        .invoke(
            &write_params,
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

    let read_params = InvocationParams::new(&root, Some([Capability::FsRead].into()));
    let read_outcome = plane
        .invoke(
            &read_params,
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
