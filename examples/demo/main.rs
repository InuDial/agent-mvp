use std::time::Duration;

use mvp_kernel::audit::AUDIT_TARGET;
use tracing::{Instrument, info_span};

mod scenario;
mod tracing_config;

use scenario::run_demo;
use tracing_config::init_tracing;

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
