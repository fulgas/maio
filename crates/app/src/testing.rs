//! In-memory ports for tests, here and in the web layer.

use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::Result;
use domain::{Playlist, PlaylistRef, Song};

use crate::ports::{AudioFiles, Clock, Ids, PlaylistRepository, SongQuery, SongRepository, SongSort};

#[derive(Default)]
pub struct MemSongs(pub Mutex<Vec<Song>>);

impl SongRepository for MemSongs {
    fn insert(&self, song: &Song) -> Result<()> {
        self.0.lock().unwrap().push(song.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Song>> {
        Ok(self.0.lock().unwrap().iter().find(|s| s.id == id).cloned())
    }

    fn get_many(&self, ids: &[String]) -> Result<Vec<Song>> {
        let all = self.0.lock().unwrap();
        Ok(ids.iter().filter_map(|id| all.iter().find(|s| &s.id == id).cloned()).collect())
    }

    fn update(&self, song: &Song) -> Result<()> {
        let mut all = self.0.lock().unwrap();
        if let Some(s) = all.iter_mut().find(|s| s.id == song.id) {
            *s = song.clone();
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().retain(|s| s.id != id);
        Ok(())
    }

    fn search(&self, query: &SongQuery) -> Result<Vec<Song>> {
        let q = query.q.as_deref().map(str::to_lowercase);
        let hit = |s: &Song| match &q {
            None => true,
            Some(q) => [Some(&s.title), Some(&s.prompt), s.mood.as_ref(), s.genre.as_ref(), s.lyrics.as_ref()]
                .into_iter()
                .flatten()
                .any(|f| f.to_lowercase().contains(q)),
        };
        let mut found: Vec<Song> = self.0.lock().unwrap().iter().filter(|s| hit(s)).cloned().collect();
        match query.sort {
            SongSort::Newest => found.sort_by(|a, b| (&b.created_at, &b.id).cmp(&(&a.created_at, &a.id))),
            SongSort::Title => found.sort_by(|a, b| (a.title.to_lowercase(), &a.id).cmp(&(b.title.to_lowercase(), &b.id))),
        }
        Ok(found.into_iter().skip(query.offset).take(query.limit).collect())
    }
}

#[derive(Default)]
pub struct MemPlaylists(pub Mutex<Vec<Playlist>>);

impl PlaylistRepository for MemPlaylists {
    fn insert(&self, playlist: &Playlist) -> Result<()> {
        self.0.lock().unwrap().push(playlist.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Playlist>> {
        Ok(self.0.lock().unwrap().iter().find(|p| p.id == id).cloned())
    }

    fn save(&self, playlist: &Playlist) -> Result<()> {
        let mut all = self.0.lock().unwrap();
        if let Some(p) = all.iter_mut().find(|p| p.id == playlist.id) {
            *p = playlist.clone();
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<()> {
        self.0.lock().unwrap().retain(|p| p.id != id);
        Ok(())
    }

    fn list(&self) -> Result<Vec<Playlist>> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn containing(&self, song_id: &str) -> Result<Vec<PlaylistRef>> {
        let all = self.0.lock().unwrap();
        Ok(all.iter().filter(|p| p.song_ids.iter().any(|s| s == song_id)).map(Playlist::to_ref).collect())
    }
}

/// Records what it was asked to do.
#[derive(Default)]
pub struct MemFiles {
    pub retagged: Mutex<Vec<String>>,
    pub deleted: Mutex<Vec<String>>,
}

impl AudioFiles for MemFiles {
    fn retag(&self, song: &Song) -> Result<()> {
        self.retagged.lock().unwrap().push(song.title.clone());
        Ok(())
    }

    fn delete(&self, file: &str) -> Result<()> {
        self.deleted.lock().unwrap().push(file.to_string());
        Ok(())
    }
}

pub struct FixedClock(pub &'static str);

impl Clock for FixedClock {
    fn now(&self) -> String {
        self.0.to_string()
    }
}

/// `id1`, `id2`, ...; keys are the same number padded to 32 characters.
#[derive(Default)]
pub struct SeqIds(AtomicUsize);

impl Ids for SeqIds {
    fn id(&self) -> String {
        format!("id{}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn key(&self) -> String {
        format!("{:0>32}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }
}
