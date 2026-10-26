//! Changes from `/api/v1/events` (arc42 3.5, ADR 0018). Each event carries the new state of one thing; it goes into
//! the store under that thing's id, and only the parts of the page that show it change. Nothing is fetched to catch
//! up, except everything once more after the stream reconnects, since events sent while it was down are lost. Loads
//! wait until the stream is open, so no change falls between a load and the first event.

use std::collections::HashMap;

use futures::{FutureExt, channel::oneshot, future::Shared};
use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{
    domain::{
        job::{Job, JobState},
        relay::RelayStatus,
    },
    ui::api::{self, EngineStatus, LiveJson, PlaylistSummary, PlaylistView, SongView},
};

/// The latest state of everything an event told, by id. `None` is deleted.
#[derive(Clone, Copy)]
pub struct Changes {
    /// Open jobs, plus failed ones until dismissed.
    pub jobs: RwSignal<Vec<Job>>,
    pub songs: RwSignal<HashMap<String, Option<SongView>>>,
    pub playlists: RwSignal<HashMap<String, Option<(PlaylistSummary, PlaylistView)>>>,
    pub live: RwSignal<HashMap<String, Option<LiveJson>>>,
    pub engines: RwSignal<HashMap<String, EngineStatus>>,
    /// MediaMTX as the last event told it; `Some(None)` when there is no relay.
    pub relay: RwSignal<Option<Option<RelayStatus>>>,
    /// Bumped when the stream reconnects: what is on the page is loaded again, and the store starts over.
    pub reloaded: RwSignal<u32>,
    /// Ready once the stream first opens; every load waits for it (`listening`).
    opened: StoredValue<Shared<oneshot::Receiver<()>>, LocalStorage>,
    open: StoredValue<Option<oneshot::Sender<()>>, LocalStorage>,
}

#[derive(Deserialize)]
struct SongEvent {
    id: String,
    song: Option<SongView>,
}

#[derive(Deserialize)]
struct PlaylistEvent {
    id: String,
    deleted: bool,
    summary: Option<PlaylistSummary>,
    view: Option<PlaylistView>,
}

#[derive(Deserialize)]
struct LiveEvent {
    id: String,
    live: Option<LiveJson>,
}

#[derive(Deserialize)]
struct EngineEvent {
    id: String,
    engine: EngineStatus,
}

#[derive(Deserialize)]
struct RelayEvent {
    relay: Option<RelayStatus>,
}

impl Changes {
    /// For a load that events then keep current: it runs again after every reconnect, and not before the event stream
    /// is open, so no change can fall between the load and the first event.
    pub fn listening(&self) -> impl Future<Output = ()> + use<> {
        self.reloaded.track();
        let opened = self.opened.get_value();
        async move { _ = opened.await }
    }

    /// The stream is open, or there is none to wait for: loads go ahead.
    fn opened(&self) {
        if let Some(open) = self.open.try_update_value(Option::take).flatten() {
            _ = open.send(());
        }
    }
}

pub fn provide() -> Changes {
    let (open, opened) = oneshot::channel();
    let c = Changes {
        jobs: RwSignal::new(vec![]),
        songs: RwSignal::new(HashMap::new()),
        playlists: RwSignal::new(HashMap::new()),
        live: RwSignal::new(HashMap::new()),
        engines: RwSignal::new(HashMap::new()),
        relay: RwSignal::new(None),
        reloaded: RwSignal::new(0),
        opened: StoredValue::new_local(opened.shared()),
        open: StoredValue::new_local(Some(open)),
    };
    provide_context(c);
    Effect::new(move |_| connect(c)); // effects run only in the browser, once here
    c
}

/// Puts a job's new state in the list.
pub fn upsert(jobs: &mut Vec<Job>, job: Job) {
    let at = jobs.iter().position(|j| j.id == job.id);
    match (job.state, at) {
        (JobState::Done | JobState::Cancelled, Some(i)) => {
            jobs.remove(i);
        }
        (JobState::Done | JobState::Cancelled, None) => {}
        (_, Some(i)) => jobs[i] = job,
        (_, None) => jobs.push(job),
    }
}

/// The playlists a page loaded, as events have changed them since: renamed or changed ones replaced, deleted ones
/// gone, new ones added at the end.
/// What a playlist event leaves in the store: gone when deleted, its new state when the event carries it, and nothing
/// when it carries neither (the server could not read it), so a failed read never makes a playlist look deleted.
pub fn playlist_told<S, V>(deleted: bool, summary: Option<S>, view: Option<V>) -> Option<Option<(S, V)>> {
    if deleted {
        return Some(None);
    }
    summary.zip(view).map(Some)
}

