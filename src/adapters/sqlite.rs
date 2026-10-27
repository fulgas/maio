//! The library in one SQLite file: songs, playlists and their order.

use parking_lot::{Mutex, MutexGuard};
use std::{path::Path, sync::Arc};

use crate::domain::{Commercial, Licence, Live, Playlist, PlaylistRef, Song, TitleSource, Visibility};
use crate::usecases::ports::{AccountStore, PlaylistRepository, SongQuery, SongRepository, SongSort};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};
use sha2::{Digest, Sha256};

/// `playlist_songs.song_id` has no `ON DELETE` action, so SQLite itself refuses to delete a song
/// that a playlist holds (arc42 1.2), even if a use case's check races.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS songs (
    id TEXT PRIMARY KEY, title TEXT NOT NULL, title_source TEXT NOT NULL, description TEXT,
    prompt TEXT NOT NULL, mood TEXT, genre TEXT, energy TEXT, seconds REAL NOT NULL, bpm INTEGER,
    seed INTEGER NOT NULL, vocals INTEGER NOT NULL, musical_key TEXT, time_signature TEXT,
    lyrics TEXT, vocal_language TEXT, engine TEXT NOT NULL, licence_name TEXT NOT NULL,
    commercial TEXT NOT NULL, licence_note TEXT NOT NULL, lyrics_engine TEXT, title_engine TEXT,
    artist TEXT NOT NULL, file TEXT NOT NULL, bytes INTEGER NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS playlists (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, live INTEGER NOT NULL, shuffle INTEGER NOT NULL,
    visibility TEXT NOT NULL, listen_key TEXT NOT NULL UNIQUE, created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS playlist_songs (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    song_id TEXT NOT NULL REFERENCES songs(id),
    position INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, song_id)
);
CREATE TABLE IF NOT EXISTS account (
    id INTEGER PRIMARY KEY CHECK (id = 1), username TEXT NOT NULL, hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    id_hash TEXT PRIMARY KEY, expires_at INTEGER NOT NULL
);
";

/// Columns added to `songs` after the first release: added on open when missing, empty for the songs already there.
const ADDED_SONG_COLS: [&str; 2] = ["voice", "engine_prompt"];

/// The one connection both repositories share.
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path).with_context(|| format!("opening {}", path.display()))?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        for col in ADDED_SONG_COLS {
            let has: bool = conn.query_row("SELECT COUNT(*) FROM pragma_table_info('songs') WHERE name = ?", [col], |r| r.get(0))?;
            if !has {
                conn.execute_batch(&format!("ALTER TABLE songs ADD COLUMN {col} TEXT"))?;
            }
        }
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock()
    }
}

/// SQLite integers are signed: a size, count or time that does not fit is an error, never a silent wrap.
fn sql_int(n: u64, what: &str) -> Result<i64> {
    i64::try_from(n).with_context(|| format!("{what} {n} does not fit in SQLite"))
}

const SONG_COLS: &str = "id, title, title_source, description, prompt, mood, genre, energy, seconds, bpm, seed, vocals, \
    musical_key, time_signature, lyrics, vocal_language, engine, licence_name, commercial, licence_note, lyrics_engine, \
    title_engine, artist, file, bytes, created_at, voice, engine_prompt";

pub struct SqliteSongs(pub Arc<Db>);

impl SqliteSongs {
    /// Insert, or replace every field of the song with this id.
    fn put(&self, s: &Song) -> Result<()> {
        let set = SONG_COLS.split(", ").skip(1).map(|c| format!("{c} = excluded.{c}")).collect::<Vec<_>>().join(", ");
        let marks = vec!["?"; SONG_COLS.split(", ").count()].join(", ");
        let sql = format!("INSERT INTO songs ({SONG_COLS}) VALUES ({marks}) ON CONFLICT(id) DO UPDATE SET {set}");
        let bytes = sql_int(s.bytes, "song size")?;
        self.0.conn().execute(
            &sql,
            params![
                s.id,
                s.title,
                s.title_source.as_str(),
                s.description,
                s.prompt,
                s.mood,
                s.genre,
                s.energy,
                s.seconds,
                s.bpm,
                s.seed,
                s.vocals,
                s.key,
                s.time_signature,
                s.lyrics,
                s.vocal_language,
                s.engine,
                s.licence.name,
                s.licence.commercial.as_str(),
                s.licence.note,
                s.lyrics_engine,
                s.title_engine,
                s.artist,
                s.file,
                bytes,
                s.created_at,
                s.voice,
                s.engine_prompt,
            ],
        )?;
        Ok(())
    }
}

