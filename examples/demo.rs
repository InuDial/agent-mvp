use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mvp_access_fs::{AllowExactFileWritePolicy, AllowWorkspaceFsPolicy, AllowWorkspaceReadPolicy};
use mvp_access_monty::{AllowMontySessionPolicy, MontySessionLoadAction, MontySessionSaveAction};
use mvp_app::App;
use mvp_contract::{Capability, InvocationParams};
use mvp_kernel::{audit::AUDIT_TARGET, kernel::Kernel};
use mvp_tool_builtin::{double::Double, read_file::ReadFileTool, write_file::WriteFileTool};
use mvp_tool_monty::{MontyOsTool, MontyTool};
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
use serde_json::json;
use tracing::{Instrument, info_span, subscriber::set_global_default};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogFormat {
    Human,
    Json,
}

#[derive(Debug)]
struct TracingConfig {
    log_format: LogFormat,
    tracer_provider: Option<SdkTracerProvider>,
}

impl TracingConfig {
    fn needs_root_span(&self) -> bool {
        self.log_format == LogFormat::Json || self.tracer_provider.is_some()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let tracing_config = init_tracing();
    let log_format = tracing_config.log_format;

    if tracing_config.needs_root_span() {
        run_demo(log_format)
            .instrument(info_span!(target: AUDIT_TARGET, "demo"))
            .await;
    } else {
        run_demo(log_format).await;
    }

    if let Some(tracer_provider) = tracing_config.tracer_provider {
        if let Err(error) = tracer_provider.shutdown_with_timeout(Duration::from_secs(1)) {
            eprintln!("failed to shutdown OTel tracer provider: {error}");
        }
    }
}

async fn run_demo(log_format: LogFormat) {
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
    app.register("write_file".to_owned(), WriteFileTool)
        .unwrap();
    app.register("read_file".to_owned(), ReadFileTool).unwrap();
    app.register("double".to_owned(), Double).unwrap();
    app.register(
        "monty".to_owned(),
        MontyTool::new()
            .expose("read_file", "read_file")
            .expose("write_file", "write_file"),
    )
    .unwrap();
    app.register("monty_os".to_owned(), MontyOsTool).unwrap();

    app.policy.append(AllowWorkspaceFsPolicy);
    app.policy
        .append(AllowExactFileWritePolicy::new(root.join("hello.txt")));
    app.policy.append(AllowWorkspaceReadPolicy);

    app.policy
        .append::<MontySessionLoadAction, _>(AllowMontySessionPolicy);
    app.policy
        .append::<MontySessionSaveAction, _>(AllowMontySessionPolicy);

    let write_params = InvocationParams::new(&root, Some([Capability::FsWrite].into()));
    let write_outcome = app
        .invoke(
            &"write_file".to_string(),
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
            &"double".to_string(),
            &read_params,
            json!({
                "path": "read_file",
                "payload": { "path": "hello.txt" },
            }),
        )
        .await
        .unwrap();

    let read_outcome_err = app
        .invoke(
            &"double".to_string(),
            &write_params,
            json!({
                "path": "read_file",
                "payload": { "path": "hello.txt" },
            }),
        )
        .await
        .unwrap_err();

    let monty_read_outcome = app
        .invoke(
            &"monty".to_string(),
            &read_params,
            json!({
                "session_id": "demo",
                "code": "from pathlib import Path\nPath('hello.txt').read_text()",
            }),
        )
        .await
        .unwrap();

    print_demo_outcomes(
        log_format,
        &format!(
            "write_outcome:\n{write_outcome:#?}\nread_outcome:\n{read_outcome:#?}\nread_err:\n{read_outcome_err:#?}\nmonty_read_outcome:\n{monty_read_outcome:#?}"
        ),
    );

    std::fs::remove_dir_all(root).unwrap();
}

fn init_tracing() -> TracingConfig {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("{AUDIT_TARGET}=debug")));
    let log_format = match std::env::var("MVP_LOG_FORMAT").as_deref() {
        Ok("json") => LogFormat::Json,
        _ => LogFormat::Human,
    };
    let tracer_provider = match std::env::var("MVP_TRACE_EXPORTER").as_deref() {
        Ok("otlp") => Some(init_otlp_tracer_provider()),
        _ => None,
    };

    let registry = Registry::default().with(filter);
    match (log_format, tracer_provider.clone()) {
        (LogFormat::Json, Some(tracer_provider)) => {
            let tracer = tracer_provider.tracer("mvp-demo");
            let subscriber = registry
                .with(
                    fmt::layer()
                        .json()
                        .with_current_span(true)
                        .with_span_list(true)
                        .with_writer(std::io::stdout),
                )
                .with(OpenTelemetryLayer::new(tracer));
            set_global_default(subscriber).expect("failed to install tracing subscriber");
        }
        (LogFormat::Json, None) => {
            let subscriber = registry.with(
                fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_writer(std::io::stdout),
            );
            set_global_default(subscriber).expect("failed to install tracing subscriber");
        }
        (LogFormat::Human, Some(tracer_provider)) => {
            let tracer = tracer_provider.tracer("mvp-demo");
            let subscriber = registry
                .with(fmt::layer())
                .with(OpenTelemetryLayer::new(tracer));
            set_global_default(subscriber).expect("failed to install tracing subscriber");
        }
        (LogFormat::Human, None) => {
            let subscriber = registry.with(fmt::layer());
            set_global_default(subscriber).expect("failed to install tracing subscriber");
        }
    }

    TracingConfig {
        log_format,
        tracer_provider,
    }
}

fn init_otlp_tracer_provider() -> SdkTracerProvider {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4318/v1/traces".to_owned());
    eprintln!("exporting OTel traces to {endpoint}");

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_timeout(Duration::from_secs(1))
        .with_endpoint(endpoint)
        .build()
        .expect("failed to create OTLP span exporter");

    SdkTracerProvider::builder()
        .with_resource(Resource::builder().with_service_name("mvp-demo").build())
        .with_batch_exporter(exporter)
        .build()
}

fn print_demo_outcomes(log_format: LogFormat, output: &str) {
    match log_format {
        LogFormat::Human => println!("{output}"),
        LogFormat::Json => eprintln!("{output}"),
    }
}
