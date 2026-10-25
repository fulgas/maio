//! Song library use cases: look up, search, rename, delete (arc42 8.2, 3.5).

use std::sync::Arc;

use parking_lot::Mutex;

use std::path::PathBuf;

use crate::domain::{DownloadFormat, Error as Rule, PlaylistRef, Song, download_name, song_deletable};
use serde::Serialize;

use crate::usecases::{
    Error, Result,
    ports::{AudioFiles, PlaylistRepository, SongQuery, SongRepository, SongSort},
};

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 100;

/// A song and the playlists it is in (arc42 3.5, `GET /songs`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SongView {
    #[serde(flatten)]
    pub song: Song,
    pub playlists: Vec<PlaylistRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SongPage {
    pub songs: Vec<SongView>,
    /// Pass back as `cursor` for the next page; `None` on the last one.
    pub next: Option<String>,
}

/// A song ready to send in a download format (arc42 3.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Download {
    pub path: PathBuf,
    /// A converted copy: the caller removes it once it has opened it.
    pub temporary: bool,
    pub name: String,
    pub format: DownloadFormat,
}

pub struct Songs {
    repo: Arc<dyn SongRepository>,
    playlists: Arc<dyn PlaylistRepository>,
    files: Arc<dyn AudioFiles>,
    /// Rename and delete one at a time, so a rename never resurrects a deleted song.
    edits: Mutex<()>,
}

impl Songs {
    pub fn new(repo: Arc<dyn SongRepository>, playlists: Arc<dyn PlaylistRepository>, files: Arc<dyn AudioFiles>) -> Self {
        Self { repo, playlists, files, edits: Mutex::new(()) }
    }

    pub fn download(&self, id: &str, format: &str) -> Result<Download> {
        let format = DownloadFormat::parse(format)?;
        let song = self.find(id)?;
        let e = self.files.export(&song, format)?;
        Ok(Download { path: e.path, temporary: e.temporary, name: download_name(&song.title, format), format })
    }

    pub fn total_bytes(&self) -> Result<u64> {
        Ok(self.repo.total_bytes()?)
    }

    pub fn get(&self, id: &str) -> Result<SongView> {
        let song = self.find(id)?;
        self.view(song)
    }

