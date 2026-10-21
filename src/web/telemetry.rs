//! Logs, traces and metrics (arc42 8.6). Logs always go to stdout, filtered by `RUST_LOG` (default `info`).
//! With `OTEL_EXPORTER_OTLP_ENDPOINT` set, traces, metrics and logs also go out over OTLP/HTTP.

use anyhow::{Context, Result};
use opentelemetry::{global, trace::TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter};
use opentelemetry_sdk::{Resource, logs::SdkLoggerProvider, metrics::SdkMeterProvider, trace::SdkTracerProvider};
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::web::config::LogFormat;

/// The OTLP providers, flushed and stopped on shutdown so the last spans, metrics and logs are not lost.
#[derive(Default)]
pub struct Telemetry {
    providers: Option<(SdkTracerProvider, SdkMeterProvider, SdkLoggerProvider)>,
}

impl Telemetry {
    pub fn shutdown(self) {
        if let Some((traces, metrics, logs)) = self.providers {
            for (what, result) in [("traces", traces.shutdown()), ("metrics", metrics.shutdown()), ("logs", logs.shutdown())] {
                if let Err(e) = result {
                    eprintln!("stopping the OTLP {what} export: {e}");
                }
            }
        }
    }
}

fn filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
}

/// What goes to OTLP: the same filter, minus the exporters' own HTTP crates, so exporting never reports on itself.
fn exported() -> EnvFilter {
    ["hyper=off", "h2=off", "reqwest=off", "opentelemetry=off", "opentelemetry_sdk=off", "opentelemetry_otlp=off"]
        .into_iter()
        .fold(filter(), |f, directive| f.add_directive(directive.parse().expect("a fixed directive parses")))
}

/// Starts logging, and OTLP when configured. Call it before the tokio runtime starts: the OTLP exporters
/// use their own blocking HTTP client on their own threads.
pub fn init(format: LogFormat) -> Result<Telemetry> {
    let stdout = match format {
        LogFormat::Text => tracing_subscriber::fmt::layer().boxed(),
        LogFormat::Json => tracing_subscriber::fmt::layer().json().boxed(),
    };
    if std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT").is_none() {
        tracing_subscriber::registry().with(stdout.with_filter(filter())).try_init().context("starting the log")?;
        return Ok(Telemetry::default());
    }
    let service = std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "maio".into());
    let resource = Resource::builder().with_service_name(service).build();
    let traces = SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_batch_exporter(SpanExporter::builder().with_http().build().context("making the OTLP span exporter")?)
        .build();
    let metrics = SdkMeterProvider::builder()
        .with_resource(resource.clone())
        .with_periodic_exporter(MetricExporter::builder().with_http().build().context("making the OTLP metric exporter")?)
        .build();
    let logs = SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(LogExporter::builder().with_http().build().context("making the OTLP log exporter")?)
        .build();
    global::set_meter_provider(metrics.clone());
    // W3C trace context on outgoing calls, so ACE-Step's spans join ours (arc42 8.6).
    global::set_text_map_propagator(opentelemetry_sdk::propagation::TraceContextPropagator::new());
    tracing_subscriber::registry()
        .with(stdout.with_filter(filter()))
        .with(tracing_opentelemetry::layer().with_tracer(traces.tracer("maio")).with_filter(exported()))
        .with(OpenTelemetryTracingBridge::new(&logs).with_filter(exported()))
        .try_init()
        .context("starting the log")?;
    tracing::info!("sending traces, metrics and logs over OTLP");
    Ok(Telemetry { providers: Some((traces, metrics, logs)) })
}
