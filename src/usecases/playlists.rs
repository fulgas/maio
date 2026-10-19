//! Playlist use cases: create, rename, delete, add, remove, reorder, list (SPEC 8).

use std::sync::{Arc, Mutex};

use crate::domain::{Error as Rule, Live, LiveChange, Playlist, Song, live};
use serde::Serialize;

use crate::usecases::{
    Error, Result,
    ports::{Clock, Ids, LiveSong, LiveStreams, OnAir, PlaylistRepository, SongRepository},
};

/// A playlist in the list: song count, length and live state (SPEC 10, `GET /playlists`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    pub songs: usize,
    pub seconds: f64,
    pub live: Live,
    pub created_at: String,
}

/// A playlist and its songs in order (SPEC 10, `GET /playlists/{id}`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaylistView {
    pub playlist: Playlist,
    pub songs: Vec<Song>,
}

/// A playlist's live state for its page (SPEC 10, `GET /playlists/{id}/live`); the web layer adds addresses.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveView {
    pub playlist: Playlist,
    pub on_air: Option<OnAir>,
    pub live_count: usize,
    pub max_live: usize,
}

pub struct Playlists {
    repo: Arc<dyn PlaylistRepository>,
    songs: Arc<dyn SongRepository>,
    ids: Arc<dyn Ids>,
    clock: Arc<dyn Clock>,
    live: Arc<dyn LiveStreams>,
    max_live: usize,
    /// Edits read, change and save a playlist; one at a time, so none is lost.
    edits: Mutex<()>,
}

impl Playlists {
    pub fn new(
        repo: Arc<dyn PlaylistRepository>,
        songs: Arc<dyn SongRepository>,
        ids: Arc<dyn Ids>,
        clock: Arc<dyn Clock>,
        live: Arc<dyn LiveStreams>,
        max_live: usize,
    ) -> Self {
        Self { repo, songs, ids, clock, live, max_live, edits: Mutex::new(()) }
    }

