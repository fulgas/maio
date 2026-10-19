//! Engine calls carry the W3C trace context (SPEC 13), so ACE-Step's spans join Maio's trace.
//! Alone in its own binary: tracing caches whether a span is wanted, and tests beside it would make it cache "no".

use std::sync::{Arc, Mutex};

use axum::{Router, extract::State, http::HeaderMap, routing::post};
use maio::adapters::engine::{EngineConfig, HttpEngine};
use maio::usecases::ports::{EngineRequest, MusicEngine};
use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::layer::SubscriberExt;

#[tokio::test]
async fn a_submitted_song_carries_traceparent() {
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let app = Router::new()
        .route(
            "/release_task",
            post(|State(s): State<Arc<Mutex<Vec<String>>>>, h: HeaderMap| async move {
                s.lock().unwrap().push(h.get("traceparent").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string());
                axum::Json(serde_json::json!({"data": {"task_id": "t1"}, "code": 200, "error": null}))
            }),
        )
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    opentelemetry::global::set_text_map_propagator(opentelemetry_sdk::propagation::TraceContextPropagator::new());
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
    let subscriber = tracing_subscriber::registry().with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let _guard = tracing::subscriber::set_default(subscriber);

    let engine = HttpEngine::new(EngineConfig { id: "ace-step".into(), url });
    let request = EngineRequest {
        prompt: "calm".into(),
        seconds: 30,
        bpm: None,
        vocals: false,
        lyrics: None,
        seed: 1,
        vocal_language: "en".into(),
        write_lyrics: false,
    };
    let span = tracing::info_span!("job", job = "j1");
    let _entered = span.enter();
    engine.submit(&request).await.unwrap();
    let got = seen.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert!(regex_like(&got[0]), "a W3C traceparent: {:?}", got[0]);
}

/// `00-<32 hex>-<16 hex>-<2 hex>`.
fn regex_like(v: &str) -> bool {
    let parts: Vec<&str> = v.split('-').collect();
    parts.len() == 4
        && parts[0] == "00"
        && parts[1].len() == 32
        && parts[2].len() == 16
        && parts[3].len() == 2
        && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
}
