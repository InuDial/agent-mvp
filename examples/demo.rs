use std::time::{SystemTime, UNIX_EPOCH};

use mvp_app::App;
use mvp_builtin::{double::Double, read_file::ReadFileTool, write_file::WriteFileTool};
use mvp_contract::{Capability, InvocationParams};
use mvp_kernel::kernel::Kernel;
use mvp_kernel::service::fs::{
    AllowExactFileWritePolicy, AllowWorkspaceFsPolicy, AllowWorkspaceReadPolicy,
};
use serde_json::json;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("mvp::audit=debug")),
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

    let mut app = App::new();
    app.register(WriteFileTool).unwrap();
    app.register(ReadFileTool).unwrap();
    app.register(Double).unwrap();
    app.policy.append(AllowWorkspaceFsPolicy);
    app.policy
        .append(AllowExactFileWritePolicy::new(root.join("hello.txt")));
    app.policy.append(AllowWorkspaceReadPolicy);

    let write_params = InvocationParams::new(&root, Some([Capability::FsWrite].into()));
    let write_outcome = app
        .invoke(
            "write_file".into(),
            &write_params,
            json!({
                "path": "hello.txt",
                "content": "hello from demo",
            }),
        )
        .await
        .unwrap();

    let read_params = InvocationParams::new(&root, Some([Capability::FsRead].into()));
    let read_outcome = app
        .invoke(
            "double".into(),
            &read_params,
            json!({
                "name": "read_file",
                "payload": { "path": "hello.txt" },
            }),
        )
        .await
        .unwrap();

    let read_outcome_err = app
        .invoke(
            "double".into(),
            &write_params,
            json!({
                "name": "read_file",
                "payload": { "path": "hello.txt" },
            }),
        )
        .await
        .unwrap_err();

    println!("write_outcome:\n{write_outcome:#?}");
    println!("read_outcome:\n{read_outcome:#?}");
    println!("read_err:\n{read_outcome_err:#?}");

    std::fs::remove_dir_all(root).unwrap();
}
