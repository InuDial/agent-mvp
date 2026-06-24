use std::time::{SystemTime, UNIX_EPOCH};

use mvp_builtin::{double::Double, read_file::ReadFileTool};
use mvp_contract::ToolRequest;
use mvp_kernel::{
    service::{
        fs::{AllowWorkspaceReadPolicy, StdFs},
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
    std::fs::write(root.join("hello.txt"), "hello from demo").unwrap();

    let mut plane = ToolPlane::new(StdFs::new(), DenyNetwork);
    plane.register(ReadFileTool).unwrap();
    plane.register(Double).unwrap();
    plane.policy.append(AllowWorkspaceReadPolicy);

    let params = InvocationParams::new(&root);
    let outcome = plane
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

    println!("outcome:\n{outcome:#?}");

    std::fs::remove_dir_all(root).unwrap();
}
