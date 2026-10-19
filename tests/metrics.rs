mod common;

use std::{sync::atomic::Ordering, time::Duration};

use axum::http::StatusCode;
use common::*;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider, data::ResourceMetrics};
use serde_json::json;

fn names(metrics: &[ResourceMetrics]) -> Vec<String> {
    metrics.iter().flat_map(|r| r.scope_metrics()).flat_map(|s| s.metrics()).map(|m| m.name().to_string()).collect()
}

#[tokio::test]
async fn requests_and_finished_jobs_are_counted() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder().with_reader(PeriodicReader::builder(exporter.clone()).build()).build();
    let w = world_with(&provider.meter("test"));
    w.engine.hold.store(true, Ordering::SeqCst);
    let (_, _, accepted) = send(&w, script("POST", "/api/v1/songs", Some(json!({"mood": "calm"})))).await;
    let (status, _, _) = send(&w, script("DELETE", &format!("/api/v1/jobs/{}", accepted["job_id"].as_str().unwrap()), None)).await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(20)).await;
    provider.force_flush().unwrap();
    let found = names(&exporter.get_finished_metrics().unwrap());
    assert!(found.contains(&"http.server.request.duration".to_string()), "{found:?}");
    assert!(found.contains(&"maio.jobs".to_string()), "{found:?}");
    let debug = format!("{:?}", exporter.get_finished_metrics().unwrap());
    assert!(debug.contains("/api/v1/jobs/{id}") && debug.contains("cancelled"), "routes are templates and outcomes are named: {debug}");
}

#[tokio::test]
async fn every_request_is_a_span_at_the_default_level() {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
    use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt};

    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder().with_simple_exporter(exporter.clone()).build();
    let subscriber =
        tracing_subscriber::registry().with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")).with_filter(EnvFilter::new("info")));
    let _guard = tracing::subscriber::set_default(subscriber);
    let w = world();
    send(&w, script("GET", "/api/v1/status", None)).await;
    provider.force_flush().unwrap();
    let names: Vec<String> = exporter.get_finished_spans().unwrap().iter().map(|s| s.name.to_string()).collect();
    assert!(names.contains(&"request".to_string()), "{names:?}");
}
