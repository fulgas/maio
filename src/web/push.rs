//! Builds each event from the state as it is now, so it carries what the web app shows and the web app never
//! fetches to catch up (arc42 3.5, ADR 0018). Request handlers and the adapters say what changed; this says how it is.

use serde_json::json;
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot,
};

use crate::web::state::{App, Shared};

pub use crate::usecases::ports::Change;

/// What a request changed, and how it learns the events are out.
pub type Asked = UnboundedSender<(Vec<Change>, oneshot::Sender<()>)>;

/// Builds and tells every event, one at a time: the adapters' changes and the requests' alike. Each event reads the
/// state as it is when it is built, so the one told last always carries the newest state.
pub async fn run(app: Shared, mut changes: UnboundedReceiver<Change>, mut asked: UnboundedReceiver<(Vec<Change>, oneshot::Sender<()>)>) {
    loop {
        let (batch, done) = tokio::select! {
            Some(change) = changes.recv() => (vec![change], None),
            Some((batch, done)) = asked.recv() => (batch, Some(done)),
            else => return,
        };
        build(&app, batch).await;
        if let Some(done) = done {
            _ = done.send(()); // the request may have gone; its events are out anyway
        }
    }
}

/// Has `run` tell what a request changed, and waits until it has, so the answer comes after its events.
pub async fn send(app: &Shared, changes: Vec<Change>) {
    let (done, told) = oneshot::channel();
    if app.asked.send((changes, done)).is_ok() {
        drop(told.await);
    }
}

/// Tells every browser, off the async runtime: building an event reads the library.
async fn build(app: &Shared, changes: Vec<Change>) {
    let app = app.clone();
    if let Err(e) = tokio::task::spawn_blocking(move || changes.into_iter().for_each(|c| tell(&app, c))).await {
        tracing::error!("an event was not built: {e}");
    }
}

fn tell(app: &App, change: Change) {
    match change {
        Change::Song(id) => {
            let song = app.songs.get(&id).ok();
            app.events.send("song", json!({"id": id, "deleted": song.is_none(), "song": song}));
        }
        Change::Playlist(id) => match app.playlists.get(&id) {
            Ok(view) => {
                let summary = app.playlists.list().ok().and_then(|all| all.into_iter().find(|p| p.id == id));
                let songs: Vec<String> = view.songs.iter().map(|s| s.id.clone()).collect();
                app.events.send("playlist", json!({"id": id, "deleted": false, "summary": summary, "view": view}));
                songs.into_iter().for_each(|s| tell(app, Change::Song(s)));
            }
            Err(_) => app.events.send("playlist", json!({"id": id, "deleted": true, "summary": null, "view": null})),
        },
        Change::Live(id) => match app.playlists.live(&id) {
            Ok(v) => {
                let relay = app.live.relay_addresses(&v.playlist);
                app.events.send("live", json!({"id": id, "deleted": false, "live": crate::web::live::json(v, &app.serve, relay)}));
            }
            Err(_) => app.events.send("live", json!({"id": id, "deleted": true, "live": null})),
        },
        Change::Engine(id) => {
            if let Some(engine) = app.making.engines().into_iter().find(|e| e.id == id) {
                app.events.send("engine", json!({"id": id, "engine": engine}));
            }
        }
        Change::Relay => app.events.send("relay", json!({"relay": app.live.relay_status().ok().flatten()})),
    }
}
