//! `GET /events` (arc42 3.5): server-sent events.

use std::convert::Infallible;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};

use crate::web::{auth::Me, state::Shared};

pub async fn events(State(app): State<Shared>, _me: Me) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut stopping = app.stopping.subscribe();
    let stopped = async move {
        let _ = stopping.wait_for(|s| *s).await;
    };
    // Ends when the server stops, so a graceful shutdown does not wait for open browsers.
    Sse::new(told(app.events.subscribe()).take_until(stopped)).keep_alive(KeepAlive::default())
}

/// Each event as the browser gets it. A browser too far behind has missed events, which carry state: the stream ends,
/// so it reconnects and loads the page again rather than keep showing what it had (ADR 0018).
fn told(rx: tokio::sync::broadcast::Receiver<crate::web::events::Event>) -> impl Stream<Item = Result<Event, Infallible>> {
    BroadcastStream::new(rx)
        .take_while(|e| std::future::ready(!matches!(e, Err(BroadcastStreamRecvError::Lagged(_)))))
        .filter_map(|e| std::future::ready(e.ok().map(|e| Ok(Event::default().event(e.kind).data(e.data.to_string())))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_browser_too_far_behind_is_ended_so_it_reconnects_and_loads_again() {
        let (tx, rx) = tokio::sync::broadcast::channel(4);
        for _ in 0..10 {
            tx.send(crate::web::events::Event { kind: "song", data: serde_json::json!({}) }).unwrap();
        }
        let got = tokio::time::timeout(std::time::Duration::from_secs(1), told(rx).count()).await;
        assert_eq!(got, Ok(0), "ended at the gap, not left open with events missing");
    }
}