fn song_from(r: &Row) -> rusqlite::Result<Song> {
    let source: String = r.get(2)?;
    let commercial: String = r.get(18)?;
    Ok(Song {
        id: r.get(0)?,
        title: r.get(1)?,
        title_source: TitleSource::parse(&source).unwrap_or(TitleSource::Words),
        description: r.get(3)?,
        prompt: r.get(4)?,
        mood: r.get(5)?,
        genre: r.get(6)?,
        energy: r.get(7)?,
        seconds: r.get(8)?,
        bpm: r.get(9)?,
        seed: r.get(10)?,
        vocals: r.get(11)?,
        key: r.get(12)?,
        time_signature: r.get(13)?,
        lyrics: r.get(14)?,
        vocal_language: r.get(15)?,
        engine: r.get(16)?,
        licence: Licence { name: r.get(17)?, commercial: Commercial::parse(&commercial).unwrap_or(Commercial::No), note: r.get(19)? },
        lyrics_engine: r.get(20)?,
        title_engine: r.get(21)?,
        artist: r.get(22)?,
        file: r.get(23)?,
        bytes: u64::try_from(r.get::<_, i64>(24)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(24, rusqlite::types::Type::Integer, Box::new(e)))?,
        created_at: r.get(25)?,
        voice: r.get(26)?,
        engine_prompt: r.get(27)?,
    })
}