    pub fn search(&self, q: Option<&str>, sort: SongSort, limit: Option<usize>, cursor: Option<&str>) -> Result<SongPage> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = match cursor {
            None => 0,
            // At most i64::MAX: SQLite's OFFSET is signed, and `offset + limit` must not wrap.
            Some(c) => c
                .parse::<usize>()
                .ok()
                .filter(|&o| o <= i64::MAX as usize)
                .ok_or_else(|| Rule::invalid("bad_cursor", "the cursor is not one this server gave"))?,
        };
        let q = q.map(str::trim).filter(|q| !q.is_empty()).map(String::from);
        let mut songs = self.repo.search(&SongQuery { q, sort, limit: limit + 1, offset })?;
        let next = (songs.len() > limit).then(|| (offset + limit).to_string());
        songs.truncate(limit);
        let songs = songs.into_iter().map(|s| self.view(s)).collect::<Result<_>>()?;
        Ok(SongPage { songs, next })
    }

    /// The file's tags are rewritten first; if that fails, nothing changes.
    pub fn rename(&self, id: &str, title: &str) -> Result<SongView> {
        let _one_at_a_time = self.lock();
        let mut song = self.find(id)?;
        song.rename(title)?;
        self.files.retag(&song)?;
        self.repo.update(&song)?;
        self.view(song)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let _one_at_a_time = self.lock();
        let song = self.find(id)?;
        song_deletable(self.playlists.containing(id)?)?;
        self.repo.delete(id)?;
        self.files.delete(&song.file)?;
        Ok(())
    }

    fn find(&self, id: &str) -> Result<Song> {
        self.repo.get(id)?.ok_or_else(|| Error::from(Rule::NotFound("song")))
    }

    fn view(&self, song: Song) -> Result<SongView> {
        let playlists = self.playlists.containing(&song.id)?;
        Ok(SongView { song, playlists })
    }

    fn lock(&self) -> parking_lot::MutexGuard<'_, ()> {
        self.edits.lock()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::domain::{Error as Rule, Playlist, PlaylistRef, TitleSource, testing::song};

    use super::*;
    use crate::usecases::{
        ports::{PlaylistRepository, SongSort},
        testing::{MemFiles, MemPlaylists, MemSongs},
    };

    struct World {
        songs: Songs,
        repo: Arc<MemSongs>,
        playlists: Arc<MemPlaylists>,
        files: Arc<MemFiles>,
    }

    fn world(n: usize) -> World {
        let repo = Arc::new(MemSongs::default());
        for i in 0..n {
            let mut s = song(&format!("s{i:02}"));
            s.created_at = format!("2026-10-16T12:00:{i:02}Z");
            repo.0.lock().push(s);
        }
        let (playlists, files) = (Arc::new(MemPlaylists::default()), Arc::new(MemFiles::default()));
        World { songs: Songs::new(repo.clone(), playlists.clone(), files.clone()), repo, playlists, files }
    }

    fn holding(id: &str, name: &str, songs: &[&str]) -> Playlist {
        let mut p = Playlist::new(id.into(), name, "k".repeat(32), "t".into()).unwrap();
        p.song_ids = songs.iter().map(|s| s.to_string()).collect();
        p
    }

    #[test]
    fn get_lists_the_playlists_a_song_is_in() {
        let w = world(1);
        w.playlists.insert(&holding("p1", "Late", &["s00"])).unwrap();
        let v = w.songs.get("s00").unwrap();
        assert_eq!(v.playlists, [PlaylistRef { id: "p1".into(), name: "Late".into() }]);
        assert_eq!(w.songs.get("nope").unwrap_err().rule(), Some(&Rule::NotFound("song")));
    }

    #[test]
    fn rename_saves_a_person_title_and_rewrites_the_tags() {
        let w = world(1);
        let v = w.songs.rename("s00", " Night Bus ").unwrap();
        assert_eq!((v.song.title.as_str(), v.song.title_source), ("Night Bus", TitleSource::Person));
        assert_eq!(w.repo.get("s00").unwrap().unwrap().title, "Night Bus");
        assert_eq!(*w.files.retagged.lock(), ["Night Bus"]);
    }

    #[test]
    fn a_refused_rename_touches_neither_row_nor_file() {
        let w = world(1);
        assert_eq!(w.songs.rename("s00", "").unwrap_err().rule().map(Rule::code), Some("bad_title"));
        assert_eq!(w.repo.get("s00").unwrap().unwrap().title, "Song s00");
        assert!(w.files.retagged.lock().is_empty());
    }

    #[test]
    fn a_song_in_a_playlist_is_not_deleted() {
        let w = world(1);
        w.playlists.insert(&holding("p1", "Late", &["s00"])).unwrap();
        let e = w.songs.delete("s00").unwrap_err();
        assert_eq!(e.rule(), Some(&Rule::SongInPlaylist(vec![PlaylistRef { id: "p1".into(), name: "Late".into() }])));
        assert!(w.repo.get("s00").unwrap().is_some());
        assert!(w.files.deleted.lock().is_empty());
    }

    #[test]
    fn deleting_removes_the_row_and_the_file() {
        let w = world(1);
        w.songs.delete("s00").unwrap();
        assert_eq!(w.repo.get("s00").unwrap(), None);
        assert_eq!(*w.files.deleted.lock(), ["songs/s00.opus"]);
        assert_eq!(w.songs.delete("s00").unwrap_err().rule(), Some(&Rule::NotFound("song")));
    }

    #[test]
    fn search_pages_newest_first_with_a_cursor() {
        let w = world(5);
        let first = w.songs.search(None, SongSort::Newest, Some(2), None).unwrap();
        assert_eq!(first.songs.iter().map(|v| v.song.id.as_str()).collect::<Vec<_>>(), ["s04", "s03"]);
        let second = w.songs.search(None, SongSort::Newest, Some(2), first.next.as_deref()).unwrap();
        assert_eq!(second.songs.iter().map(|v| v.song.id.as_str()).collect::<Vec<_>>(), ["s02", "s01"]);
        let last = w.songs.search(None, SongSort::Newest, Some(2), second.next.as_deref()).unwrap();
        assert_eq!((last.songs.len(), last.next), (1, None));
    }

    #[test]
    fn search_limits_and_cursors_are_checked() {
        let w = world(3);
        assert_eq!(w.songs.search(None, SongSort::Newest, Some(0), None).unwrap().songs.len(), 1, "limit is at least 1");
        assert_eq!(w.songs.search(None, SongSort::Newest, Some(10_000), None).unwrap().songs.len(), 3);
        let e = w.songs.search(None, SongSort::Newest, None, Some("abc")).unwrap_err();
        assert_eq!(e.rule().map(Rule::code), Some("bad_cursor"));
    }

    #[test]
    fn a_huge_cursor_is_refused_not_wrapped() {
        let w = world(3);
        for c in ["18446744073709551615", "9223372036854775808"] {
            let e = w.songs.search(None, SongSort::Newest, None, Some(c)).unwrap_err();
            assert_eq!(e.rule().map(Rule::code), Some("bad_cursor"), "{c}");
        }
    }

    #[test]
    fn a_blank_query_matches_everything() {
        let w = world(2);
        assert_eq!(w.songs.search(Some("   "), SongSort::Title, None, None).unwrap().songs.len(), 2);
        assert_eq!(w.songs.search(Some("nothing like it"), SongSort::Title, None, None).unwrap().songs.len(), 0);
    }
}
