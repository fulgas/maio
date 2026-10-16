use std::{collections::{HashSet, VecDeque}, f32::consts::FRAC_PI_2, path::Path, sync::Arc, time::Duration};

use bytes::Bytes;
use rand::seq::SliceRandom;
use tokio::sync::oneshot;

use crate::{
    config::Scope,
    engine, jobs, media,
    playlists::{self, Kind, Order, Playlist, Recipe},
    queue::{Entry, EntryState, Position, Source},
    radio::Radio,
    request::{Rules, normalize},
    state::{AppState, NowPlaying, TrackBrief},
    store::{self, Track},
};

pub const CHANNELS: usize = 2;
pub const SAMPLE_RATE: usize = 48_000;
/// 20 ms of interleaved stereo samples.
pub const CHUNK: usize = SAMPLE_RATE / 50 * CHANNELS;

/// A decoded track and the play position within it, in interleaved samples.
#[derive(Clone)]
pub struct Deck {
    pub entry_id: String,
    pub track_id: String,
    pub samples: Arc<Vec<i16>>,
    pub pos: usize,
}

impl Deck {
    pub fn new(entry_id: String, track_id: String, mut samples: Vec<i16>) -> Self {
        samples.truncate(samples.len() / CHANNELS * CHANNELS); // whole frames only, so channels never swap
        Self { entry_id, track_id, samples: Arc::new(samples), pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.samples.len() - self.pos
    }

    fn take(&mut self) -> f32 {
        match self.samples.get(self.pos) {
            Some(&s) => {
                self.pos += 1;
                f32::from(s)
            }
            None => 0.0,
        }
    }

    pub fn remaining_s(&self) -> f32 {
        self.remaining() as f32 / (SAMPLE_RATE * CHANNELS) as f32
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MixEvent {
    Started { entry_id: String, track_id: String },
    Ended { entry_id: String, track_id: String, skipped: bool },
}

/// Two decks and an equal-power crossfade (SPEC 6). Pure: no clock, no I/O.
pub struct Mixer {
    pub cur: Option<Deck>,
    pub next: Option<Deck>,
    fade_len: usize,
    fade_pos: Option<usize>,
    skipping: bool,
}

impl Mixer {
    pub fn new(fade_s: f32) -> Self {
        let frames = (fade_s.max(0.001) * SAMPLE_RATE as f32) as usize;
        Self { cur: None, next: None, fade_len: frames * CHANNELS, fade_pos: None, skipping: false }
    }

    pub fn fade_s(&self) -> f32 {
        self.fade_len as f32 / (SAMPLE_RATE * CHANNELS) as f32
    }

    /// Seconds left of the current track; 0 when nothing plays.
    pub fn remaining_s(&self) -> f32 {
        self.cur.as_ref().map_or(0.0, Deck::remaining_s)
    }

    /// Starts the crossfade now. With nothing loaded next, the track fades to silence.
    pub fn skip(&mut self) {
        if self.cur.is_some() && self.fade_pos.is_none() {
            self.fade_pos = Some(0);
            self.skipping = true;
        }
    }

    pub fn render(&mut self, out: &mut [i16], events: &mut Vec<MixEvent>) {
        for o in out.iter_mut() {
            if self.cur.is_none() {
                match self.next.take() {
                    Some(n) => {
                        events.push(MixEvent::Started { entry_id: n.entry_id.clone(), track_id: n.track_id.clone() });
                        self.cur = Some(n);
                    }
                    None => {
                        *o = 0;
                        continue;
                    }
                }
            }
            let next_ready = self.next.is_some();
            let cur = self.cur.as_mut().expect("cur set above");
            if self.fade_pos.is_none() && next_ready && cur.remaining() <= self.fade_len {
                self.fade_pos = Some(0);
            }
            let v = match self.fade_pos {
                None => cur.take(),
                Some(p) => {
                    let t = p as f32 / self.fade_len as f32 * FRAC_PI_2;
                    let a = cur.take() * t.cos();
                    let b = self.next.as_mut().map_or(0.0, Deck::take) * t.sin();
                    self.fade_pos = Some(p + 1);
                    a + b
                }
            };
            *o = v.round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
            let fade_done = self.fade_pos.is_some_and(|p| p >= self.fade_len);
            let ended = fade_done || (self.fade_pos.is_none() && cur.remaining() == 0);
            if ended {
                let old = self.cur.take().expect("cur set above");
                events.push(MixEvent::Ended { entry_id: old.entry_id, track_id: old.track_id, skipped: self.skipping });
                self.skipping = false;
                if self.fade_pos.take().is_some() {
                    if let Some(n) = self.next.take() {
                        events.push(MixEvent::Started { entry_id: n.entry_id.clone(), track_id: n.track_id.clone() });
                        self.cur = Some(n);
                    }
                }
            }
        }
    }
}

/// Start decoding the next track this long before the current one starts fading.
pub const LOAD_AHEAD_S: f32 = 15.0;
/// With less than this left before the fade, a still-generating entry is late: play from the library.
pub const LATE_S: f32 = 3.0;

/// The playout loop: one 20 ms chunk per tick, paced against a fixed start so it never drifts.
pub async fn run(state: AppState, radio: Arc<Radio>) {
    let mut mixer = Mixer::new(state.cfg.crossfade_s);
    let mut buf = vec![0i16; CHUNK];
    let mut events = Vec::new();
    let mut loading: Option<oneshot::Receiver<Option<Deck>>> = None;
    let mut play_row: Option<i64> = None;
    let mut next_try = tokio::time::Instant::now();
    // Skip pressed before the next track was decoded: load it now and crossfade when it arrives (SPEC 6).
    let mut pending_skip = false;
    let start = tokio::time::Instant::now();
    let mut n: u64 = 0;
    loop {
        if radio.is_closed() {
            return;
        }
        if n % 50 == 0 {
            fill(&state, &radio);
        }
        let (stopped, skip) = {
            let mut p = radio.playout.lock().unwrap();
            (p.stopped, std::mem::take(&mut p.skip))
        };
        if stopped {
            if let Some(d) = mixer.next.take() {
                unload(&radio, &d.entry_id);
            }
            mixer.skip();
            pending_skip = false;
        } else {
            let mut now = tokio::time::Instant::now();
            if skip {
                if mixer.next.is_some() {
                    mixer.skip();
                } else {
                    pending_skip = true;
                    next_try = now;
                }
            }
            if mixer.next.is_none() && loading.is_none() && now >= next_try
                && (pending_skip || mixer.remaining_s() <= mixer.fade_s() + LOAD_AHEAD_S)
            {
                let time_left = if pending_skip { 0.0 } else { mixer.remaining_s() - mixer.fade_s() };
                loading = load_next(&state, &radio, time_left);
                if loading.is_none() {
                    now = tokio::time::Instant::now();
                    next_try = now + Duration::from_secs(1); // nothing to play: retry once a second, not every tick
                    if std::mem::take(&mut pending_skip) {
                        mixer.skip(); // nothing to skip to: fade out
                    }
                }
            }
        }
        if let Some(rx) = loading.as_mut() {
            match rx.try_recv() {
                Ok(deck) => {
                    loading = None;
                    match deck {
                        Some(d) if stopped => unload(&radio, &d.entry_id),
                        Some(d) => {
                            mixer.next = Some(d);
                            if std::mem::take(&mut pending_skip) {
                                mixer.skip();
                            }
                        }
                        None => {
                            next_try = tokio::time::Instant::now() + Duration::from_secs(1);
                            if std::mem::take(&mut pending_skip) {
                                mixer.skip();
                            }
                        }
                    }
                }
                Err(oneshot::error::TryRecvError::Empty) => {}
                Err(oneshot::error::TryRecvError::Closed) => loading = None,
            }
        }
        mixer.render(&mut buf, &mut events);
        for e in events.drain(..) {
            on_event(&state, &radio, e, &mut play_row);
        }
        let mut bytes = Vec::with_capacity(CHUNK * 2);
        for s in &buf {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let _ = radio.pcm.send(Bytes::from(bytes));
        n += 1;
        tokio::time::sleep_until(start + Duration::from_millis(20 * n)).await;
    }
}

fn set_entry_state(r: &Radio, entry_id: &str, s: EntryState) {
    if let Some(e) = r.queue.lock().unwrap().get_mut(entry_id) {
        e.state = s;
    }
}

fn mark_failed(state: &AppState, r: &Radio, entry_id: &str) {
    set_entry_state(r, entry_id, EntryState::Failed);
    state.emit_queue(r);
}

fn unload(r: &Radio, entry_id: &str) {
    if let Some(e) = r.queue.lock().unwrap().get_mut(entry_id) {
        e.loaded = false;
    }
}

fn on_event(state: &AppState, r: &Radio, e: MixEvent, play_row: &mut Option<i64>) {
    match e {
        MixEvent::Started { entry_id, track_id } => {
            let source = {
                let mut q = r.queue.lock().unwrap();
                q.get_mut(&entry_id).map(|e| {
                    e.state = EntryState::Playing;
                    e.source
                })
            };
            if let Ok(Some(t)) = state.store.track(&track_id) {
                *play_row = state.store.play_started(&t.id, &t.title, &r.id).ok();
                let now = NowPlaying {
                    entry_id,
                    track: TrackBrief::from(&t),
                    source: source.unwrap_or(Source::Library),
                    started_at: store::now(),
                    started: std::time::Instant::now(),
                };
                state.emit(Some(&r.id), "now_playing", now.json());
                r.playout.lock().unwrap().now = Some(now);
            }
            state.emit_queue(r);
        }
        MixEvent::Ended { entry_id, skipped, .. } => {
            {
                let mut q = r.queue.lock().unwrap();
                if let Some(e) = q.get_mut(&entry_id) {
                    e.state = EntryState::Played;
                }
                q.prune();
            }
            if let Some(row) = play_row.take() {
                let _ = state.store.play_ended(row, skipped);
            }
            let cleared = {
                let mut p = r.playout.lock().unwrap();
                let was_current = p.now.as_ref().is_some_and(|n| n.entry_id == entry_id);
                if was_current {
                    p.now = None;
                }
                was_current
            };
            if cleared {
                state.emit(Some(&r.id), "now_playing", serde_json::Value::Null);
            }
            state.emit_queue(r);
        }
    }
}

/// Picks what plays next and decodes it in the background.
pub fn load_next(state: &AppState, r: &Arc<Radio>, time_left_s: f32) -> Option<oneshot::Receiver<Option<Deck>>> {
    let (entry_id, track) = pick_next(state, r, time_left_s)?;
    let (tx, rx) = oneshot::channel();
    let st = state.clone();
    let radio = r.clone();
    tokio::spawn(async move {
        match media::decode(Path::new(&track.file)).await {
            Ok(samples) => {
                let _ = tx.send(Some(Deck::new(entry_id, track.id, samples)));
            }
            Err(e) => {
                tracing::warn!("cannot decode {}: {e:#}", track.file);
                if !Path::new(&track.file).exists() {
                    let _ = st.store.mark_deleted(&track.id); // gone from disk: never pick it again
                }
                mark_failed(&st, &radio, &entry_id);
                let _ = tx.send(None);
            }
        }
    });
    Some(rx)
}

/// The next entry to play (SPEC 6): the head of the queue when ready; when the head is still
/// generating and time is short, a library track with the same mood; with an empty queue, any library track.
pub fn pick_next(state: &AppState, r: &Radio, time_left_s: f32) -> Option<(String, Track)> {
    let mood = {
        let mut q = r.queue.lock().unwrap();
        match q.next_to_load() {
            Some(e) if e.state == EntryState::Ready => {
                e.loaded = true;
                let (id, track_id) = (e.id.clone(), e.track_id.clone());
                drop(q);
                return match track_id.and_then(|t| state.store.track(&t).ok().flatten()) {
                    Some(t) => Some((id, t)),
                    None => {
                        mark_failed(state, r, &id);
                        None
                    }
                };
            }
            Some(_) if time_left_s > LATE_S => return None,
            Some(e) => e.mood.clone(),
            // Nothing queued: silence. Only a playlist on air keeps the stream going (SPEC 10).
            None => return None,
        }
    };
    let exclude = state.busy_track_ids();
    // A late entry wants its own mood (SPEC 6); only an entry without one prefers the playlist on air.
    let playlist = if mood.is_some() { None } else { state.store.on_air(&r.id).ok().flatten() };
    let track = state.store
        .pick(&r.id, playlist.as_deref(), mood.as_deref(), state.cfg.no_repeat, &exclude)
        .ok()
        .flatten()?;
    let mut entry = Entry::ready(jobs::new_id("q"), &track, Source::Library, "maio");
    entry.loaded = true;
    entry.filler = true;
    let id = entry.id.clone();
    r.queue.lock().unwrap().add(entry, Position::Next);
    state.emit_queue(r);
    Some((id, track))
}

/// The play order drawn for the playlist on air: popped from the front, drawn again when empty.
#[derive(Debug, Default)]
pub struct PlayOrder {
    pub playlist: Option<String>,
    pub order: VecDeque<String>,
}

/// The playlist on air on `r`; `None` when nothing is, or when it no longer exists.
pub fn on_air(state: &AppState, r: &Radio) -> Option<Playlist> {
    let id = state.store.on_air(&r.id).ok().flatten()?;
    playlists::find(&state.store, &id).ok().flatten()
}

/// Keeps `lookahead` entries ready or generating from the playlist on air (SPEC 10, On air).
/// With nothing on air, nothing is added: idle means silence.
pub fn fill(state: &AppState, r: &Radio) {
    let pending = r.queue.lock().unwrap().pending_count();
    if pending >= state.cfg.lookahead {
        return;
    }
    let Some(pl) = on_air(state, r) else { return };
    let mut added = false;
    for _ in pending..state.cfg.lookahead {
        if !fill_from_playlist(state, r, &pl) {
            break;
        }
        added = true;
    }
    if added {
        state.emit_queue(r);
    }
}

fn fill_from_playlist(state: &AppState, r: &Radio, pl: &Playlist) -> bool {
    let recipe = match (&pl.kind, &pl.recipe) {
        (Kind::Generative, Some(recipe)) => recipe,
        _ => return queue_next(state, r, pl),
    };
    // Replays: the playlist's own tracks, else (nothing of its own yet) the library by mood and favourites.
    let replay = || queue_next(state, r, pl) || fill_from_library(state, r, &pl.id, recipe.mood.as_deref());
    let generating = r.queue.lock().unwrap().upcoming()
        .filter(|e| e.state == EntryState::Generating && e.playlist_id.as_deref() == Some(pl.id.as_str()))
        .count() as u32;
    let size = state.store.member_count(pl, None).unwrap_or(0) + generating;
    if pl.limit(state.cfg.playlists.max_tracks).is_some_and(|max| size >= max) {
        return replay(); // at its size it loops what it has (SPEC 10)
    }
    if rand::random::<f32>() < recipe.library_share && replay() {
        return true;
    }
    generate(state, r, pl, recipe) || replay()
}

/// Starts a new track for a generative playlist; false when it cannot (invalid recipe, full crate,
/// backing off, or no healthy engine).
fn generate(state: &AppState, r: &Radio, pl: &Playlist, recipe: &Recipe) -> bool {
    let scopes: HashSet<Scope> = [Scope::Queue].into();
    let ids = state.cfg.text_engine_ids();
    let rules = Rules { moods: &state.cfg.moods, genres: &state.cfg.genres, scopes: &scopes, playlist_name: Some(&pl.name),
                        text_engines: &ids, default_language: &state.cfg.lyrics.default_language };
    let n = match normalize(&recipe.next_request(), &rules) {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(playlist = %pl.id, "recipe request invalid: {}", e.message);
            return false;
        }
    };
    let full = !crate::storage::make_room(state, crate::storage::estimate_bytes(state, n.seconds), &[]);
    if full || r.backoff.lock().unwrap().active() || engine::choose(&state.engines, &n, &[]).is_err() {
        return false; // no healthy engine: no point queueing a job that fails at once
    }
    let (entry_id, job_id) = (jobs::new_id("q"), jobs::new_id("job"));
    let eta = jobs::eta_s(state, &n);
    let entry = Entry::generating(entry_id.clone(), job_id.clone(), &n, Source::Playlist, "playlist", Some(pl.id.clone()), eta);
    r.queue.lock().unwrap().add(entry, Position::End);
    jobs::start(state, job_id, n, "playlist", Some(&r.id), Some(entry_id), Some(pl.id.clone()));
    true
}

/// Queues the playlist's next track on side B; false when it has none to play.
fn queue_next(state: &AppState, r: &Radio, pl: &Playlist) -> bool {
    let Some(t) = next_track(state, r, pl) else { return false };
    let mut e = Entry::ready(jobs::new_id("q"), &t, Source::Playlist, "maio");
    e.filler = true;
    e.playlist_id = Some(pl.id.clone());
    r.queue.lock().unwrap().add(e, Position::End);
    true
}

fn fill_from_library(state: &AppState, r: &Radio, playlist_id: &str, mood: Option<&str>) -> bool {
    let exclude = state.busy_track_ids();
    match state.store.pick(&r.id, Some(playlist_id), mood, state.cfg.no_repeat, &exclude) {
        Ok(Some(t)) => {
            let mut e = Entry::ready(jobs::new_id("q"), &t, Source::Playlist, "maio");
            e.filler = true;
            e.playlist_id = Some(playlist_id.to_string());
            r.queue.lock().unwrap().add(e, Position::End);
            true
        }
        _ => false,
    }
}

/// Draws a play order: the playlist's own, or shuffled with recent plays moved to the end.
fn draw(order: Order, members: &[String], recent: &[String]) -> VecDeque<String> {
    let mut d = members.to_vec();
    if order == Order::Shuffle {
        d.shuffle(&mut rand::rng());
        d.sort_by_key(|id| recent.contains(id)); // stable, so the shuffle survives within each half
    }
    d.into()
}

/// The playlist's next track on `r` (SPEC 10, On air): from the play order, in the playlist's order or
/// shuffled, looping, never one already queued here, and not within the last `no_repeat` plays
/// unless the playlist is too short; then the least recently played one.
pub fn next_track(state: &AppState, r: &Radio, pl: &Playlist) -> Option<Track> {
    let members = state.store.member_ids(pl, pl.cap(state.cfg.playlists.max_tracks)).ok()?;
    if members.is_empty() {
        return None;
    }
    let queued: HashSet<String> = r.queue.lock().unwrap().upcoming().filter_map(|e| e.track_id.clone()).collect();
    let recent = state.store.recent_track_ids(&r.id, state.cfg.no_repeat).unwrap_or_default();
    let live: HashSet<&String> = members.iter().collect();
    let mut po = r.play_order.lock().unwrap();
    if po.playlist.as_deref() != Some(pl.id.as_str()) {
        *po = PlayOrder { playlist: Some(pl.id.clone()), order: VecDeque::new() };
    }
    for redraw in [false, true] {
        if redraw {
            po.order = draw(pl.order, &members, &recent);
        }
        while let Some(id) = po.order.pop_front() {
            if live.contains(&id) && !queued.contains(&id) && !recent.contains(&id) {
                if let Some(t) = state.store.track(&id).ok().flatten() {
                    return Some(t);
                }
            }
        }
    }
    let rank = |id: &&String| std::cmp::Reverse(recent.iter().position(|x| x == *id).unwrap_or(usize::MAX));
    let id = members.iter().filter(|id| !queued.contains(*id)).min_by_key(rank)?;
    state.store.track(id).ok().flatten()
}

/// The next `n` tracks the play order holds, drawing it now if needed so the order shown is the one that plays.
pub fn upcoming(state: &AppState, r: &Radio, pl: &Playlist, n: usize) -> Vec<String> {
    let members = state.store.member_ids(pl, pl.cap(state.cfg.playlists.max_tracks)).unwrap_or_default();
    let live: HashSet<&String> = members.iter().collect();
    let mut po = r.play_order.lock().unwrap();
    if po.playlist.as_deref() != Some(pl.id.as_str()) || !po.order.iter().any(|id| live.contains(id)) {
        let recent = state.store.recent_track_ids(&r.id, state.cfg.no_repeat).unwrap_or_default();
        *po = PlayOrder { playlist: Some(pl.id.clone()), order: draw(pl.order, &members, &recent) };
    }
    po.order.iter().filter(|id| live.contains(id)).take(n).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(id: &str, value: i16, frames: usize) -> Deck {
        Deck::new(id.into(), format!("t{id}"), vec![value; frames * CHANNELS])
    }
    fn started(id: &str) -> MixEvent {
        MixEvent::Started { entry_id: id.into(), track_id: format!("t{id}") }
    }
    fn ended(id: &str, skipped: bool) -> MixEvent {
        MixEvent::Ended { entry_id: id.into(), track_id: format!("t{id}"), skipped }
    }

    #[test]
    fn plays_a_track_then_silence() {
        let mut m = Mixer::new(0.001); // 48 frames = 96 samples
        m.next = Some(deck("a", 1000, 100));
        let mut out = vec![1i16; 300];
        let mut ev = Vec::new();
        m.render(&mut out, &mut ev);
        assert!(out[..200].iter().all(|&s| s == 1000));
        assert!(out[200..].iter().all(|&s| s == 0));
        assert_eq!(ev, [started("a"), ended("a", false)]);
    }

    #[test]
    fn crossfades_with_equal_power() {
        let mut m = Mixer::new(0.01); // 480 frames = 960 samples
        let mut ev = Vec::new();
        m.next = Some(deck("a", 10000, 1000));
        let mut first = vec![0i16; 2];
        m.render(&mut first, &mut ev);
        m.next = Some(deck("b", 10000, 1000));
        let mut out = vec![0i16; 3998];
        m.render(&mut out, &mut ev);
        let at = |i: usize| if i < 2 { first[i] } else { out[i - 2] };
        assert_eq!(at(1039), 10000); // just before the fade
        assert_eq!(at(1520), 14142); // fade midpoint: (cos + sin)(pi/4) * 10000
        assert_eq!(at(3039), 10000); // last sample of b
        assert_eq!(at(3040), 0);
        assert_eq!(ev, [started("a"), ended("a", false), started("b"), ended("b", false)]);
    }

    #[test]
    fn skip_without_next_fades_to_silence() {
        let mut m = Mixer::new(0.001);
        let mut ev = Vec::new();
        m.next = Some(deck("a", 10000, 1000));
        m.render(&mut vec![0i16; 10], &mut ev);
        m.skip();
        let mut out = vec![0i16; 200];
        m.render(&mut out, &mut ev);
        assert_eq!(out[0], 10000);
        assert!(out[50] < out[10] && out[95] < 500);
        assert!(out[96..].iter().all(|&s| s == 0));
        assert_eq!(ev.last(), Some(&ended("a", true)));
        assert!(m.cur.is_none());
    }

    #[test]
    fn late_next_fades_in_from_silence() {
        let mut m = Mixer::new(0.001); // 96 samples
        let mut ev = Vec::new();
        m.next = Some(deck("a", 10000, 100)); // 200 samples
        m.render(&mut vec![0i16; 150], &mut ev); // 50 samples left, no next yet
        m.next = Some(deck("b", 10000, 1000));
        let mut out = vec![0i16; 300];
        m.render(&mut out, &mut ev);
        assert_eq!(ev, [started("a"), ended("a", false), started("b")]);
        assert_eq!(out[200], 10000, "b at full level after the fade");
    }

    #[test]
    fn remaining_seconds() {
        let mut m = Mixer::new(1.0);
        assert_eq!(m.remaining_s(), 0.0);
        m.next = Some(deck("a", 1, SAMPLE_RATE * 3));
        m.render(&mut vec![0i16; CHUNK], &mut Vec::new());
        assert!((m.remaining_s() - 2.98).abs() < 0.001);
        assert!((m.fade_s() - 1.0).abs() < 0.001);
    }
}