/// `%` and `_` in what a person typed match themselves (`ESCAPE '\'`).
fn like_pattern(text: &str) -> String {
    format!("%{}%", text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"))
}

impl SongRepository for SqliteSongs {
    fn total_bytes(&self) -> Result<u64> {
        let bytes: i64 = self.0.conn().query_row("SELECT COALESCE(SUM(bytes), 0) FROM songs", [], |r| r.get(0))?;
        u64::try_from(bytes).context("the songs' total size is negative")
    }

    fn title_taken(&self, title: &str) -> Result<bool> {
        Ok(self.0.conn().prepare("SELECT 1 FROM songs WHERE title = ?1")?.exists([title])?)
    }

    fn insert(&self, song: &Song) -> Result<()> {
        self.put(song)
    }

    fn get(&self, id: &str) -> Result<Option<Song>> {
        let sql = format!("SELECT {SONG_COLS} FROM songs WHERE id = ?1");
        Ok(self.0.conn().query_row(&sql, [id], song_from).optional()?)
    }

    fn get_many(&self, ids: &[String]) -> Result<Vec<Song>> {
        let mut songs = Vec::with_capacity(ids.len());
        for id in ids {
            songs.extend(self.get(id)?);
        }
        Ok(songs)
    }

    fn update(&self, song: &Song) -> Result<()> {
        self.put(song)
    }

    fn delete(&self, id: &str) -> Result<()> {
        self.0.conn().execute("DELETE FROM songs WHERE id = ?1", [id])?;
        Ok(())
    }

    fn search(&self, query: &SongQuery) -> Result<Vec<Song>> {
        let order = match query.sort {
            SongSort::Newest => "created_at DESC, id DESC",
            SongSort::Title => "title COLLATE NOCASE, id",
        };
        let matches = ["title", "prompt", "mood", "genre", "lyrics"].map(|col| format!("{col} LIKE ?1 ESCAPE '\\'")).join(" OR ");
        let sql = format!("SELECT {SONG_COLS} FROM songs WHERE ?1 IS NULL OR {matches} ORDER BY {order} LIMIT ?2 OFFSET ?3");
        let pattern = query.q.as_deref().map(like_pattern);
        let conn = self.0.conn();
        let mut stmt = conn.prepare(&sql)?;
        let (limit, offset) = (sql_int(u64::try_from(query.limit)?, "limit")?, sql_int(u64::try_from(query.offset)?, "offset")?);
        let rows = stmt.query_map(params![pattern, limit, offset], song_from)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

const PLAYLIST_COLS: &str = "id, name, live, shuffle, visibility, listen_key, created_at";

pub struct SqlitePlaylists(pub Arc<Db>);

fn playlist_from(r: &Row) -> rusqlite::Result<Playlist> {
    let visibility: String = r.get(4)?;
    Ok(Playlist {
        id: r.get(0)?,
        name: r.get(1)?,
        song_ids: Vec::new(),
        live: Live { on: r.get(2)?, shuffle: r.get(3)?, visibility: Visibility::parse(&visibility).unwrap_or(Visibility::Private), key: r.get(5)? },
        created_at: r.get(6)?,
    })
}

fn songs_of(conn: &Connection, playlist_id: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT song_id FROM playlist_songs WHERE playlist_id = ?1 ORDER BY position")?;
    let rows = stmt.query_map([playlist_id], |r| r.get(0))?;
    rows.collect()
}

impl SqlitePlaylists {
    /// The row and the whole song order, in one transaction.
    fn put(&self, p: &Playlist) -> Result<()> {
        let mut conn = self.0.conn();
        let tx = conn.transaction()?;
        tx.execute(
            &format!(
                "INSERT INTO playlists ({PLAYLIST_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, live = excluded.live, shuffle = excluded.shuffle,
                 visibility = excluded.visibility, listen_key = excluded.listen_key"
            ),
            params![p.id, p.name, p.live.on, p.live.shuffle, p.live.visibility.as_str(), p.live.key, p.created_at],
        )?;
        tx.execute("DELETE FROM playlist_songs WHERE playlist_id = ?1", [&p.id])?;
        for (position, song_id) in p.song_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_songs (playlist_id, song_id, position) VALUES (?1, ?2, ?3)",
                params![p.id, song_id, sql_int(u64::try_from(position)?, "position")?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

impl PlaylistRepository for SqlitePlaylists {
    fn insert(&self, playlist: &Playlist) -> Result<()> {
        self.put(playlist)
    }

    fn get(&self, id: &str) -> Result<Option<Playlist>> {
        let conn = self.0.conn();
        let found = conn.query_row(&format!("SELECT {PLAYLIST_COLS} FROM playlists WHERE id = ?1"), [id], playlist_from).optional()?;
        let Some(mut p) = found else { return Ok(None) };
        p.song_ids = songs_of(&conn, &p.id)?;
        Ok(Some(p))
    }

    fn save(&self, playlist: &Playlist) -> Result<()> {
        self.put(playlist)
    }

    fn delete(&self, id: &str) -> Result<()> {
        self.0.conn().execute("DELETE FROM playlists WHERE id = ?1", [id])?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<Playlist>> {
        let conn = self.0.conn();
        let mut stmt = conn.prepare(&format!("SELECT {PLAYLIST_COLS} FROM playlists ORDER BY created_at, id"))?;
        let mut all = stmt.query_map([], playlist_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
        for p in &mut all {
            p.song_ids = songs_of(&conn, &p.id)?;
        }
        Ok(all)
    }

    fn containing(&self, song_id: &str) -> Result<Vec<PlaylistRef>> {
        let conn = self.0.conn();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name FROM playlists p JOIN playlist_songs ps ON ps.playlist_id = p.id
             WHERE ps.song_id = ?1 ORDER BY p.created_at, p.id",
        )?;
        let rows = stmt.query_map([song_id], |r| Ok(PlaylistRef { id: r.get(0)?, name: r.get(1)? }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// The account and its sessions (arc42 8.1). Session ids are kept only as SHA-256 hashes,
/// so a copy of the database file signs nobody in.
pub struct SqliteAccount(pub Arc<Db>);

fn id_hash(id: &str) -> String {
    format!("{:x}", Sha256::digest(id.as_bytes()))
}

impl AccountStore for SqliteAccount {
    fn account(&self) -> Result<Option<(String, String)>> {
        let found = self.0.conn().query_row("SELECT username, hash FROM account WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        Ok(found)
    }

    fn set_account(&self, username: &str, hash: &str) -> Result<()> {
        self.0.conn().execute(
            "INSERT INTO account (id, username, hash) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET username = excluded.username, hash = excluded.hash",
            params![username, hash],
        )?;
        Ok(())
    }

    fn create_session(&self, id: &str, expires_at: u64) -> Result<()> {
        self.0.conn().execute("INSERT INTO sessions (id_hash, expires_at) VALUES (?1, ?2)", params![id_hash(id), sql_int(expires_at, "expiry")?])?;
        Ok(())
    }

    fn session_valid(&self, id: &str, now: u64) -> Result<bool> {
        Ok(self
            .0
            .conn()
            .prepare("SELECT 1 FROM sessions WHERE id_hash = ?1 AND expires_at > ?2")?
            .exists(params![id_hash(id), sql_int(now, "time")?])?)
    }

    fn delete_session(&self, id: &str) -> Result<()> {
        self.0.conn().execute("DELETE FROM sessions WHERE id_hash = ?1", [id_hash(id)])?;
        Ok(())
    }

    fn delete_other_sessions(&self, keep: &str) -> Result<()> {
        self.0.conn().execute("DELETE FROM sessions WHERE id_hash != ?1", [id_hash(keep)])?;
        Ok(())
    }

    fn delete_expired_sessions(&self, now: u64) -> Result<()> {
        self.0.conn().execute("DELETE FROM sessions WHERE expires_at <= ?1", [sql_int(now, "time")?])?;
        Ok(())
    }
}

#[cfg(test)]
mod song_tests {
    use std::sync::Arc;

    use crate::domain::{Commercial, Song, TitleSource, testing::song};
    use crate::usecases::ports::{SongQuery, SongRepository, SongSort};

    use super::*;

    fn songs() -> SqliteSongs {
        SqliteSongs(Arc::new(Db::open_in_memory().unwrap()))
    }

    fn query(q: Option<&str>, sort: SongSort) -> SongQuery {
        SongQuery { q: q.map(String::from), sort, limit: 50, offset: 0 }
    }

    fn ids(found: Vec<Song>) -> Vec<String> {
        found.into_iter().map(|s| s.id).collect()
    }

    #[test]
    fn a_song_round_trips_with_every_field() {
        let db = songs();
        let mut s = song("a");
        s.title_source = TitleSource::Lyrics;
        s.description = Some("rain on a bus".into());
        s.vocals = true;
        s.key = Some("C major".into());
        s.time_signature = Some("4".into());
        s.lyrics = Some("[chorus]\nnight bus".into());
        s.vocal_language = Some("en".into());
        s.voice = Some("duet".into());
        s.licence.commercial = Commercial::Limited;
        s.licence.note = "revenue cap".into();
        s.lyrics_engine = Some("local".into());
        s.title_engine = Some("local".into());
        s.bytes = 5_000_000_000;
        db.insert(&s).unwrap();
        assert_eq!(db.get("a").unwrap(), Some(s));
        assert_eq!(db.get("b").unwrap(), None);
    }

    #[test]
    fn a_library_from_before_voice_and_engine_text_opens_with_its_songs() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(
            "INSERT INTO songs VALUES ('old', 'Old', 'words', NULL, 'pads', NULL, NULL, NULL, 60.0, NULL, 1, 0, NULL, NULL, NULL, \
             NULL, 'ace-step', 'MIT', 'yes', '', NULL, NULL, 'Maio', 'songs/old.opus', 10, '2026-01-01T00:00:00Z')",
        )
        .unwrap();
        let db = SqliteSongs(Arc::new(Db::init(conn).unwrap()));
        let old = db.get("old").unwrap().unwrap();
        assert_eq!((old.voice, old.engine_prompt), (None, None));
        db.insert(&song("new")).unwrap();
        assert_eq!(db.get("new").unwrap(), Some(song("new")));
    }

    #[test]
    fn update_changes_and_delete_removes() {
        let db = songs();
        let mut s = song("a");
        db.insert(&s).unwrap();
        s.rename("Night Bus").unwrap();
        db.update(&s).unwrap();
        assert_eq!(db.get("a").unwrap().unwrap().title_source, TitleSource::Person);
        db.delete("a").unwrap();
        assert_eq!(db.get("a").unwrap(), None);
    }

    #[test]
    fn get_many_keeps_the_order_given_and_skips_unknown_ids() {
        let db = songs();
        for id in ["a", "b", "c"] {
            db.insert(&song(id)).unwrap();
        }
        assert_eq!(ids(db.get_many(&["c", "x", "a"].map(String::from)).unwrap()), ["c", "a"]);
    }

    #[test]
    fn search_covers_titles_prompts_moods_genres_and_lyrics() {
        let db = songs();
        let mut a = song("a");
        a.title = "Night Bus".into();
        let mut b = song("b");
        b.prompt = "rainy harbour".into();
        let mut c = song("c");
        c.mood = Some("tense".into());
        let mut d = song("d");
        d.lyrics = Some("[verse]\nlanterns in the dark".into());
        for s in [&a, &b, &c, &d] {
            db.insert(s).unwrap();
        }
        assert_eq!(ids(db.search(&query(Some("night bus"), SongSort::Newest)).unwrap()), ["a"]);
        assert_eq!(ids(db.search(&query(Some("HARBOUR"), SongSort::Newest)).unwrap()), ["b"]);
        assert_eq!(ids(db.search(&query(Some("tense"), SongSort::Newest)).unwrap()), ["c"]);
        assert_eq!(ids(db.search(&query(Some("lanterns"), SongSort::Newest)).unwrap()), ["d"]);
        assert_eq!(db.search(&query(None, SongSort::Newest)).unwrap().len(), 4);
    }

    #[test]
    fn search_text_is_literal_not_a_wildcard() {
        let db = songs();
        for (id, title) in [("a", "100% Rain"), ("b", "1000 Rains"), ("c", "lo_fi nights"), ("d", "lofi nights")] {
            let mut s = song(id);
            s.title = title.into();
            db.insert(&s).unwrap();
        }
        assert_eq!(ids(db.search(&query(Some("100%"), SongSort::Newest)).unwrap()), ["a"]);
        assert_eq!(ids(db.search(&query(Some("lo_fi"), SongSort::Newest)).unwrap()), ["c"]);
    }

    #[test]
    fn search_sorts_newest_first_or_by_title_and_pages() {
        let db = songs();
        for (id, title, at) in [("a", "beta", "2026-10-16T12:00:01Z"), ("b", "Alpha", "2026-10-16T12:00:03Z"), ("c", "gamma", "2026-10-16T12:00:02Z")]
        {
            let mut s = song(id);
            s.title = title.into();
            s.created_at = at.into();
            db.insert(&s).unwrap();
        }
        assert_eq!(ids(db.search(&query(None, SongSort::Newest)).unwrap()), ["b", "c", "a"]);
        assert_eq!(ids(db.search(&query(None, SongSort::Title)).unwrap()), ["b", "a", "c"]);
        let page = SongQuery { q: None, sort: SongSort::Newest, limit: 1, offset: 1 };
        assert_eq!(ids(db.search(&page).unwrap()), ["c"]);
    }

    #[test]
    fn a_title_is_taken_once_a_song_has_it() {
        let db = songs();
        db.insert(&song("a")).unwrap();
        assert!(db.title_taken("Song a").unwrap());
        assert!(!db.title_taken("song a").unwrap(), "exact title, as word titles are made");
    }

    #[test]
    fn total_bytes_adds_up_every_song() {
        let db = songs();
        assert_eq!(db.total_bytes().unwrap(), 0);
        db.insert(&Song { bytes: 1500, ..song("a") }).unwrap();
        db.insert(&Song { bytes: 2500, ..song("b") }).unwrap();
        assert_eq!(db.total_bytes().unwrap(), 4000);
    }
}

#[cfg(test)]
mod playlist_tests {
    use std::sync::Arc;

    use crate::domain::{Playlist, PlaylistRef, Visibility, testing::song};
    use crate::usecases::ports::{PlaylistRepository, SongRepository};

    use super::*;

    fn tables(db: Db, song_ids: &[&str]) -> (SqliteSongs, SqlitePlaylists) {
        let db = Arc::new(db);
        let songs = SqliteSongs(db.clone());
        for id in song_ids {
            songs.insert(&song(id)).unwrap();
        }
        (songs, SqlitePlaylists(db))
    }

    fn playlist(id: &str, name: &str, at: &str, song_ids: &[&str]) -> Playlist {
        let mut p = Playlist::new(id.into(), name, format!("{id:0>32}"), at.into()).unwrap();
        p.song_ids = song_ids.iter().map(|s| s.to_string()).collect();
        p
    }

    #[test]
    fn a_playlist_round_trips_with_its_order_and_live_settings() {
        let (_, playlists) = tables(Db::open_in_memory().unwrap(), &["a", "b", "c"]);
        let mut p = playlist("p1", "Late", "2026-10-16T12:00:00Z", &[]);
        playlists.insert(&p).unwrap();
        p.add(&["c", "a", "b"].map(String::from), None);
        p.live.on = true;
        p.live.shuffle = true;
        p.live.visibility = Visibility::Private;
        playlists.save(&p).unwrap();
        assert_eq!(playlists.get("p1").unwrap(), Some(p.clone()));
        p.reorder(["b", "c", "a"].map(String::from).to_vec()).unwrap();
        p.remove("c").unwrap();
        p.rename("Later").unwrap();
        playlists.save(&p).unwrap();
        assert_eq!(playlists.get("p1").unwrap(), Some(p));
        assert_eq!(playlists.get("nope").unwrap(), None);
    }

    #[test]
    fn list_is_oldest_first_and_containing_names_the_playlists() {
        let (_, playlists) = tables(Db::open_in_memory().unwrap(), &["a"]);
        playlists.insert(&playlist("p2", "Late", "2026-10-16T12:00:02Z", &["a"])).unwrap();
        playlists.insert(&playlist("p1", "Early", "2026-10-16T12:00:01Z", &["a"])).unwrap();
        playlists.insert(&playlist("p3", "Empty", "2026-10-16T12:00:03Z", &[])).unwrap();
        assert_eq!(playlists.list().unwrap().iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["p1", "p2", "p3"]);
        let named = |id: &str, name: &str| PlaylistRef { id: id.into(), name: name.into() };
        assert_eq!(playlists.containing("a").unwrap(), [named("p1", "Early"), named("p2", "Late")]);
        assert!(playlists.containing("zzz").unwrap().is_empty());
    }

    #[test]
    fn the_database_refuses_to_delete_a_song_a_playlist_holds() {
        let (songs, playlists) = tables(Db::open_in_memory().unwrap(), &["a"]);
        playlists.insert(&playlist("p1", "Late", "t", &["a"])).unwrap();
        assert!(songs.delete("a").is_err());
        assert!(songs.get("a").unwrap().is_some());
    }

    #[test]
    fn deleting_a_playlist_frees_its_songs() {
        let (songs, playlists) = tables(Db::open_in_memory().unwrap(), &["a"]);
        playlists.insert(&playlist("p1", "Late", "t", &["a"])).unwrap();
        playlists.delete("p1").unwrap();
        assert_eq!(playlists.get("p1").unwrap(), None);
        assert!(playlists.containing("a").unwrap().is_empty());
        songs.delete("a").unwrap();
    }

    #[test]
    fn a_playlist_never_holds_a_song_twice() {
        let (_, playlists) = tables(Db::open_in_memory().unwrap(), &["a"]);
        assert!(playlists.insert(&playlist("p1", "Late", "t", &["a", "a"])).is_err(), "the primary key refuses a repeat");
        assert_eq!(playlists.get("p1").unwrap(), None, "and nothing of it is stored");
    }

    #[test]
    fn the_library_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        {
            let (_, playlists) = tables(Db::open(&path).unwrap(), &["a", "b"]);
            playlists.insert(&playlist("p1", "Late", "t", &["b", "a"])).unwrap();
        }
        let (songs, playlists) = tables(Db::open(&path).unwrap(), &[]);
        assert_eq!(songs.get("a").unwrap(), Some(song("a")));
        assert_eq!(playlists.get("p1").unwrap().unwrap().song_ids, ["b", "a"]);
    }
}

#[cfg(test)]
mod account_tests {
    use std::sync::Arc;

    use crate::usecases::ports::AccountStore;

    use super::*;

    fn store() -> SqliteAccount {
        SqliteAccount(Arc::new(Db::open_in_memory().unwrap()))
    }

    #[test]
    fn the_account_is_one_row_that_can_change_its_hash() {
        let s = store();
        assert_eq!(s.account().unwrap(), None);
        s.set_account("me", "h1").unwrap();
        s.set_account("me", "h2").unwrap();
        assert_eq!(s.account().unwrap(), Some(("me".into(), "h2".into())));
    }

    #[test]
    fn sessions_expire_end_and_are_kept_only_as_hashes() {
        let s = store();
        s.create_session("aaaa", 200).unwrap();
        s.create_session("bbbb", 300).unwrap();
        assert!(s.session_valid("aaaa", 100).unwrap());
        assert!(!s.session_valid("aaaa", 200).unwrap(), "expired at its time");
        assert!(!s.session_valid("zzzz", 100).unwrap());
        let raw: i64 = s.0.conn().query_row("SELECT COUNT(*) FROM sessions WHERE id_hash IN ('aaaa', 'bbbb')", [], |r| r.get(0)).unwrap();
        assert_eq!(raw, 0, "no session id is stored in the clear");
        s.delete_expired_sessions(250).unwrap();
        assert!(!s.session_valid("aaaa", 0).unwrap() && s.session_valid("bbbb", 0).unwrap());
        s.create_session("cccc", 900).unwrap();
        s.delete_other_sessions("cccc").unwrap();
        assert!(!s.session_valid("bbbb", 0).unwrap() && s.session_valid("cccc", 0).unwrap());
        s.delete_session("cccc").unwrap();
        assert!(!s.session_valid("cccc", 0).unwrap());
    }
}
