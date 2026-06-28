use std::time::Duration;

use mvp_kernel::audit::AUDIT_TARGET;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
use tracing::subscriber::set_global_default;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogFormat {
    Human,
    Json,
}

#[derive(Debug)]
pub struct TracingConfig {
    pub log_format: LogFormat,
    pub tracer_provider: Option<SdkTracerProvider>,
}

impl TracingConfig {
    pub fn needs_root_span(&self) -> bool {
        self.log_format == LogFormat::Json || self.tracer_provider.is_some()
    }
}

pub fn init_tracing() -> TracingConfig {
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
