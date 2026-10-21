//! Live changes from `/api/v1/events` (arc42 3.5): jobs as they move, and a nudge to reload when songs or playlists change.

use leptos::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use crate::{
    domain::job::{Job, JobState},
    ui::api,
};

#[derive(Clone, Copy)]
pub struct Changes {
    /// Open jobs, plus failed ones until dismissed.
    pub jobs: RwSignal<Vec<Job>>,
    /// Bumped when songs change; resources that list songs track it.
    pub songs: RwSignal<u32>,
    pub playlists: RwSignal<u32>,
    /// Bumped when a playlist's live state changes.
    pub live: RwSignal<u32>,
}

pub fn provide() -> Changes {
    let c = Changes { jobs: RwSignal::new(vec![]), songs: RwSignal::new(0), playlists: RwSignal::new(0), live: RwSignal::new(0) };
    provide_context(c);
    Effect::new(move |_| connect(c)); // effects run only in the browser, once here
    c
}

/// Puts a job's new state in the list; returns true when it just finished a song.
pub fn upsert(jobs: &mut Vec<Job>, job: Job) -> bool {
    let at = jobs.iter().position(|j| j.id == job.id);
    match job.state {
        JobState::Done | JobState::Cancelled => {
            if let Some(i) = at {
                jobs.remove(i);
            }
            job.state == JobState::Done
        }
        _ => {
            match at {
                Some(i) => jobs[i] = job,
                None => jobs.push(job),
            }
            false
        }
    }
}

fn connect(c: Changes) {
    if window().location().pathname().ok().as_deref() == Some("/sign-in") {
        return;
    }
    let Ok(source) = web_sys::EventSource::new("/api/v1/events") else { return };
    let listen = |kind: &str, f: Box<dyn Fn(String)>| {
        let cb = Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| f(e.data().as_string().unwrap_or_default()));
        let _ = source.add_event_listener_with_callback(kind, cb.as_ref().unchecked_ref());
        cb.forget(); // one stream for the page's life
    };
    listen(
        "job",
        Box::new(move |data| {
            if let Ok(job) = serde_json::from_str::<Job>(&data)
                && c.jobs.try_update(|jobs| upsert(jobs, job)).unwrap_or(false)
            {
                c.songs.update(|n| *n += 1);
            }
        }),
    );
    listen("song", Box::new(move |_| c.songs.update(|n| *n += 1)));
    listen("playlist", Box::new(move |_| c.playlists.update(|n| *n += 1)));
    listen("live", Box::new(move |_| c.live.update(|n| *n += 1)));
    // On first connect and every reconnect: reload what may have been missed.
    let first = std::rc::Rc::new(std::cell::Cell::new(true));
    listen(
        "open",
        Box::new(move |_| {
            // The page just loaded what it shows; only a reconnect may have missed changes.
            if !first.replace(false) {
                c.songs.update(|n| *n += 1);
                c.playlists.update(|n| *n += 1);
                c.live.update(|n| *n += 1);
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
    use super::*;
    use crate::domain::job::{Job, JobState};

    fn job(id: &str, state: JobState) -> Job {
        let mut j = Job::new(id.into(), "pads".into(), Some(30), "2026-10-19T12:00:00Z".into());
        j.state = state;
        j
    }

    #[test]
    fn open_jobs_are_added_or_updated_and_finished_ones_leave() {
        let mut jobs = vec![];
        assert!(!upsert(&mut jobs, job("j1", JobState::Waiting)));
        assert!(!upsert(&mut jobs, job("j1", JobState::Making)));
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].state, JobState::Making);
        assert!(upsert(&mut jobs, job("j1", JobState::Done)), "a done job made a song");
        assert!(jobs.is_empty());
        assert!(!upsert(&mut jobs, job("j2", JobState::Cancelled)));
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
}
