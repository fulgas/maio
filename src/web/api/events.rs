//! `GET /events` (arc42 3.5): server-sent events.

use std::convert::Infallible;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;

use crate::web::{auth::Me, state::Shared};

pub async fn events(State(app): State<Shared>, _me: Me) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut stopping = app.stopping.subscribe();
    let stopped = async move {
        let _ = stopping.wait_for(|s| *s).await;
    };
    // Ends when the server stops, so a graceful shutdown does not wait for open browsers.
    let stream = BroadcastStream::new(app.events.subscribe())
        .filter_map(|e| async move { e.ok().map(|e| Ok(Event::default().event(e.kind).data(e.data.to_string()))) })
        .take_until(stopped);
    Sse::new(stream).keep_alive(KeepAlive::default())
}
