use serde::{Deserialize, Serialize};

use crate::{request::Normalized, store::Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Playlist,
    Manual,
    Library,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryState {
    Generating,
    Ready,
    Playing,
    Played,
    Failed,
    Replaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Next,
    #[default]
    End,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RemoveError {
    NotFound,
    Started,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    #[serde(rename = "entry_id")]
    pub id: String,
    pub state: EntryState,
    pub source: Source,
    pub title: String,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub bpm: Option<u32>,
    pub seconds: f64,
    pub track_id: Option<String>,
    pub job_id: Option<String>,
    pub eta_s: Option<u32>,
    /// Drawn into the record label in the web UI.
    pub seed: i64,
    /// Who asked: "user:<name>" for people, "playlist" or "maio" for side B.
    #[serde(rename = "by")]
    pub token: String,
    #[serde(skip)]
    pub playlist_id: Option<String>,
    /// Decoded into the next deck: about to play, so it can no longer be moved, replaced or removed.
    #[serde(skip)]
    pub loaded: bool,
    /// Added by Maio to fill a gap (playlist or library); requests by people go ahead of fillers.
    /// Shown as `"side": "b"`; everything else is side A.
    #[serde(rename = "side", serialize_with = "side")]
    pub filler: bool,
}

fn side<S: serde::Serializer>(filler: &bool, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(if *filler { "b" } else { "a" })
}

impl Entry {
    pub fn generating(id: String, job_id: String, n: &Normalized, source: Source, token: &str, playlist_id: Option<String>, eta_s: Option<u32>) -> Self {
        Self {
            id, state: EntryState::Generating, source, title: n.title.clone(), mood: n.mood.clone(), genre: n.genre.clone(),
            bpm: n.bpm, seconds: f64::from(n.seconds), track_id: None, job_id: Some(job_id), eta_s, seed: n.seed, token: token.into(),
            playlist_id, loaded: false, filler: source == Source::Playlist,
        }
    }

    pub fn ready(id: String, t: &Track, source: Source, token: &str) -> Self {
        Self {
            id, state: EntryState::Ready, source, title: t.title.clone(), mood: t.mood.clone(), genre: t.genre.clone(),
            bpm: t.bpm, seconds: t.seconds, track_id: Some(t.id.clone()), job_id: None, eta_s: None, seed: t.seed, token: token.into(),
            playlist_id: None, loaded: false, filler: false,
        }
    }

    fn pending(&self) -> bool {
        matches!(self.state, EntryState::Generating | EntryState::Ready) && !self.loaded
    }
}

/// Play order and history of recent entries. ponytail: in memory only; a restart starts with an empty queue.
#[derive(Default)]
pub struct Queue {
    pub entries: Vec<Entry>,
}

impl Queue {
    pub fn add(&mut self, e: Entry, pos: Position) {
        let idx = match pos {
            Position::Next => self.entries.iter().position(Entry::pending),
            Position::End if e.filler => None,
            Position::End => self.entries.iter().position(|x| x.pending() && x.filler),
        };
        self.entries.insert(idx.unwrap_or(self.entries.len()), e);
        self.prune();
    }

    /// Entries still to play, in order, including one already loaded into the next deck.
    pub fn upcoming(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| matches!(e.state, EntryState::Generating | EntryState::Ready))
    }

    pub fn current(&self) -> Option<&Entry> {
        self.entries.iter().find(|e| e.state == EntryState::Playing)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Marks matching pending entries `replaced`; returns the jobs to cancel.
    pub fn replace(&mut self, pred: impl Fn(&Entry) -> bool) -> Vec<String> {
        let mut jobs = Vec::new();
        for e in self.entries.iter_mut().filter(|e| e.pending() && pred(e)) {
            if e.state == EntryState::Generating {
                jobs.extend(e.job_id.clone());
            }
            e.state = EntryState::Replaced;
        }
        jobs
    }

    /// Removes a pending entry; returns its job to cancel, if it was generating.
    pub fn remove(&mut self, id: &str) -> Result<Option<String>, RemoveError> {
        let idx = self.entries.iter().position(|e| e.id == id).ok_or(RemoveError::NotFound)?;
        if !self.entries[idx].pending() {
            return Err(RemoveError::Started);
        }
        let e = self.entries.remove(idx);
        Ok(if e.state == EntryState::Generating { e.job_id } else { None })
    }

    pub fn next_to_load(&mut self) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.pending())
    }

    pub fn pending_count(&self) -> usize {
        self.entries.iter().filter(|e| e.pending()).count()
    }

    /// Tracks that are queued or playing: never picked twice, never swept by retention.
    pub fn track_ids(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| matches!(e.state, EntryState::Generating | EntryState::Ready | EntryState::Playing))
            .filter_map(|e| e.track_id.clone())
            .collect()
    }

    /// Keeps live entries and the last 20 finished ones.
    pub fn prune(&mut self) {
        let finished = |e: &Entry| matches!(e.state, EntryState::Played | EntryState::Failed | EntryState::Replaced);
        let mut extra = self.entries.iter().filter(|e| finished(e)).count().saturating_sub(20);
        self.entries.retain(|e| {
            if extra > 0 && finished(e) {
                extra -= 1;
                false
            } else {
                true
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: &str, source: Source, state: EntryState) -> Entry {
        Entry {
            id: id.into(), state, source, title: id.into(), mood: None, genre: None, bpm: None, seconds: 180.0,
            track_id: Some(format!("trk_{id}")), job_id: Some(format!("job_{id}")), eta_s: None, seed: 1,
            token: "me".into(), playlist_id: None, loaded: false, filler: source == Source::Playlist,
        }
    }
    fn ids(q: &Queue) -> Vec<String> {
        q.upcoming().map(|e| e.id.clone()).collect()
    }

    #[test]
    fn next_goes_before_everything_pending() {
        let mut q = Queue::default();
        q.add(e("playing", Source::Manual, EntryState::Playing), Position::End);
        q.add(e("a", Source::Manual, EntryState::Ready), Position::End);
        q.add(e("b", Source::Manual, EntryState::Generating), Position::Next);
        assert_eq!(ids(&q), ["b", "a"]);
    }

    #[test]
    fn end_requests_jump_ahead_of_side_b() {
        let mut q = Queue::default();
        q.add(e("s1", Source::Playlist, EntryState::Ready), Position::End);
        q.add(e("s2", Source::Playlist, EntryState::Generating), Position::End);
        q.add(e("m", Source::Manual, EntryState::Ready), Position::End);
        assert_eq!(ids(&q), ["m", "s1", "s2"]);
    }

    #[test]
    fn loaded_entries_are_not_moved_replaced_or_removed() {
        let mut q = Queue::default();
        let mut loaded = e("loaded", Source::Manual, EntryState::Ready);
        loaded.loaded = true;
        q.add(loaded, Position::End);
        q.add(e("p", Source::Manual, EntryState::Generating), Position::Next);
        assert_eq!(ids(&q), ["loaded", "p"]);
        let cancelled = q.replace(|e| e.source == Source::Manual);
        assert_eq!(cancelled, ["job_p"]);
        assert_eq!(q.get_mut("loaded").unwrap().state, EntryState::Ready);
        assert!(matches!(q.remove("loaded"), Err(RemoveError::Started)));
    }

    #[test]
    fn replace_returns_jobs_of_generating_entries_only() {
        let mut q = Queue::default();
        q.add(e("s1", Source::Playlist, EntryState::Ready), Position::End);
        q.add(e("s2", Source::Playlist, EntryState::Generating), Position::End);
        q.add(e("m", Source::Manual, EntryState::Generating), Position::End);
        let jobs = q.replace(|e| e.source == Source::Playlist);
        assert_eq!(jobs, ["job_s2"]);
        assert_eq!(ids(&q), ["m"]);
    }

    #[test]
    fn adding_keeps_finished_entries_bounded() {
        let mut q = Queue::default();
        for i in 0..100 {
            q.add(e(&format!("f{i}"), Source::Playlist, EntryState::Failed), Position::End);
        }
        assert!(q.entries.len() <= 21, "got {}", q.entries.len());
    }

    #[test]
    fn remove_rules() {
        let mut q = Queue::default();
        q.add(e("playing", Source::Manual, EntryState::Playing), Position::End);
        q.add(e("g", Source::Manual, EntryState::Generating), Position::End);
        assert_eq!(q.remove("g").unwrap(), Some("job_g".to_string()));
        assert!(matches!(q.remove("playing"), Err(RemoveError::Started)));
        assert!(matches!(q.remove("nope"), Err(RemoveError::NotFound)));
    }

    #[test]
    fn counts_track_ids_and_pruning() {
        let mut q = Queue::default();
        q.add(e("playing", Source::Manual, EntryState::Playing), Position::End);
        q.add(e("r", Source::Manual, EntryState::Ready), Position::End);
        for i in 0..30 {
            q.add(e(&format!("old{i}"), Source::Manual, EntryState::Played), Position::End);
        }
        assert_eq!(q.pending_count(), 1);
        assert_eq!(q.current().unwrap().id, "playing");
        let mut t = q.track_ids();
        t.sort();
        assert_eq!(t, ["trk_playing", "trk_r"]);
        q.prune();
        assert_eq!(q.entries.len(), 2 + 20);
        assert_eq!(q.next_to_load().unwrap().id, "r");
    }
}
