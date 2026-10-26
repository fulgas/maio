//! The use cases over the real SQLite adapters: the rules hold end to end.

use std::sync::Arc;

use maio::adapters::{
    sqlite::{Db, SqlitePlaylists, SqliteSongs},
    system::{RandomIds, SystemClock},
};
use maio::domain::{Error as Rule, Playlist, PlaylistRef, testing::song};
use maio::usecases::{
    Playlists, Songs,
    ports::{PlaylistRepository, SongRepository, SongSort},
    testing::MemFiles,
};

#[test]
fn songs_and_playlists_follow_the_rules_over_sqlite() {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let (song_table, playlist_table) = (Arc::new(SqliteSongs(db.clone())), Arc::new(SqlitePlaylists(db)));
    let files = Arc::new(MemFiles::default());
    let edits = maio::usecases::Edits::default();
    let songs = Songs::new(song_table.clone(), playlist_table.clone(), files.clone(), edits.clone());
    let playlists = Playlists::new(
        playlist_table,
        song_table.clone(),
        Arc::new(RandomIds),
        Arc::new(SystemClock),
        Arc::new(maio::usecases::testing::FakeLive::default()),
        4,
        edits,
    );
    for id in ["a", "b"] {
        song_table.insert(&song(id)).unwrap();
    }

    let late = playlists.create("Late").unwrap();
    playlists.add_songs(&late.id, &["b".into(), "a".into(), "b".into()], None).unwrap();
    assert_eq!(playlists.get(&late.id).unwrap().songs.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);

    let refused = songs.delete("a").unwrap_err();
    assert_eq!(refused.rule().map(Rule::code), Some("song_in_playlist"));
    assert_eq!(songs.get("a").unwrap().playlists[0].name, "Late");

    playlists.remove_song(&late.id, "a").unwrap();
    songs.delete("a").unwrap();
    assert_eq!(*files.deleted.lock(), ["songs/a.opus"]);

    songs.rename("b", "Night Bus").unwrap();
    let found = songs.search(Some("night"), SongSort::Newest, None, None).unwrap();
    assert_eq!(found.songs.iter().map(|v| v.song.id.as_str()).collect::<Vec<_>>(), ["b"]);

    assert_eq!(playlists.list().unwrap()[0].songs, 1);
    playlists.delete(&late.id).unwrap();
    songs.delete("b").unwrap();
}

/// A playlist table that lets another request in at one moment of an edit: while a song delete checks which playlists
/// hold the song, or while an add reads the playlist.
struct Racing {
    table: Arc<SqlitePlaylists>,
    during_containing: parking_lot::Mutex<Option<Box<dyn FnOnce() + Send>>>,
    during_get: parking_lot::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

fn race(slot: &parking_lot::Mutex<Option<Box<dyn FnOnce() + Send>>>) {
    if let Some(other) = slot.lock().take() {
        std::thread::spawn(other);
        std::thread::sleep(std::time::Duration::from_millis(200)); // the other request runs now, if nothing holds it
    }
}

impl PlaylistRepository for Racing {
    fn insert(&self, p: &Playlist) -> anyhow::Result<()> {
        self.table.insert(p)
    }
    fn get(&self, id: &str) -> anyhow::Result<Option<Playlist>> {
        race(&self.during_get);
        self.table.get(id)
    }
    fn save(&self, p: &Playlist) -> anyhow::Result<()> {
        self.table.save(p)
    }
    fn delete(&self, id: &str) -> anyhow::Result<()> {
        self.table.delete(id)
    }
    fn list(&self) -> anyhow::Result<Vec<Playlist>> {
        self.table.list()
    }
    fn containing(&self, song_id: &str) -> anyhow::Result<Vec<PlaylistRef>> {
        race(&self.during_containing);
        self.table.containing(song_id)
    }
}

type Outcome = Arc<parking_lot::Mutex<Option<maio::usecases::Result<()>>>>;

/// Songs and playlists over SQLite, with one playlist "p" and one song "a".
fn racing() -> (Arc<Songs>, Arc<Playlists>, Arc<Racing>, String) {
    let db = Arc::new(Db::open_in_memory().unwrap());
    let song_table = Arc::new(SqliteSongs(db.clone()));
    song_table.insert(&song("a")).unwrap();
    let table = Arc::new(Racing { table: Arc::new(SqlitePlaylists(db)), during_containing: Default::default(), during_get: Default::default() });
    let edits = maio::usecases::Edits::default();
    let songs = Arc::new(Songs::new(song_table.clone(), table.clone(), Arc::new(MemFiles::default()), edits.clone()));
    let playlists = Arc::new(Playlists::new(
        table.clone(),
        song_table,
        Arc::new(RandomIds),
        Arc::new(SystemClock),
        Arc::new(maio::usecases::testing::FakeLive::default()),
        4,
        edits,
    ));
    let id = playlists.create("Late").unwrap().id;
    (songs, playlists, table, id)
}

/// A refusal is a rule the person can read (404, 409), never a database error.
fn a_rule_or_ok(r: &maio::usecases::Result<()>) -> bool {
    r.as_ref().err().is_none_or(|e| e.rule().is_some())
}

#[test]
fn adding_a_song_while_it_is_deleted_answers_with_a_rule() {
    let (songs, playlists, table, p) = racing();
    let added: Outcome = Arc::default();
    let (a, pl, pid) = (added.clone(), playlists.clone(), p.clone());
    *table.during_containing.lock() = Some(Box::new(move || *a.lock() = Some(pl.add_songs(&pid, &["a".into()], None).map(drop))));
    let deleted = songs.delete("a");
    std::thread::sleep(std::time::Duration::from_millis(300));
    let added = added.lock().take().expect("the add ran");
    assert!(a_rule_or_ok(&deleted) && a_rule_or_ok(&added), "delete {deleted:?}, add {added:?}");
    assert_eq!(added.unwrap_err().rule(), Some(&Rule::NotFound("song")), "the song was gone by then");
}

#[test]
fn deleting_a_song_while_it_is_added_answers_with_a_rule() {
    let (songs, playlists, table, p) = racing();
    let deleted: Outcome = Arc::default();
    let (d, s) = (deleted.clone(), songs.clone());
    *table.during_get.lock() = Some(Box::new(move || *d.lock() = Some(s.delete("a"))));
    let added = playlists.add_songs(&p, &["a".into()], None).map(drop);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let deleted = deleted.lock().take().expect("the delete ran");
    assert!(a_rule_or_ok(&added) && a_rule_or_ok(&deleted), "add {added:?}, delete {deleted:?}");
    assert!(matches!(deleted.unwrap_err().rule(), Some(Rule::SongInPlaylist(_))), "the song was in the playlist by then");
}