pub fn playlists_now(loaded: &[PlaylistSummary], told: &HashMap<String, Option<(PlaylistSummary, PlaylistView)>>) -> Vec<PlaylistSummary> {
    let mut all: Vec<PlaylistSummary> = loaded
        .iter()
        .filter_map(|p| match told.get(&p.id) {
            Some(Some((summary, _))) => Some(summary.clone()),
            Some(None) => None,
            None => Some(p.clone()),
        })
        .collect();
    let mut new: Vec<PlaylistSummary> = told.values().flatten().map(|(s, _)| s.clone()).filter(|s| !loaded.iter().any(|p| p.id == s.id)).collect();
    new.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    all.extend(new);
    all
}

/// The songs a list shows, as events have changed them since. With `take_new` (no search, newest first), a song
/// newer than the newest row joins at the top; a song the list never loaded (further down) is left out.
pub fn songs_now(rows: &[SongView], told: &HashMap<String, Option<SongView>>, take_new: bool) -> Vec<SongView> {
    let shown: Vec<SongView> = rows
        .iter()
        .filter_map(|r| match told.get(&r.song.id) {
            Some(Some(s)) => Some(s.clone()),
            Some(None) => None,
            None => Some(r.clone()),
        })
        .collect();
    if !take_new {
        return shown;
    }
    let newest = rows.first().map(|r| r.song.created_at.clone()).unwrap_or_default();
    let mut new: Vec<SongView> =
        told.values().flatten().filter(|s| !rows.iter().any(|r| r.song.id == s.song.id) && s.song.created_at >= newest).cloned().collect();
    new.sort_by(|a, b| b.song.created_at.cmp(&a.song.created_at));
    new.extend(shown);
    new
}

/// The engines the page loaded, each as its latest event told it.
pub fn engines_now(loaded: &[EngineStatus], told: &HashMap<String, EngineStatus>) -> Vec<EngineStatus> {
    loaded.iter().map(|e| told.get(&e.id).cloned().unwrap_or_else(|| e.clone())).collect()
}