    // ponytail: one song lookup per playlist; fine for a personal library, a joined query if lists get slow.
    pub fn list(&self) -> Result<Vec<PlaylistSummary>> {
        self.repo
            .list()?
            .into_iter()
            .map(|p| {
                let songs = self.songs.get_many(&p.song_ids)?;
                let seconds = songs.iter().map(|s| s.seconds).sum();
                Ok(PlaylistSummary { id: p.id, name: p.name, songs: songs.len(), seconds, live: p.live, created_at: p.created_at })
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<PlaylistView> {
        let playlist = self.find(id)?;
        let songs = self.songs.get_many(&playlist.song_ids)?;
        Ok(PlaylistView { playlist, songs })
    }

    pub fn create(&self, name: &str) -> Result<Playlist> {
        let playlist = Playlist::new(self.ids.id(), name, self.ids.key(), self.clock.now())?;
        self.repo.insert(&playlist)?;
        Ok(playlist)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<Playlist> {
        self.edit(id, |p| p.rename(name))
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let _one_at_a_time = self.lock();
        self.find(id)?.ensure_deletable()?;
        self.repo.delete(id)?;
        Ok(())
    }

    /// Adds songs that exist; one unknown id refuses the whole request.
    pub fn add_songs(&self, id: &str, song_ids: &[String], position: Option<usize>) -> Result<Playlist> {
        let known = self.songs.get_many(song_ids)?;
        if song_ids.iter().any(|s| !known.iter().any(|k| &k.id == s)) {
            return Err(Rule::NotFound("song").into());
        }
        self.edit(id, |p| {
            p.add(song_ids, position);
            Ok(())
        })
    }

    pub fn remove_song(&self, id: &str, song_id: &str) -> Result<Playlist> {
        self.edit(id, |p| p.remove(song_id))
    }

    pub fn reorder(&self, id: &str, song_ids: Vec<String>) -> Result<Playlist> {
        self.edit(id, |p| p.reorder(song_ids))
    }

    /// Every playlist with its live settings, for the stream paths and the relay hook.
    pub fn all_raw(&self) -> Result<Vec<Playlist>> {
        Ok(self.repo.list()?)
    }

    /// On or off air, shuffle, public or private (SPEC 9).
    pub fn set_live(&self, id: &str, change: &LiveChange) -> Result<Playlist> {
        let _one_at_a_time = self.lock();
        let mut p = self.find(id)?;
        if change.live
            && let Some(live) = live::limit_reached(&self.repo.list()?, id, self.max_live)
        {
            return Err(Rule::LiveLimit(live).into());
        }
        p.change_live(change);
        self.repo.save(&p)?;
        if p.live.on {
            self.follow(&p)?
        } else {
            self.live.stop(&p.id)
        }
        Ok(p)
    }

    /// A new private key; the old address stops at once.
    pub fn new_key(&self, id: &str) -> Result<Playlist> {
        self.edit(id, |p| {
            p.new_key(self.ids.key());
            Ok(())
        })
    }

    pub fn live(&self, id: &str) -> Result<LiveView> {
        let playlist = self.find(id)?;
        let live_count = self.repo.list()?.iter().filter(|x| x.live.on).count();
        let on_air = if playlist.live.on { self.live.on_air(&playlist.id) } else { None };
        Ok(LiveView { playlist, on_air, live_count, max_live: self.max_live })
    }

    /// At start: every playlist that was live goes back on air.
    pub fn resume(&self) -> Result<()> {
        for p in self.repo.list()?.into_iter().filter(|p| p.live.on) {
            self.follow(&p)?;
        }
        Ok(())
    }

    /// Its songs as a live stream needs them, in playlist order.
    fn live_songs(&self, p: &Playlist) -> Result<Vec<LiveSong>> {
        Ok(self.songs.get_many(&p.song_ids)?.into_iter().map(|s| LiveSong { id: s.id, title: s.title, file: s.file, seconds: s.seconds }).collect())
    }

    /// Brings the stream in line with a saved live playlist.
    fn follow(&self, p: &Playlist) -> Result<()> {
        if p.live.on {
            self.live.play(p, &self.live_songs(p)?);
        }
        Ok(())
    }

    fn find(&self, id: &str) -> Result<Playlist> {
        self.repo.get(id)?.ok_or_else(|| Error::from(Rule::NotFound("playlist")))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.edits.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn edit(&self, id: &str, change: impl FnOnce(&mut Playlist) -> std::result::Result<(), Rule>) -> Result<Playlist> {
        let _one_at_a_time = self.lock();
        let mut playlist = self.find(id)?;
        change(&mut playlist)?;
        self.repo.save(&playlist)?;
        self.follow(&playlist)?;
        Ok(playlist)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::domain::{Error as Rule, testing::song};

    use super::*;
    use crate::usecases::testing::{FakeLive, FixedClock, MemPlaylists, MemSongs, SeqIds};

    struct World {
        playlists: Playlists,
        songs: Arc<MemSongs>,
        repo: Arc<MemPlaylists>,
        live: Arc<FakeLive>,
    }

    fn world(song_ids: &[&str]) -> World {
        let songs = Arc::new(MemSongs::default());
        for id in song_ids {
            songs.0.lock().unwrap().push(song(id));
        }
        let repo = Arc::new(MemPlaylists::default());
        let live = Arc::new(FakeLive::default());
        let playlists =
            Playlists::new(repo.clone(), songs.clone(), Arc::new(SeqIds::default()), Arc::new(FixedClock("2026-10-16T12:00:00Z")), live.clone(), 2);
        World { playlists, songs, repo, live }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn create_stores_a_new_empty_playlist() {
        let w = world(&[]);
        let p = w.playlists.create("Late").unwrap();
        assert_eq!((p.id.as_str(), p.name.as_str(), p.live.key.len()), ("id1", "Late", 32));
        assert_eq!(p.created_at, "2026-10-16T12:00:00Z");
        assert_eq!(w.repo.get("id1").unwrap(), Some(p));
        assert_eq!(w.playlists.create("").unwrap_err().rule().map(Rule::code), Some("bad_name"));
    }

    #[test]
    fn add_remove_and_reorder_are_saved() {
        let w = world(&["a", "b", "c"]);
        let id = w.playlists.create("Late").unwrap().id;
        w.playlists.add_songs(&id, &ids(&["a", "b", "a"]), None).unwrap();
        w.playlists.add_songs(&id, &ids(&["c"]), Some(0)).unwrap();
        assert_eq!(w.repo.get(&id).unwrap().unwrap().song_ids, ids(&["c", "a", "b"]));
        w.playlists.reorder(&id, ids(&["b", "c", "a"])).unwrap();
        w.playlists.remove_song(&id, "c").unwrap();
        assert_eq!(w.repo.get(&id).unwrap().unwrap().song_ids, ids(&["b", "a"]));
    }

    #[test]
    fn a_bad_order_leaves_the_stored_order_alone() {
        let w = world(&["a", "b"]);
        let id = w.playlists.create("Late").unwrap().id;
        w.playlists.add_songs(&id, &ids(&["a", "b"]), None).unwrap();
        let e = w.playlists.reorder(&id, ids(&["a", "a"])).unwrap_err();
        assert_eq!(e.rule().map(Rule::code), Some("bad_order"));
        assert_eq!(w.repo.get(&id).unwrap().unwrap().song_ids, ids(&["a", "b"]));
    }

    #[test]
    fn adding_an_unknown_song_adds_nothing() {
        let w = world(&["a"]);
        let id = w.playlists.create("Late").unwrap().id;
        let e = w.playlists.add_songs(&id, &ids(&["a", "ghost"]), None).unwrap_err();
        assert_eq!(e.rule(), Some(&Rule::NotFound("song")));
        assert!(w.repo.get(&id).unwrap().unwrap().song_ids.is_empty());
    }

    #[test]
    fn unknown_playlists_are_not_found() {
        let w = world(&["a"]);
        assert_eq!(w.playlists.get("nope").unwrap_err().rule(), Some(&Rule::NotFound("playlist")));
        assert_eq!(w.playlists.rename("nope", "x").unwrap_err().rule(), Some(&Rule::NotFound("playlist")));
        assert_eq!(w.playlists.delete("nope").unwrap_err().rule(), Some(&Rule::NotFound("playlist")));
    }

    #[test]
    fn a_live_playlist_is_not_deleted() {
        let w = world(&[]);
        let mut p = w.playlists.create("Late").unwrap();
        p.live.on = true;
        w.repo.save(&p).unwrap();
        assert_eq!(w.playlists.delete(&p.id).unwrap_err().rule(), Some(&Rule::PlaylistLive));
        p.live.on = false;
        w.repo.save(&p).unwrap();
        w.playlists.delete(&p.id).unwrap();
        assert_eq!(w.repo.get(&p.id).unwrap(), None);
    }

    #[test]
    fn list_counts_songs_and_length_and_get_returns_them_in_order() {
        let w = world(&["a", "b"]);
        w.songs.0.lock().unwrap()[1].seconds = 60.5;
        let id = w.playlists.create("Late").unwrap().id;
        w.playlists.create("Empty").unwrap();
        w.playlists.add_songs(&id, &ids(&["b", "a"]), None).unwrap();
        let list = w.playlists.list().unwrap();
        assert_eq!(list.iter().map(|p| (p.name.as_str(), p.songs, p.seconds)).collect::<Vec<_>>(), [("Late", 2, 240.5), ("Empty", 0, 0.0)]);
        let view = w.playlists.get(&id).unwrap();
        assert_eq!(view.songs.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
    }

    fn on(visibility: Option<crate::domain::Visibility>) -> crate::domain::LiveChange {
        crate::domain::LiveChange { live: true, shuffle: None, visibility }
    }

    fn last(w: &World) -> String {
        w.live.calls.lock().unwrap().last().cloned().unwrap_or_default()
    }

    #[test]
    fn going_live_starts_its_stream_with_the_songs_in_order() {
        let w = world(&["s1", "s2"]);
        let p = w.playlists.create("Late").unwrap();
        w.playlists.add_songs(&p.id, &ids(&["s1", "s2"]), None).unwrap();
        let live = w.playlists.set_live(&p.id, &on(None)).unwrap();
        assert!(live.live.on);
        assert_eq!(last(&w), format!("play {} [s1,s2] public", p.id));
    }

    #[test]
    fn going_live_past_the_limit_names_the_live_ones() {
        let w = world(&[]); // max_live = 2
        let all: Vec<String> = ["A", "B", "C"].iter().map(|n| w.playlists.create(n).unwrap().id).collect();
        for id in &all[..2] {
            w.playlists.set_live(id, &on(None)).unwrap();
        }
        match w.playlists.set_live(&all[2], &on(None)).unwrap_err().rule() {
            Some(Rule::LiveLimit(named)) => assert_eq!(named.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn off_air_stops_its_stream() {
        let w = world(&[]);
        let p = w.playlists.create("Late").unwrap();
        w.playlists.set_live(&p.id, &on(None)).unwrap();
        w.playlists.set_live(&p.id, &crate::domain::LiveChange { live: false, shuffle: None, visibility: None }).unwrap();
        assert_eq!(last(&w), format!("stop {}", p.id));
    }

    #[test]
    fn edits_to_a_live_playlist_reach_its_stream() {
        let w = world(&["s1"]);
        let p = w.playlists.create("Late").unwrap();
        w.playlists.set_live(&p.id, &on(None)).unwrap();
        w.playlists.add_songs(&p.id, &ids(&["s1"]), None).unwrap();
        assert_eq!(last(&w), format!("play {} [s1] public", p.id));
        w.playlists.remove_song(&p.id, "s1").unwrap();
        assert_eq!(last(&w), format!("play {} [] public", p.id));
    }

    #[test]
    fn a_new_key_moves_a_private_stream() {
        let w = world(&[]);
        let p = w.playlists.create("Late").unwrap();
        w.playlists.set_live(&p.id, &on(Some(crate::domain::Visibility::Private))).unwrap();
        let before = w.playlists.get(&p.id).unwrap().playlist.live.key;
        let after = w.playlists.new_key(&p.id).unwrap();
        assert_ne!(after.live.key, before);
        assert_eq!(last(&w), format!("play {} [] private", p.id));
    }

    #[test]
    fn live_playlists_resume_at_start() {
        let w = world(&[]);
        let p = w.playlists.create("Late").unwrap();
        w.playlists.create("Off").unwrap();
        w.playlists.set_live(&p.id, &on(None)).unwrap();
        w.live.calls.lock().unwrap().clear();
        w.playlists.resume().unwrap();
        assert_eq!(*w.live.calls.lock().unwrap(), [format!("play {} [] public", p.id)]);
    }

    #[test]
    fn the_live_view_counts_streams_in_use() {
        let w = world(&[]);
        let p = w.playlists.create("Late").unwrap();
        w.playlists.set_live(&p.id, &on(None)).unwrap();
        let v = w.playlists.live(&p.id).unwrap();
        assert_eq!((v.live_count, v.max_live), (1, 2));
    }
}
