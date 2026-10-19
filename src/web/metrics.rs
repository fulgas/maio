//! Metrics (SPEC 13): API requests by route, jobs by outcome, engine health.
//! Without OTLP the meter does nothing.

use std::{
    sync::{Arc, OnceLock},
    time::Instant,
};

use crate::domain::{
    engine::Health,
    job::{Job, JobState},
};
use crate::usecases::Making;
use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use opentelemetry::{
    KeyValue,
    metrics::{Counter, Histogram, Meter, ObservableGauge},
};

pub struct Metrics {
    requests: Histogram<f64>,
    jobs: Counter<u64>,
    /// Read whenever metrics are collected; kept so its callback stays registered.
    engines: OnceLock<ObservableGauge<u64>>,
}

impl Metrics {
    pub fn new(meter: &Meter) -> Self {
        Self {
            requests: meter.f64_histogram("http.server.request.duration").with_unit("s").with_description("How long API requests take").build(),
            jobs: meter.u64_counter("maio.jobs").with_description("Jobs that finished, by outcome").build(),
            engines: OnceLock::new(),
        }
    }

    /// Reports each music engine as 1 while up, else 0. `Making` exists only after these metrics, hence the second step.
    pub fn watch_engines(&self, meter: &Meter, making: Arc<Making>) {
        let gauge = meter
            .u64_observable_gauge("maio.engine.up")
            .with_description("1 while a music engine is up, else 0")
            .with_callback(move |gauge| {
                for e in making.engines() {
                    gauge.observe(u64::from(e.health == Health::Up), &[KeyValue::new("engine", e.id)]);
                }
            })
            .build();
        let _ = self.engines.set(gauge);
    }

    /// Counts a job once it is done, failed or cancelled.
    pub fn job_finished(&self, job: &Job) {
        let outcome = match job.state {
            JobState::Done => "done",
            JobState::Failed => "failed",
            JobState::Cancelled => "cancelled",
            JobState::Waiting | JobState::Making => return,
        };
        self.jobs.add(1, &[KeyValue::new("outcome", outcome)]);
    }
}

/// Records each API request's duration by route template (never the ids in it), method and status.
pub async fn record(State(metrics): State<Arc<Metrics>>, req: Request, next: Next) -> Response {
    let route = req.extensions().get::<MatchedPath>().map_or_else(|| "unmatched".to_string(), |p| p.as_str().to_string());
    let method = req.method().to_string();
    let started = Instant::now();
    let res = next.run(req).await;
    let attributes = [
        KeyValue::new("http.route", route),
        KeyValue::new("http.request.method", method),
        KeyValue::new("http.response.status_code", i64::from(res.status().as_u16())),
    ];
    metrics.requests.record(started.elapsed().as_secs_f64(), &attributes);
    res
}