fn connect(c: Changes) {
    // No stream to wait for: loads go ahead.
    if window().location().pathname().ok().as_deref() == Some("/sign-in") {
        return c.opened();
    }
    let Ok(source) = web_sys::EventSource::new("/api/v1/events") else { return c.opened() };
    let listen = |kind: &str, f: Box<dyn Fn(String)>| {
        let cb = Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| f(e.data().as_string().unwrap_or_default()));
        let _ = source.add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref());
        cb.forget(); // one stream for the page's life
    };
    listen(
        "job",
        Box::new(move |data| {
            if let Ok(job) = serde_json::from_str::<Job>(&data) {
                c.jobs.update(|jobs| upsert(jobs, job));
            }
        }),
    );
    listen(
        "song",
        Box::new(move |data| {
            if let Ok(e) = serde_json::from_str::<SongEvent>(&data) {
                c.songs.update(|m| drop(m.insert(e.id, e.song)));
            }
        }),
    );
    listen(
        "playlist",
        Box::new(move |data| {
            if let Ok(e) = serde_json::from_str::<PlaylistEvent>(&data)
                && let Some(state) = playlist_told(e.deleted, e.summary, e.view)
            {
                c.playlists.update(|m| drop(m.insert(e.id, state)));
            }
        }),
    );
    listen(
        "live",
        Box::new(move |data| {
            if let Ok(e) = serde_json::from_str::<LiveEvent>(&data) {
                c.live.update(|m| drop(m.insert(e.id, e.live)));
            }
        }),
    );
    listen(
        "engine",
        Box::new(move |data| {
            if let Ok(e) = serde_json::from_str::<EngineEvent>(&data) {
                c.engines.update(|m| drop(m.insert(e.id, e.engine)));
            }
        }),
    );
    listen(
        "relay",
        Box::new(move |data| {
            if let Ok(e) = serde_json::from_str::<RelayEvent>(&data) {
                c.relay.set(Some(e.relay));
            }
        }),
    );
    // Loads wait for the first open; on every reconnect, events sent while the stream was down are lost, so the page
    // loads again from scratch.
    let first = std::rc::Rc::new(std::cell::Cell::new(true));
    listen(
        "open",
        Box::new(move |_| {
            if first.replace(false) {
                c.opened();
            } else {
                c.songs.update(HashMap::clear);
                c.playlists.update(HashMap::clear);
                c.live.update(HashMap::clear);
                c.engines.update(HashMap::clear);
                c.relay.set(None);
                c.reloaded.update(|n| *n += 1);
            }
            leptos::task::spawn_local(async move {
                if let Ok(open) = api::jobs().await {
                    c.jobs.update(|jobs| {
                        jobs.retain(|j| j.state == JobState::Failed);
                        jobs.extend(open);
                    });
                }
            });
        }),
    );
    // A stream the server closed for good (the session ended): any API call then sends you to sign-in.
    let closed = source.clone();
    listen(
        "error",
        Box::new(move |_| {
            if closed.ready_state() == web_sys::EventSource::CLOSED {
                leptos::task::spawn_local(async {
                    let _ = api::jobs().await;
                });
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_playlist_event_without_its_state_leaves_the_playlist_as_it_was() {
        assert_eq!(playlist_told(true, Some(1), Some(2)), Some(None), "deleted");
        assert_eq!(playlist_told(false, Some(1), Some(2)), Some(Some((1, 2))), "its new state");
        assert_eq!(playlist_told::<u8, u8>(false, None, Some(2)), None, "the server could not read it: not deleted");
    }

    use crate::domain::{
        engine::Health,
        job::{Job, JobState},
        testing::song,
    };

    fn job(id: &str, state: JobState) -> Job {
        let mut j = Job::new(id.into(), "pads".into(), Some(30), "2026-10-19T12:00:00Z".into());
        j.state = state;
        j
    }

    fn summary(id: &str, name: &str, created_at: &str) -> PlaylistSummary {
        let live = json!({"on": false, "shuffle": false, "visibility": "public", "key": ""});
        serde_json::from_value(json!({"id": id, "name": name, "songs": 0, "seconds": 0.0, "live": live, "created_at": created_at})).unwrap()
    }

    fn view(id: &str, name: &str) -> PlaylistView {
        let live = json!({"on": false, "shuffle": false, "visibility": "public", "key": ""});
        let playlist = json!({"id": id, "name": name, "song_ids": [], "live": live, "created_at": "2026-10-19T12:00:00Z"});
        serde_json::from_value(json!({"playlist": playlist, "songs": []})).unwrap()
    }

    fn row(id: &str, created_at: &str) -> SongView {
        let mut s = song(id);
        s.created_at = created_at.into();
        SongView { song: s, playlists: vec![] }
    }

    #[test]
    fn open_jobs_are_added_or_updated_and_finished_ones_leave() {
        let mut jobs = vec![];
        upsert(&mut jobs, job("j1", JobState::Waiting));
        upsert(&mut jobs, job("j1", JobState::Making));
        assert_eq!((jobs.len(), jobs[0].state), (1, JobState::Making));
        upsert(&mut jobs, job("j1", JobState::Done));
        assert!(jobs.is_empty());
        upsert(&mut jobs, job("j2", JobState::Cancelled));
        assert!(jobs.is_empty());
    }

    #[test]
    fn failed_jobs_stay_to_show_why() {
        let mut jobs = vec![job("j1", JobState::Making)];
        let mut failed = job("j1", JobState::Failed);
        failed.error = Some("engine down".into());
        upsert(&mut jobs, failed);
        assert_eq!(jobs[0].error.as_deref(), Some("engine down"));
    }

    #[test]
    fn playlists_take_what_events_told_renamed_deleted_and_new() {
        let loaded = [summary("a", "Late", "1"), summary("b", "Gone", "2")];
        let told = HashMap::from([
            ("a".to_string(), Some((summary("a", "Later", "1"), view("a", "Later")))),
            ("b".to_string(), None),
            ("c".to_string(), Some((summary("c", "New", "3"), view("c", "New")))),
        ]);
        let names: Vec<String> = playlists_now(&loaded, &told).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Later", "New"]);
    }

    #[test]
    fn songs_take_what_events_told_and_new_ones_join_at_the_top() {
        let rows = [row("b", "2026-10-20"), row("a", "2026-10-19")];
        let mut renamed = row("a", "2026-10-19");
        renamed.song.title = "Night Bus".into();
        let told = HashMap::from([
            ("a".to_string(), Some(renamed)),
            ("b".to_string(), None),
            ("new".to_string(), Some(row("new", "2026-10-21"))),
            ("old".to_string(), Some(row("old", "2026-10-01"))),
        ]);
        let shown = |take_new| songs_now(&rows, &told, take_new).into_iter().map(|s| (s.song.id, s.song.title)).collect::<Vec<_>>();
        assert_eq!(
            shown(true),
            [("new".to_string(), song("new").title), ("a".to_string(), "Night Bus".to_string())],
            "an old song further down is not pulled up"
        );
        assert_eq!(shown(false), [("a".to_string(), "Night Bus".to_string())], "a search takes no new songs");
    }

    #[test]
    fn engines_take_their_latest_health() {
        let loaded = [EngineStatus { id: "ace-step".into(), info: None, health: Health::Up }];
        let told = HashMap::from([("ace-step".to_string(), EngineStatus { id: "ace-step".into(), info: None, health: Health::Down })]);
        assert_eq!(engines_now(&loaded, &told)[0].health, Health::Down);
    }
}
