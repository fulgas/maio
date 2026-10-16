use std::{path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, Row, params, types::{Type, Value as SqlValue}};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::playlists::{Kind, Order, Playlist, Rule};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tracks (
    id TEXT PRIMARY KEY, title TEXT NOT NULL, file TEXT NOT NULL, prompt TEXT NOT NULL,
    mood TEXT, genre TEXT, energy TEXT, engine TEXT NOT NULL, licence TEXT NOT NULL,
    commercial TEXT NOT NULL, seconds REAL NOT NULL, bpm INTEGER, vocals INTEGER NOT NULL,
    seed INTEGER NOT NULL, favourite INTEGER NOT NULL DEFAULT 0, made_by TEXT,
    requested_by TEXT NOT NULL, created_at TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
    bytes INTEGER NOT NULL DEFAULT 0, title_source TEXT NOT NULL DEFAULT 'words',
    artist TEXT NOT NULL DEFAULT 'Maio', album TEXT NOT NULL DEFAULT 'Requests', musical_key TEXT,
    time_signature TEXT, lyrics TEXT, vocal_language TEXT, description TEXT, lyrics_engine TEXT, title_engine TEXT
);
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY, track_id TEXT NOT NULL, title TEXT NOT NULL,
    started_at TEXT NOT NULL, ended_at TEXT, skipped INTEGER NOT NULL DEFAULT 0,
    radio TEXT NOT NULL DEFAULT 'main'
);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS radios (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, visibility TEXT NOT NULL,
    listen_key TEXT NOT NULL UNIQUE, created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS users (
    username TEXT PRIMARY KEY, name TEXT NOT NULL, role TEXT NOT NULL, hash TEXT NOT NULL,
    must_change INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, last_seen_at TEXT
);
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY, username TEXT NOT NULL, expires_at INTEGER NOT NULL, remember INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS playlists (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, play_order TEXT NOT NULL,
    max_tracks INTEGER, recipe TEXT, rule TEXT, created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS playlist_tracks (
    playlist_id TEXT NOT NULL, track_id TEXT NOT NULL, position INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, track_id)
);
";

const COLS: &str = "id, title, file, prompt, mood, genre, energy, engine, licence, commercial, seconds, bpm, vocals, seed, favourite, made_by, requested_by, created_at, bytes, title_source, artist, album, musical_key, time_signature, lyrics, vocal_language, description, lyrics_engine, title_engine";

/// Tracks the storage budget and sweeps never delete besides kept ones (SPEC 10, Storage protection).
const IN_FIXED: &str = "SELECT pt.track_id FROM playlist_tracks pt JOIN playlists p ON p.id = pt.playlist_id WHERE p.kind = 'fixed'";


pub fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub file: String,
    pub prompt: String,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<String>,
    pub engine: String,
    pub licence: String,
    pub commercial: String,
    pub seconds: f64,
    pub bpm: Option<u32>,
    pub vocals: bool,
    pub seed: i64,
    pub favourite: bool,
    /// The generative playlist that made it; `None` for one-time requests (side A).
    pub made_by: Option<String>,
    pub requested_by: String,
    pub created_at: String,
    /// File size on disk; counts against the storage budget.
    pub bytes: i64,
    /// Which layer named it (SPEC 10, Titles): `person`, `engine`, `lyrics` or `words`.
    pub title_source: String,
    /// The generative playlist that made it, else "Maio".
    pub artist: String,
    /// The generative playlist that made it, else "Requests".
    pub album: String,
    /// Musical key, when the engine reports it (for example "C major").
    pub key: Option<String>,
    pub time_signature: Option<String>,
    /// The words actually sung.
    pub lyrics: Option<String>,
    pub vocal_language: Option<String>,
    /// One line, only when a text engine named the song.
    pub description: Option<String>,
    /// Which engine wrote the lyrics, and which named the song, for credits and licence terms.
    pub lyrics_engine: Option<String>,
    pub title_engine: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Play {
    pub id: i64,
    pub track_id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub skipped: bool,
    /// Whether the track is kept, for the keep buttons beside each play.
    pub favourite: bool,
}

fn track_at(r: &Row, o: usize) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(o)?,
        title: r.get(o + 1)?,
        file: r.get(o + 2)?,
        prompt: r.get(o + 3)?,
        mood: r.get(o + 4)?,
        genre: r.get(o + 5)?,
        energy: r.get(o + 6)?,
        engine: r.get(o + 7)?,
        licence: r.get(o + 8)?,
        commercial: r.get(o + 9)?,
        seconds: r.get(o + 10)?,
        bpm: r.get(o + 11)?,
        vocals: r.get(o + 12)?,
        seed: r.get(o + 13)?,
        favourite: r.get(o + 14)?,
        made_by: r.get(o + 15)?,
        requested_by: r.get(o + 16)?,
        created_at: r.get(o + 17)?,
        bytes: r.get(o + 18)?,
        title_source: r.get(o + 19)?,
        artist: r.get(o + 20)?,
        album: r.get(o + 21)?,
        key: r.get(o + 22)?,
        time_signature: r.get(o + 23)?,
        lyrics: r.get(o + 24)?,
        vocal_language: r.get(o + 25)?,
        description: r.get(o + 26)?,
        lyrics_engine: r.get(o + 27)?,
        title_engine: r.get(o + 28)?,
    })
}

fn to_json<T: Serialize>(v: &T) -> rusqlite::Result<String> {
    serde_json::to_string(v).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}


fn from_json<T: DeserializeOwned>(col: usize, s: Option<String>) -> rusqlite::Result<Option<T>> {
    s.map(|s| serde_json::from_str(&s).map_err(|e| rusqlite::Error::FromSqlConversionFailure(col, Type::Text, Box::new(e)))).transpose()
}

const PLAYLIST_COLS: &str = "id, name, kind, play_order, max_tracks, recipe, rule, created_at";

fn playlist_row(r: &Row) -> rusqlite::Result<Playlist> {
    let bad = |col: usize, v: String| rusqlite::Error::FromSqlConversionFailure(col, Type::Text, v.into());
    let (kind, order): (String, String) = (r.get(2)?, r.get(3)?);
    Ok(Playlist {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: Kind::parse(&kind).ok_or_else(|| bad(2, kind))?,
        order: Order::parse(&order).ok_or_else(|| bad(3, order))?,
        max_tracks: r.get(4)?,
        recipe: from_json(5, r.get(5)?)?,
        rule: from_json(6, r.get(6)?)?,
        created_at: r.get(7)?,
    })
}

const IN_PLAYLIST: &str = "id IN (SELECT track_id FROM playlist_tracks WHERE playlist_id = ?)";

/// The condition on `tracks` that selects a playlist's tracks, and its arguments.
fn member_clause(p: &Playlist) -> (String, Vec<SqlValue>) {
    match p.kind {
        Kind::Crate => ("1".into(), vec![]),
        Kind::Kept => ("favourite = 1".into(), vec![]),
        Kind::Requests => ("made_by IS NULL".into(), vec![]),
        Kind::Fixed | Kind::Generative => (IN_PLAYLIST.into(), vec![p.id.clone().into()]),
        Kind::Smart => rule_clause(p.rule.as_ref().unwrap_or(&Rule::default())),
    }
}

fn rule_clause(r: &Rule) -> (String, Vec<SqlValue>) {
    let mut c = vec!["1".to_string()];
    let mut a: Vec<SqlValue> = Vec::new();
    if let Some(m) = &r.mood {
        c.push("mood = ?".into());
        a.push(m.clone().into());
    }
    if let Some(g) = &r.genre {
        c.push("genre = ?".into());
        a.push(g.clone().into());
    }
    if let Some(k) = r.kept {
        c.push("favourite = ?".into());
        a.push(i64::from(k).into());
    }
    match r.side.as_deref() {
        Some("a") => c.push("made_by IS NULL".into()),
        Some("b") => c.push("made_by IS NOT NULL".into()),
        _ => {}
    }
    if let Some(p) = &r.from_playlist {
        c.push(IN_PLAYLIST.into());
        a.push(p.clone().into());
    }
    if let Some(s) = r.max_seconds {
        c.push("seconds <= ?".into());
        a.push(f64::from(s).into());
    }
    if let Some(d) = r.made_within_days {
        c.push("created_at >= strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?)".into());
        a.push(format!("-{d} days").into());
    }
    (c.join(" AND "), a)
}

fn write_order(tx: &rusqlite::Transaction, id: &str, order: &[String]) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", [id])?;
    let mut ins = tx.prepare("INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)")?;
    for (i, t) in order.iter().enumerate() {
        ins.execute(params![id, t, i as i64])?;
    }
    Ok(())
}

/// Side A: asked for once by a person. Side B: made by a generative playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// How the crate lists tracks (SPEC 10, Web UI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Newest,
    /// Most recently played first; never-played tracks last.
    Played,
    Title,
}

/// Crate filters; every field narrows the list.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrackFilter<'a> {
    pub q: Option<&'a str>,
    pub mood: Option<&'a str>,
    pub fav: Option<bool>,
    pub side: Option<Side>,
    pub playlist: Option<&'a Playlist>,
}

/// A person who signs in (SPEC 8). `role` is `admin`, `operator` or `listener`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct User {
    pub username: String,
    pub name: String,
    pub role: String,
    #[serde(skip)]
    pub hash: String,
    pub must_change: bool,
    pub created_at: String,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: String,
    pub username: String,
    pub expires_at: i64,
    pub remember: bool,
}

/// Who can listen to a radio (SPEC 10): anyone who knows its name, or only people given its secret link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Self::Public),
            "private" => Some(Self::Private),
            _ => None,
        }
    }
}

/// A radio as stored; its runtime lives in `radio::Radio`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RadioRow {
    pub id: String,
    pub name: String,
    pub visibility: Visibility,
    #[serde(skip)]
    pub listen_key: String,
    pub created_at: String,
}

fn radio_row(r: &Row) -> rusqlite::Result<RadioRow> {
    let v: String = r.get(2)?;
    Ok(RadioRow {
        id: r.get(0)?,
        name: r.get(1)?,
        visibility: Visibility::parse(&v).ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(2, Type::Text, format!("visibility {v}").into()))?,
        listen_key: r.get(3)?,
        created_at: r.get(4)?,
    })
}

const USER_COLS: &str = "username, name, role, hash, must_change, created_at, last_seen_at";

fn user_at(r: &Row, o: usize) -> rusqlite::Result<User> {
    Ok(User { username: r.get(o)?, name: r.get(o + 1)?, role: r.get(o + 2)?, hash: r.get(o + 3)?, must_change: r.get(o + 4)?, created_at: r.get(o + 5)?, last_seen_at: r.get(o + 6)? })
}

/// SQLite behind one mutex. ponytail: one connection, fine for one writer and a handful of reads per second.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(SCHEMA)?;
        // Machine tokens are gone (SPEC 2): drop what older databases kept for them.
        conn.execute_batch("DROP TABLE IF EXISTS tokens; DELETE FROM settings WHERE key LIKE 'token_removed:%';")?;
        // Databases from before the storage budget lack tracks.bytes.
        let has_bytes = conn.prepare("SELECT 1 FROM pragma_table_info('tracks') WHERE name = 'bytes'")?.exists([])?;
        if !has_bytes {
            conn.execute("ALTER TABLE tracks ADD COLUMN bytes INTEGER NOT NULL DEFAULT 0", [])?;
        }
        // Databases from before playlists name the maker column station_id (SPEC 10).
        let has_station_id = conn.prepare("SELECT 1 FROM pragma_table_info('tracks') WHERE name = 'station_id'")?.exists([])?;
        if has_station_id {
            conn.execute("ALTER TABLE tracks RENAME COLUMN station_id TO made_by", [])?;
        }
        // Databases from before radios: plays belong to main, and the active station becomes main's.
        let has_radio = conn.prepare("SELECT 1 FROM pragma_table_info('history') WHERE name = 'radio'")?.exists([])?;
        if !has_radio {
            conn.execute("ALTER TABLE history ADD COLUMN radio TEXT NOT NULL DEFAULT 'main'", [])?;
        }
        conn.execute("UPDATE OR IGNORE settings SET key = 'active_station:main' WHERE key = 'active_station'", [])?;
        // Stations become generative playlists with the same ids, and the tracks they made their members (SPEC 10).
        let has_stations = conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'stations'")?.exists([])?;
        if has_stations {
            conn.execute_batch(
                "INSERT OR IGNORE INTO playlists (id, name, kind, play_order, max_tracks, recipe, rule, created_at)
                   SELECT id, json_extract(body, '$.name'), 'generative', 'shuffle', NULL, json_remove(body, '$.id', '$.name'), NULL,
                          strftime('%Y-%m-%dT%H:%M:%SZ', 'now') FROM stations;
                 INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position)
                   SELECT made_by, id, ROW_NUMBER() OVER (PARTITION BY made_by ORDER BY rowid) - 1 FROM tracks
                   WHERE deleted = 0 AND made_by IN (SELECT id FROM stations);
                 DROP TABLE stations;",
            )?;
        }
        conn.execute("UPDATE OR IGNORE settings SET key = 'on_air:' || substr(key, 16) WHERE key LIKE 'active_station:%'", [])?;
        conn.execute("DELETE FROM settings WHERE key LIKE 'active_station:%'", [])?;
        // Databases from before song metadata (SPEC 10): tracks gain it, and a playlist's tracks take its name as artist and album.
        // Column by column, so a start cut short part-way finishes on the next one.
        let has_artist = conn.prepare("SELECT 1 FROM pragma_table_info('tracks') WHERE name = 'artist'")?.exists([])?;
        for (col, decl) in [
            ("title_source", "TEXT NOT NULL DEFAULT 'words'"), ("artist", "TEXT NOT NULL DEFAULT 'Maio'"),
            ("album", "TEXT NOT NULL DEFAULT 'Requests'"), ("musical_key", "TEXT"), ("time_signature", "TEXT"), ("lyrics", "TEXT"),
            ("vocal_language", "TEXT"), ("description", "TEXT"), ("lyrics_engine", "TEXT"), ("title_engine", "TEXT"),
        ] {
            if !conn.prepare("SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1")?.exists([col])? {
                conn.execute(&format!("ALTER TABLE tracks ADD COLUMN {col} {decl}"), [])?;
            }
        }
        if !has_artist {
            conn.execute("UPDATE tracks SET artist = p.name, album = p.name FROM playlists p WHERE p.id = tracks.made_by", [])?;
        }
        conn.execute(
            "INSERT OR IGNORE INTO radios (id, name, visibility, listen_key, created_at) VALUES ('main', 'Main', 'public', ?1, ?2)",
            params![crate::users::listen_key(), now()],
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn insert_track(&self, t: &Track) -> rusqlite::Result<()> {
        let marks = (1..=29).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
        self.conn.lock().unwrap().execute(
            &format!("INSERT INTO tracks ({COLS}) VALUES ({marks})"),
            params![t.id, t.title, t.file, t.prompt, t.mood, t.genre, t.energy, t.engine, t.licence, t.commercial,
                    t.seconds, t.bpm, t.vocals, t.seed, t.favourite, t.made_by, t.requested_by, t.created_at, t.bytes,
                    t.title_source, t.artist, t.album, t.key, t.time_signature, t.lyrics, t.vocal_language,
                    t.description, t.lyrics_engine, t.title_engine],
        )?;
        Ok(())
    }

    pub fn track(&self, id: &str) -> rusqlite::Result<Option<Track>> {
        self.conn.lock().unwrap()
            .query_row(&format!("SELECT {COLS} FROM tracks WHERE id = ?1 AND deleted = 0"), [id], |r| track_at(r, 0))
            .optional()
    }

    /// A page of the crate. `next` is the offset of the following page, when there is one.
    pub fn tracks(&self, f: &TrackFilter, sort: Sort, limit: u32, offset: u32) -> rusqlite::Result<(Vec<Track>, Option<u32>)> {
        let TrackFilter { q, mood, fav, side, playlist } = *f;
        let mut sql = format!("SELECT {COLS} FROM tracks WHERE deleted = 0");
        let mut args: Vec<SqlValue> = Vec::new();
        match side {
            Some(Side::A) => sql.push_str(" AND made_by IS NULL"),
            Some(Side::B) => sql.push_str(" AND made_by IS NOT NULL"),
            None => {}
        }
        if let Some(p) = playlist {
            let (clause, a) = member_clause(p);
            sql.push_str(&format!(" AND {clause}"));
            args.extend(a);
        }
        if let Some(q) = q {
            sql.push_str(" AND (title LIKE ? OR prompt LIKE ? OR IFNULL(lyrics, '') LIKE ? OR IFNULL(mood, '') LIKE ? OR IFNULL(genre, '') LIKE ?)");
            let pat = format!("%{q}%");
            for _ in 0..5 {
                args.push(pat.clone().into());
            }
        }
        if let Some(m) = mood {
            sql.push_str(" AND mood = ?");
            args.push(m.to_string().into());
        }
        if let Some(f) = fav {
            sql.push_str(" AND favourite = ?");
            args.push(i64::from(f).into());
        }
        sql.push_str(match sort {
            Sort::Newest => " ORDER BY rowid DESC",
            Sort::Played => " ORDER BY (SELECT MAX(started_at) FROM history h WHERE h.track_id = tracks.id) DESC NULLS LAST, rowid DESC",
            Sort::Title => " ORDER BY title COLLATE NOCASE, rowid",
        });
        sql.push_str(" LIMIT ? OFFSET ?");
        args.push(i64::from(limit).into());
        args.push(i64::from(offset).into());
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| track_at(r, 0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let next = (rows.len() as u32 == limit).then_some(offset + limit);
        Ok((rows, next))
    }

    /// When each of `ids` last started playing on any radio.
    pub fn last_played(&self, ids: &[String]) -> rusqlite::Result<std::collections::HashMap<String, String>> {
        let ids = to_json(&ids)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT track_id, MAX(started_at) FROM history WHERE track_id IN (SELECT value FROM json_each(?1)) GROUP BY track_id")?;
        let rows = stmt.query_map([ids], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
        rows
    }

    /// The total length of what a playlist plays, in seconds.
    pub fn member_seconds(&self, p: &Playlist, cap: Option<u32>) -> rusqlite::Result<f64> {
        Ok(self.playlist_tracks(p, cap, u32::MAX, 0)?.iter().map(|t| t.seconds).sum())
    }

    pub fn set_favourite(&self, id: &str, fav: bool) -> rusqlite::Result<bool> {
        let n = self.conn.lock().unwrap()
            .execute("UPDATE tracks SET favourite = ?2 WHERE id = ?1 AND deleted = 0", params![id, fav])?;
        Ok(n > 0)
    }

    /// A person's title (SPEC 10, Titles): never overwritten by any other layer.
    pub fn rename_track(&self, id: &str, title: &str) -> rusqlite::Result<bool> {
        let n = self.conn.lock().unwrap().execute(
            "UPDATE tracks SET title = ?2, title_source = 'person', title_engine = NULL WHERE id = ?1 AND deleted = 0",
            params![id, title],
        )?;
        Ok(n > 0)
    }

    /// Whether a live track already has this title, ignoring case.
    pub fn title_taken(&self, title: &str) -> rusqlite::Result<bool> {
        self.conn.lock().unwrap()
            .prepare("SELECT 1 FROM tracks WHERE deleted = 0 AND title = ?1 COLLATE NOCASE")?
            .exists([title])
    }

    /// Library pick (SPEC 6): the playlist's tracks first, then the mood, then favourites, random within a rank.
    /// Skips the last `no_repeat` plays and `exclude`. With nothing else left it repeats rather than go silent.
    pub fn pick(&self, radio: &str, playlist_id: Option<&str>, mood: Option<&str>, no_repeat: u32, exclude: &[String])
        -> rusqlite::Result<Option<Track>> {
        let exclude = to_json(&exclude)?;
        let sql = format!(
            "SELECT {COLS} FROM tracks WHERE deleted = 0
               AND id NOT IN (SELECT track_id FROM history WHERE radio = ?5 ORDER BY id DESC LIMIT ?3)
               AND id NOT IN (SELECT value FROM json_each(?4))
             ORDER BY id IN (SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1) DESC, IFNULL(mood = ?2, 0) DESC, favourite DESC, RANDOM()
             LIMIT 1"
        );
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let first = stmt.query_row(params![playlist_id, mood, no_repeat, exclude, radio], |r| track_at(r, 0)).optional()?;
        if first.is_some() || no_repeat == 0 {
            return Ok(first);
        }
        stmt.query_row(params![playlist_id, mood, 0, exclude, radio], |r| track_at(r, 0)).optional()
    }

    pub fn play_started(&self, track_id: &str, title: &str, radio: &str) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO history (track_id, title, started_at, radio) VALUES (?1, ?2, ?3, ?4)", params![track_id, title, now(), radio])?;
        Ok(conn.last_insert_rowid())
    }

    pub fn play_ended(&self, id: i64, skipped: bool) -> rusqlite::Result<()> {
        self.conn.lock().unwrap()
            .execute("UPDATE history SET ended_at = ?2, skipped = ?3 WHERE id = ?1", params![id, now(), skipped])?;
        Ok(())
    }

    pub fn history(&self, radio: &str, limit: u32, cursor: Option<i64>) -> rusqlite::Result<(Vec<Play>, Option<i64>)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
"SELECT h.id, h.track_id, h.title, h.started_at, h.ended_at, h.skipped, IFNULL(t.favourite, 0) FROM history h
             LEFT JOIN tracks t ON t.id = h.track_id
             WHERE h.id < IFNULL(?1, 9223372036854775807) AND h.radio = ?3 ORDER BY h.id DESC LIMIT ?2",
        )?;
        let plays = stmt
            .query_map(params![cursor, limit, radio], |r| {
                Ok(Play { id: r.get(0)?, track_id: r.get(1)?, title: r.get(2)?, started_at: r.get(3)?, ended_at: r.get(4)?, skipped: r.get(5)?, favourite: r.get(6)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let next = (plays.len() as u32 == limit).then(|| plays.last().map(|p| p.id)).flatten();
        Ok((plays, next))
    }

    pub fn insert_playlist(&self, p: &Playlist) -> rusqlite::Result<()> {
        let recipe = p.recipe.as_ref().map(to_json).transpose()?;
        let rule = p.rule.as_ref().map(to_json).transpose()?;
        self.conn.lock().unwrap().execute(
            &format!("INSERT INTO playlists ({PLAYLIST_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)"),
            params![p.id, p.name, p.kind.as_str(), p.order.as_str(), p.max_tracks, recipe, rule, p.created_at],
        )?;
        Ok(())
    }

    /// A stored playlist; built-ins come from `playlists::find`.
    pub fn playlist(&self, id: &str) -> rusqlite::Result<Option<Playlist>> {
        self.conn.lock().unwrap()
            .query_row(&format!("SELECT {PLAYLIST_COLS} FROM playlists WHERE id = ?1"), [id], playlist_row)
            .optional()
    }

    /// Stored playlists, oldest first.
    pub fn playlists(&self) -> rusqlite::Result<Vec<Playlist>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!("SELECT {PLAYLIST_COLS} FROM playlists ORDER BY created_at, rowid"))?;
        let rows = stmt.query_map([], playlist_row)?.collect();
        rows
    }

    /// Replaces name, order, max_tracks, recipe and rule; the kind never changes.
    pub fn update_playlist(&self, p: &Playlist) -> rusqlite::Result<bool> {
        let recipe = p.recipe.as_ref().map(to_json).transpose()?;
        let rule = p.rule.as_ref().map(to_json).transpose()?;
        let n = self.conn.lock().unwrap().execute(
            "UPDATE playlists SET name = ?2, play_order = ?3, max_tracks = ?4, recipe = ?5, rule = ?6 WHERE id = ?1",
            params![p.id, p.name, p.order.as_str(), p.max_tracks, recipe, rule],
        )?;
        Ok(n > 0)
    }

    /// Removes the playlist and its membership; its tracks stay in the crate.
    pub fn delete_playlist(&self, id: &str) -> rusqlite::Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1", [id])?;
        let n = tx.execute("DELETE FROM playlists WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Inserts tracks at `position` (default and past the end: the end); ones already in it are skipped.
    pub fn add_to_playlist(&self, id: &str, track_ids: &[String], position: Option<usize>) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut order: Vec<String> = {
            let mut stmt = tx.prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")?;
            let rows = stmt.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            rows
        };
        let mut seen: std::collections::HashSet<String> = order.iter().cloned().collect();
        let new: Vec<String> = track_ids.iter().filter(|t| seen.insert((*t).clone())).cloned().collect();
        let at = position.unwrap_or(order.len()).min(order.len());
        order.splice(at..at, new);
        write_order(&tx, id, &order)?;
        tx.commit()
    }

    /// The whole new order.
    pub fn set_playlist_tracks(&self, id: &str, track_ids: &[String]) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        write_order(&tx, id, track_ids)?;
        tx.commit()
    }

    pub fn remove_from_playlist(&self, id: &str, track_id: &str) -> rusqlite::Result<bool> {
        Ok(self.conn.lock().unwrap().execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2", params![id, track_id])? > 0)
    }

    /// A page of a playlist's live tracks in play order: by position when stored, else newest
    /// first. `cap` bounds the whole list (smart playlists).
    pub fn playlist_tracks(&self, p: &Playlist, cap: Option<u32>, limit: u32, offset: u32) -> rusqlite::Result<Vec<Track>> {
        let (clause, mut args) = member_clause(p);
        let order = if p.kind.stored() {
            args.push(p.id.clone().into());
            "(SELECT position FROM playlist_tracks WHERE playlist_id = ? AND track_id = tracks.id)"
        } else {
            "rowid DESC"
        };
        let limit = cap.map_or(limit, |c| limit.min(c.saturating_sub(offset)));
        args.push(i64::from(limit).into());
        args.push(i64::from(offset).into());
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM tracks WHERE deleted = 0 AND {clause} ORDER BY {order} LIMIT ? OFFSET ?"))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| track_at(r, 0))?.collect();
        rows
    }

    pub fn member_ids(&self, p: &Playlist, cap: Option<u32>) -> rusqlite::Result<Vec<String>> {
        Ok(self.playlist_tracks(p, cap, u32::MAX, 0)?.into_iter().map(|t| t.id).collect())
    }

    pub fn member_count(&self, p: &Playlist, cap: Option<u32>) -> rusqlite::Result<u32> {
        let (clause, args) = member_clause(p);
        let n: u32 = self.conn.lock().unwrap().query_row(
            &format!("SELECT COUNT(*) FROM tracks WHERE deleted = 0 AND {clause}"),
            rusqlite::params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        Ok(cap.map_or(n, |c| n.min(c)))
    }

    pub fn setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn.lock().unwrap()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
    }

    /// `None` deletes the setting.
    pub fn set_setting(&self, key: &str, value: Option<&str>) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        match value {
            Some(v) => conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, v],
            )?,
            None => conn.execute("DELETE FROM settings WHERE key = ?1", [key])?,
        };
        Ok(())
    }

    /// The id of the playlist on air on `radio` (SPEC 10, On air).
    pub fn on_air(&self, radio: &str) -> rusqlite::Result<Option<String>> {
        self.setting(&format!("on_air:{radio}"))
    }

    pub fn set_on_air(&self, radio: &str, playlist: Option<&str>) -> rusqlite::Result<()> {
        self.set_setting(&format!("on_air:{radio}"), playlist)
    }

    /// The last `n` tracks played on `radio`, newest first.
    pub fn recent_track_ids(&self, radio: &str, n: u32) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT track_id FROM history WHERE radio = ?1 ORDER BY id DESC LIMIT ?2")?;
        let rows = stmt.query_map(params![radio, n], |r| r.get(0))?.collect();
        rows
    }

    pub fn radios(&self) -> rusqlite::Result<Vec<RadioRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, visibility, listen_key, created_at FROM radios ORDER BY id = 'main' DESC, created_at, rowid")?;
        let rows = stmt.query_map([], radio_row)?.collect();
        rows
    }

    pub fn insert_radio(&self, r: &RadioRow) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO radios (id, name, visibility, listen_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![r.id, r.name, r.visibility.as_str(), r.listen_key, r.created_at],
        )?;
        Ok(())
    }

    pub fn update_radio(&self, id: &str, name: Option<&str>, visibility: Option<Visibility>) -> rusqlite::Result<bool> {
        let n = self.conn.lock().unwrap().execute(
            "UPDATE radios SET name = IFNULL(?2, name), visibility = IFNULL(?3, visibility) WHERE id = ?1",
            params![id, name, visibility.map(Visibility::as_str)],
        )?;
        Ok(n > 0)
    }

    pub fn set_listen_key(&self, id: &str, key: &str) -> rusqlite::Result<bool> {
        Ok(self.conn.lock().unwrap().execute("UPDATE radios SET listen_key = ?2 WHERE id = ?1", params![id, key])? > 0)
    }

    /// Removes the radio and its on-air setting; its plays stay in history.
    pub fn delete_radio(&self, id: &str) -> rusqlite::Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let n = tx.execute("DELETE FROM radios WHERE id = ?1", [id])?;
        tx.execute("DELETE FROM settings WHERE key = ?1", [format!("on_air:{id}")])?;
        tx.commit()?;
        Ok(n > 0)
    }

    /// Retention (SPEC 6): tracks older than `days` that are not favourites, not in a fixed playlist and not in `keep`.
    pub fn expired(&self, days: u32, keep: &[String]) -> rusqlite::Result<Vec<(String, String)>> {
        let keep = to_json(&keep)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, file FROM tracks WHERE deleted = 0 AND favourite = 0
               AND created_at < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1)
               AND id NOT IN (SELECT value FROM json_each(?2)) AND id NOT IN ({IN_FIXED})
             ORDER BY created_at"
        ))?;
        let rows = stmt
            .query_map(params![format!("-{days} days"), keep], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect();
        rows
    }

    pub fn mark_deleted(&self, id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE tracks SET deleted = 1 WHERE id = ?1", [id])?;
        conn.execute("DELETE FROM playlist_tracks WHERE track_id = ?1", [id])?; // deleting from the crate removes it everywhere
        Ok(())
    }

    pub fn insert_user(&self, u: &User) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            &format!("INSERT INTO users ({USER_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7)"),
            params![u.username, u.name, u.role, u.hash, u.must_change, u.created_at, u.last_seen_at],
        )?;
        Ok(())
    }

    pub fn user(&self, username: &str) -> rusqlite::Result<Option<User>> {
        self.conn.lock().unwrap()
            .query_row(&format!("SELECT {USER_COLS} FROM users WHERE username = ?1"), [username], |r| user_at(r, 0))
            .optional()
    }

    pub fn users(&self) -> rusqlite::Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!("SELECT {USER_COLS} FROM users ORDER BY username"))?;
        let rows = stmt.query_map([], |r| user_at(r, 0))?.collect();
        rows
    }

    pub fn update_user(&self, username: &str, name: Option<&str>, role: Option<&str>) -> rusqlite::Result<bool> {
        let n = self.conn.lock().unwrap().execute(
            "UPDATE users SET name = IFNULL(?2, name), role = IFNULL(?3, role) WHERE username = ?1",
            params![username, name, role],
        )?;
        Ok(n > 0)
    }

    pub fn set_password(&self, username: &str, hash: &str, must_change: bool) -> rusqlite::Result<bool> {
        let n = self.conn.lock().unwrap()
            .execute("UPDATE users SET hash = ?2, must_change = ?3 WHERE username = ?1", params![username, hash, must_change])?;
        Ok(n > 0)
    }

    pub fn touch_user(&self, username: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute("UPDATE users SET last_seen_at = ?2 WHERE username = ?1", params![username, now()])?;
        Ok(())
    }

    /// Removes the user and every session they had.
    pub fn delete_user(&self, username: &str) -> rusqlite::Result<bool> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM sessions WHERE username = ?1", [username])?;
        let n = tx.execute("DELETE FROM users WHERE username = ?1", [username])?;
        tx.commit()?;
        Ok(n > 0)
    }

    pub fn count_admins(&self) -> rusqlite::Result<u32> {
        self.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM users WHERE role = 'admin'", [], |r| r.get(0))
    }

    pub fn create_session(&self, id: &str, username: &str, expires_at: i64, remember: bool) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO sessions (id, username, expires_at, remember) VALUES (?1, ?2, ?3, ?4)",
            params![id, username, expires_at, remember],
        )?;
        Ok(())
    }

    /// The live session `id` and its user; `None` when unknown, expired, or the user is gone.
    pub fn session_user(&self, id: &str, now: i64) -> rusqlite::Result<Option<(User, Session)>> {
        let cols = USER_COLS.split(", ").map(|c| format!("u.{c}")).collect::<Vec<_>>().join(", ");
        self.conn.lock().unwrap()
            .query_row(
                &format!("SELECT {cols}, s.id, s.expires_at, s.remember FROM sessions s JOIN users u ON u.username = s.username
                          WHERE s.id = ?1 AND s.expires_at > ?2"),
                params![id, now],
                |r| {
                    let u = user_at(r, 0)?;
                    let s = Session { id: r.get(7)?, username: u.username.clone(), expires_at: r.get(8)?, remember: r.get(9)? };
                    Ok((u, s))
                },
            )
            .optional()
    }

    pub fn delete_session(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Ends every session of `username` except `keep`; returns how many ended.
    pub fn delete_sessions_for(&self, username: &str, keep: Option<&str>) -> rusqlite::Result<usize> {
        self.conn.lock().unwrap()
            .execute("DELETE FROM sessions WHERE username = ?1 AND id IS NOT ?2", params![username, keep])
    }

    pub fn delete_expired_sessions(&self, now: i64) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM sessions WHERE expires_at <= ?1", [now])?;
        Ok(())
    }

    /// Every live track that is not kept, not in a fixed playlist and not in `keep`: what "Empty the crate" deletes (SPEC 3).
    pub fn unkept(&self, keep: &[String]) -> rusqlite::Result<Vec<(String, String)>> {
        let keep = to_json(&keep)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, file FROM tracks WHERE deleted = 0 AND favourite = 0 AND id NOT IN (SELECT value FROM json_each(?1))
               AND id NOT IN ({IN_FIXED})"
        ))?;
        let rows = stmt.query_map([keep], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
        rows
    }

    /// Bytes on disk of every live track (SPEC 6, Storage).
    pub fn usage(&self) -> rusqlite::Result<i64> {
        self.conn.lock().unwrap().query_row("SELECT IFNULL(SUM(bytes), 0) FROM tracks WHERE deleted = 0", [], |r| r.get(0))
    }

    /// What the budget may delete, first to last: not kept, not in a fixed playlist and not in `keep`, least recently played
    /// first (a never-played track counts from when it was made). Returns (id, file, bytes).
    pub fn eviction_order(&self, keep: &[String]) -> rusqlite::Result<Vec<(String, String, i64)>> {
        let keep = to_json(&keep)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT t.id, t.file, t.bytes FROM tracks t
             LEFT JOIN (SELECT track_id, MAX(started_at) AS last FROM history GROUP BY track_id) h ON h.track_id = t.id
             WHERE t.deleted = 0 AND t.favourite = 0 AND t.id NOT IN (SELECT value FROM json_each(?1)) AND t.id NOT IN ({IN_FIXED})
             ORDER BY IFNULL(h.last, t.created_at), t.rowid"
        ))?;
        let rows = stmt.query_map([keep], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect();
        rows
    }

    pub fn tracks_without_bytes(&self) -> rusqlite::Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, file FROM tracks WHERE deleted = 0 AND bytes = 0")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
        rows
    }

    pub fn set_bytes(&self, id: &str, bytes: i64) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute("UPDATE tracks SET bytes = ?2 WHERE id = ?1", params![id, bytes])?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sample(id: &str, mood: Option<&str>, made_by: Option<&str>) -> Track {
        Track {
            id: id.into(),
            title: format!("title {id}"),
            file: format!("/data/tracks/{id}.flac"),
            prompt: "calm mood, instrumental".into(),
            mood: mood.map(Into::into),
            genre: None,
            energy: None,
            engine: "fake".into(),
            licence: "CC0-1.0".into(),
            commercial: "yes".into(),
            seconds: 180.0,
            bpm: Some(75),
            vocals: false,
            seed: 1,
            favourite: false,
            made_by: made_by.map(Into::into),
            requested_by: "me".into(),
            created_at: now(),
            bytes: 1_000,
            title_source: "words".into(),
            artist: "Maio".into(),
            album: "Requests".into(),
            key: None,
            time_signature: None,
            lyrics: None,
            vocal_language: None,
            description: None,
            lyrics_engine: None,
            title_engine: None,
        }
    }

    fn user(name: &str, role: &str) -> User {
        User { username: name.into(), name: name.to_uppercase(), role: role.into(), hash: format!("hash-{name}"), must_change: false, created_at: now(), last_seen_at: None }
    }

    #[test]
    fn users_roundtrip_update_and_count_admins() {
        let s = Store::open_in_memory().unwrap();
        s.insert_user(&user("ana", "admin")).unwrap();
        s.insert_user(&user("bo", "listener")).unwrap();
        assert!(s.insert_user(&user("ana", "operator")).is_err(), "usernames are unique");
        assert_eq!(s.count_admins().unwrap(), 1);
        assert!(s.update_user("bo", Some("Bo B"), Some("admin")).unwrap());
        assert_eq!(s.count_admins().unwrap(), 2);
        assert!(!s.update_user("zed", None, Some("admin")).unwrap());
        assert!(s.set_password("bo", "newhash", true).unwrap());
        let bo = s.user("bo").unwrap().unwrap();
        assert_eq!((bo.name.as_str(), bo.hash.as_str(), bo.must_change), ("Bo B", "newhash", true));
        assert_eq!(s.users().unwrap().len(), 2);
    }

    #[test]
    fn sessions_expire_and_die_with_their_user() {
        let s = Store::open_in_memory().unwrap();
        s.insert_user(&user("ana", "admin")).unwrap();
        s.create_session("live", "ana", 1_000, true).unwrap();
        s.create_session("old", "ana", 10, false).unwrap();
        let (u, sess) = s.session_user("live", 500).unwrap().unwrap();
        assert_eq!((u.username.as_str(), sess.remember), ("ana", true));
        assert!(s.session_user("old", 500).unwrap().is_none(), "expired");
        assert!(s.session_user("nope", 500).unwrap().is_none());
        s.delete_expired_sessions(500).unwrap();
        s.create_session("other", "ana", 1_000, false).unwrap();
        assert_eq!(s.delete_sessions_for("ana", Some("live")).unwrap(), 1, "keeps the given session");
        assert!(s.session_user("live", 500).unwrap().is_some());
        assert!(s.delete_user("ana").unwrap());
        assert!(s.session_user("live", 500).unwrap().is_none(), "removing a user ends their sessions");
    }

    #[test]
    fn main_radio_exists_and_radios_can_be_added_changed_and_removed() {
        let s = Store::open_in_memory().unwrap();
        let all = s.radios().unwrap();
        assert_eq!((all.len(), all[0].id.as_str(), all[0].visibility), (1, "main", Visibility::Public));
        assert_eq!(all[0].listen_key.len(), 32);
        let lobby = RadioRow { id: "lobby".into(), name: "Lobby".into(), visibility: Visibility::Private, listen_key: "k".repeat(32), created_at: now() };
        s.insert_radio(&lobby).unwrap();
        assert!(s.insert_radio(&lobby).is_err(), "ids are unique");
        assert!(s.update_radio("lobby", Some("Front desk"), Some(Visibility::Public)).unwrap());
        assert!(s.set_listen_key("lobby", &"z".repeat(32)).unwrap());
        let row = s.radios().unwrap().into_iter().find(|r| r.id == "lobby").unwrap();
        assert_eq!((row.name.as_str(), row.visibility, row.listen_key.as_str()), ("Front desk", Visibility::Public, "z".repeat(32).as_str()));
        s.set_on_air("lobby", Some("pl_1")).unwrap();
        assert!(s.delete_radio("lobby").unwrap());
        assert!(s.on_air("lobby").unwrap().is_none(), "its on-air setting goes with it");
        assert!(!s.delete_radio("lobby").unwrap());
    }

    #[test]
    fn history_and_no_repeat_are_per_radio() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("a", None, None)).unwrap();
        s.play_started("a", "A", "main").unwrap();
        assert_eq!(s.history("main", 10, None).unwrap().0.len(), 1);
        assert!(s.history("lobby", 10, None).unwrap().0.is_empty());
        assert!(s.pick("main", None, None, 5, &[]).unwrap().is_some(), "only track left: it repeats rather than go silent");
        assert_eq!(s.pick("lobby", None, None, 5, &[]).unwrap().unwrap().id, "a");
    }

    #[test]
    fn an_old_database_gains_radios_history_radio_and_keeps_its_station() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE history (id INTEGER PRIMARY KEY, track_id TEXT NOT NULL, title TEXT NOT NULL,
                               started_at TEXT NOT NULL, ended_at TEXT, skipped INTEGER NOT NULL DEFAULT 0);
                             INSERT INTO history (track_id, title, started_at) VALUES ('t', 'T', '2026-01-01T00:00:00Z');
                             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                             INSERT INTO settings VALUES ('active_station', 'st_9');").unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.history("main", 10, None).unwrap().0.len(), 1, "old plays belong to main");
        assert_eq!(s.on_air("main").unwrap().as_deref(), Some("st_9"));
        assert!(s.setting("active_station:main").unwrap().is_none());
        assert_eq!(s.radios().unwrap()[0].id, "main");
    }

    #[test]
    fn old_token_tables_are_dropped_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE tokens (name TEXT); CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                             INSERT INTO settings VALUES ('token_removed:x', 'y');").unwrap();
        }
        let s = Store::open(&path).unwrap();
        let c = s.conn.lock().unwrap();
        assert!(!c.prepare("SELECT 1 FROM sqlite_master WHERE name = 'tokens'").unwrap().exists([]).unwrap());
        assert!(!c.prepare("SELECT 1 FROM settings WHERE key LIKE 'token_removed:%'").unwrap().exists([]).unwrap());
    }

    #[test]
    fn usage_counts_live_tracks_only() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&Track { bytes: 300, ..sample("a", None, None) }).unwrap();
        s.insert_track(&Track { bytes: 700, ..sample("b", None, None) }).unwrap();
        assert_eq!(s.usage().unwrap(), 1_000);
        s.mark_deleted("a").unwrap();
        assert_eq!(s.usage().unwrap(), 700);
    }

    #[test]
    fn eviction_goes_least_recently_played_first_and_spares_kept_and_queued() {
        let s = Store::open_in_memory().unwrap();
        for id in ["old_unplayed", "played_long_ago", "played_recently", "kept", "queued"] {
            s.insert_track(&sample(id, None, None)).unwrap();
        }
        let conn = s.conn.lock().unwrap();
        conn.execute("UPDATE tracks SET created_at = '2026-01-01T00:00:00Z' WHERE id = 'old_unplayed'", []).unwrap();
        conn.execute("UPDATE tracks SET created_at = '2025-06-01T00:00:00Z' WHERE id IN ('played_long_ago', 'played_recently')", []).unwrap();
        conn.execute("INSERT INTO history (track_id, title, started_at) VALUES ('played_long_ago', 't', '2026-02-01T00:00:00Z'), ('played_recently', 't', '2026-10-01T00:00:00Z')", []).unwrap();
        drop(conn);
        s.set_favourite("kept", true).unwrap();
        let order: Vec<String> = s.eviction_order(&["queued".to_string()]).unwrap().into_iter().map(|(id, _, _)| id).collect();
        assert_eq!(order[..3], ["old_unplayed", "played_long_ago", "played_recently"]);
        assert!(!order.contains(&"kept".to_string()) && !order.contains(&"queued".to_string()));
    }

    #[test]
    fn sizes_can_be_backfilled() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&Track { bytes: 0, ..sample("a", None, None) }).unwrap();
        assert_eq!(s.tracks_without_bytes().unwrap(), vec![("a".to_string(), "/data/tracks/a.flac".to_string())]);
        s.set_bytes("a", 4_200).unwrap();
        assert!(s.tracks_without_bytes().unwrap().is_empty());
        assert_eq!(s.usage().unwrap(), 4_200);
    }

    #[test]
    fn an_old_database_gains_the_bytes_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        Connection::open(&path).unwrap().execute_batch(
            "CREATE TABLE tracks (id TEXT PRIMARY KEY, title TEXT NOT NULL, file TEXT NOT NULL, prompt TEXT NOT NULL,
             mood TEXT, genre TEXT, energy TEXT, engine TEXT NOT NULL, licence TEXT NOT NULL, commercial TEXT NOT NULL,
             seconds REAL NOT NULL, bpm INTEGER, vocals INTEGER NOT NULL, seed INTEGER NOT NULL, favourite INTEGER NOT NULL DEFAULT 0,
             station_id TEXT, requested_by TEXT NOT NULL, created_at TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);",
        ).unwrap();
        let s = Store::open(&path).unwrap();
        s.insert_track(&sample("a", None, None)).unwrap();
        assert_eq!(s.track("a").unwrap().unwrap().bytes, 1_000);
        assert_eq!(s.track("a").unwrap().unwrap().made_by, None, "station_id became made_by");
    }

    #[test]
    fn insert_get_and_favourite() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("a", Some("calm"), None)).unwrap();
        assert_eq!(s.track("a").unwrap().unwrap().title, "title a");
        assert!(s.track("zzz").unwrap().is_none());
        assert!(s.set_favourite("a", true).unwrap());
        assert!(!s.set_favourite("zzz", true).unwrap());
        assert!(s.track("a").unwrap().unwrap().favourite);
    }

    #[test]
    fn list_filters_and_pages() {
        let s = Store::open_in_memory().unwrap();
        for i in 0..5 {
            s.insert_track(&sample(&format!("t{i}"), Some(if i % 2 == 0 { "calm" } else { "tense" }), None)).unwrap();
        }
        s.set_favourite("t1", true).unwrap();
        let (calm, _) = s.tracks(&TrackFilter { mood: Some("calm"), ..Default::default() }, Sort::Newest, 50, 0).unwrap();
        assert_eq!(calm.len(), 3);
        let (fav, _) = s.tracks(&TrackFilter { fav: Some(true), ..Default::default() }, Sort::Newest, 50, 0).unwrap();
        assert_eq!(fav.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["t1"]);
        let (q, _) = s.tracks(&TrackFilter { q: Some("title t3"), ..Default::default() }, Sort::Newest, 50, 0).unwrap();
        assert_eq!(q.len(), 1);
        let (page1, cursor) = s.tracks(&TrackFilter::default(), Sort::Newest, 2, 0).unwrap();
        assert_eq!(cursor, Some(2));
        assert_eq!(page1.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["t4", "t3"]);
        let (page2, _) = s.tracks(&TrackFilter::default(), Sort::Newest, 2, cursor.unwrap()).unwrap();
        assert_eq!(page2.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["t2", "t1"]);
    }

    #[test]
    fn filters_by_side() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("asked", None, None)).unwrap();
        s.insert_track(&sample("st1a", None, Some("st1"))).unwrap();
        s.insert_track(&sample("st2a", None, Some("st2"))).unwrap();
        let ids = |f: TrackFilter| s.tracks(&f, Sort::Newest, 50, 0).unwrap().0.into_iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(ids(TrackFilter { side: Some(Side::A), ..Default::default() }), ["asked"]);
        assert_eq!(ids(TrackFilter { side: Some(Side::B), ..Default::default() }), ["st2a", "st1a"]);
    }

    #[test]
    fn pick_prefers_the_playlist_then_mood_then_favourite() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("mood", Some("calm"), None)).unwrap();
        s.insert_track(&sample("fav", None, None)).unwrap();
        s.insert_track(&sample("station", None, Some("st1"))).unwrap();
        s.insert_playlist(&pl("st1", Kind::Generative)).unwrap();
        s.add_to_playlist("st1", &["station".into()], None).unwrap();
        s.set_favourite("fav", true).unwrap();
        assert_eq!(s.pick("main", Some("st1"), Some("calm"), 0, &[]).unwrap().unwrap().id, "station");
        assert_eq!(s.pick("main", None, Some("calm"), 0, &[]).unwrap().unwrap().id, "mood");
        assert_eq!(s.pick("main", None, None, 0, &[]).unwrap().unwrap().id, "fav");
    }

    #[test]
    fn pick_skips_recent_plays_but_repeats_rather_than_going_silent() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("a", None, None)).unwrap();
        s.insert_track(&sample("b", None, None)).unwrap();
        s.play_started("a", "title a", "main").unwrap();
        assert_eq!(s.pick("main", None, None, 20, &[]).unwrap().unwrap().id, "b");
        s.play_started("b", "title b", "main").unwrap();
        assert!(s.pick("main", None, None, 20, &[]).unwrap().is_some(), "falls back to a repeat");
    }

    #[test]
    fn pick_never_returns_excluded_tracks() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("a", None, None)).unwrap();
        assert!(s.pick("main", None, None, 0, &["a".to_string()]).unwrap().is_none());
        assert!(Store::open_in_memory().unwrap().pick("main", None, None, 20, &[]).unwrap().is_none());
    }

    #[test]
    fn history_records_plays() {
        let s = Store::open_in_memory().unwrap();
        let id = s.play_started("a", "title a", "main").unwrap();
        s.play_ended(id, true).unwrap();
        let (plays, _) = s.history("main", 10, None).unwrap();
        assert_eq!(plays.len(), 1);
        assert!(plays[0].skipped && plays[0].ended_at.is_some());
    }

    #[test]
    fn expired_keeps_favourites_recent_and_queued_tracks() {
        let s = Store::open_in_memory().unwrap();
        for id in ["old", "oldfav", "oldqueued", "new"] {
            s.insert_track(&sample(id, None, None)).unwrap();
        }
        s.set_favourite("oldfav", true).unwrap();
        s.conn.lock().unwrap()
            .execute("UPDATE tracks SET created_at = '2020-01-01T00:00:00Z' WHERE id LIKE 'old%'", [])
            .unwrap();
        let expired = s.expired(7, &["oldqueued".to_string()]).unwrap();
        assert_eq!(expired, vec![("old".to_string(), "/data/tracks/old.flac".to_string())]);
        s.mark_deleted("old").unwrap();
        assert!(s.track("old").unwrap().is_none());
        assert!(s.pick("main", None, None, 0, &["new".into(), "oldfav".into(), "oldqueued".into()]).unwrap().is_none());
    }

    use crate::playlists::{Kind, Order, Playlist, Rule};

    fn pl(id: &str, kind: Kind) -> Playlist {
        Playlist { id: id.into(), name: format!("name {id}"), kind, order: Order::InOrder, max_tracks: None, recipe: None, rule: None, created_at: now() }
    }
    fn ids(v: Vec<Track>) -> Vec<String> {
        v.into_iter().map(|t| t.id).collect()
    }

    #[test]
    fn playlists_roundtrip_update_and_delete_drops_membership() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("a", None, None)).unwrap();
        let mut p = pl("pl_1", Kind::Generative);
        p.recipe = Some(serde_json::from_value(serde_json::json!({"mood": "calm", "seconds": [20, 30]})).unwrap());
        s.insert_playlist(&p).unwrap();
        assert_eq!(s.playlist("pl_1").unwrap().unwrap(), p);
        assert!(s.insert_playlist(&p).is_err(), "ids are unique");
        p.name = "renamed".into();
        p.max_tracks = Some(9);
        assert!(s.update_playlist(&p).unwrap());
        assert_eq!(s.playlists().unwrap(), vec![p.clone()]);
        s.add_to_playlist("pl_1", &["a".into()], None).unwrap();
        assert!(s.delete_playlist("pl_1").unwrap());
        assert!(!s.delete_playlist("pl_1").unwrap());
        assert_eq!(s.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM playlist_tracks", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert!(s.track("a").unwrap().is_some(), "its tracks stay in the crate");
    }

    #[test]
    fn adding_keeps_order_skips_duplicates_and_clamps_position() {
        let s = Store::open_in_memory().unwrap();
        for id in ["a", "b", "c", "d"] {
            s.insert_track(&sample(id, None, None)).unwrap();
        }
        let p = pl("pl_f", Kind::Fixed);
        s.insert_playlist(&p).unwrap();
        s.add_to_playlist("pl_f", &["a".into(), "b".into(), "a".into()], None).unwrap();
        s.add_to_playlist("pl_f", &["c".into()], Some(1)).unwrap();
        s.add_to_playlist("pl_f", &["d".into(), "b".into()], Some(99)).unwrap();
        assert_eq!(s.member_ids(&p, None).unwrap(), ["a", "c", "b", "d"]);
        assert_eq!(s.member_count(&p, None).unwrap(), 4);
        s.set_playlist_tracks("pl_f", &["d".into(), "a".into()]).unwrap();
        assert_eq!(s.member_ids(&p, None).unwrap(), ["d", "a"]);
        assert!(s.remove_from_playlist("pl_f", "d").unwrap());
        assert!(!s.remove_from_playlist("pl_f", "d").unwrap());
        assert_eq!(ids(s.playlist_tracks(&p, None, 10, 0).unwrap()), ["a"]);
    }

    #[test]
    fn builtins_and_smart_rules_select_from_the_crate_newest_first() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("asked", Some("calm"), None)).unwrap();
        s.insert_track(&Track { seconds: 500.0, ..sample("made", Some("calm"), Some("pl_g")) }).unwrap();
        s.insert_track(&sample("tense", Some("tense"), Some("pl_g"))).unwrap();
        s.set_favourite("asked", true).unwrap();
        let all = |p: &Playlist| s.member_ids(p, p.cap(50)).unwrap();
        assert_eq!(all(&Playlist::builtin("crate").unwrap()), ["tense", "made", "asked"]);
        assert_eq!(all(&Playlist::builtin("kept").unwrap()), ["asked"]);
        assert_eq!(all(&Playlist::builtin("requests").unwrap()), ["asked"]);
        let smart = |rule: Rule| Playlist { rule: Some(rule), ..pl("pl_s", Kind::Smart) };
        assert_eq!(all(&smart(Rule { mood: Some("calm".into()), ..Default::default() })), ["made", "asked"]);
        assert_eq!(all(&smart(Rule { side: Some("b".into()), max_seconds: Some(300), ..Default::default() })), ["tense"]);
        assert_eq!(all(&smart(Rule { made_within_days: Some(1), kept: Some(false), ..Default::default() })), ["tense", "made"]);
        assert_eq!(all(&Playlist { max_tracks: Some(2), ..smart(Rule::default()) }), ["tense", "made"], "at most max_tracks, newest first");
        let fixed = pl("pl_f", Kind::Fixed);
        s.insert_playlist(&fixed).unwrap();
        s.add_to_playlist("pl_f", &["asked".into()], None).unwrap();
        assert_eq!(all(&smart(Rule { from_playlist: Some("pl_f".into()), ..Default::default() })), ["asked"]);
        let (page, _) = s.tracks(&TrackFilter { playlist: Some(&fixed), ..Default::default() }, Sort::Newest, 50, 0).unwrap();
        assert_eq!(ids(page), ["asked"], "the crate filters by playlist");
        let crate_ = Playlist::builtin("crate").unwrap();
        assert_eq!(ids(s.playlist_tracks(&crate_, None, 2, 1).unwrap()), ["made", "asked"], "pages by offset");
    }

    #[test]
    fn deleting_a_track_removes_it_from_every_playlist() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&sample("a", None, None)).unwrap();
        for id in ["pl_1", "pl_2"] {
            s.insert_playlist(&pl(id, Kind::Fixed)).unwrap();
            s.add_to_playlist(id, &["a".into()], None).unwrap();
        }
        s.mark_deleted("a").unwrap();
        assert_eq!(s.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM playlist_tracks", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }

    #[test]
    fn storage_spares_tracks_in_fixed_playlists_only() {
        let s = Store::open_in_memory().unwrap();
        for id in ["in_fixed", "in_generative", "loose"] {
            s.insert_track(&sample(id, None, None)).unwrap();
        }
        s.conn.lock().unwrap().execute("UPDATE tracks SET created_at = '2020-01-01T00:00:00Z'", []).unwrap();
        s.insert_playlist(&pl("pl_f", Kind::Fixed)).unwrap();
        s.insert_playlist(&pl("pl_g", Kind::Generative)).unwrap();
        s.add_to_playlist("pl_f", &["in_fixed".into()], None).unwrap();
        s.add_to_playlist("pl_g", &["in_generative".into()], None).unwrap();
        let first = |v: Vec<(String, String)>| v.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        assert_eq!(first(s.expired(7, &[]).unwrap()), ["in_generative", "loose"]);
        assert_eq!(first(s.unkept(&[]).unwrap()).len(), 2);
        assert!(!s.eviction_order(&[]).unwrap().iter().any(|(id, _, _)| id == "in_fixed"));
    }

    #[test]
    fn stations_become_generative_playlists_with_their_tracks_and_on_air_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(r#"
                CREATE TABLE tracks (id TEXT PRIMARY KEY, title TEXT NOT NULL, file TEXT NOT NULL, prompt TEXT NOT NULL,
                  mood TEXT, genre TEXT, energy TEXT, engine TEXT NOT NULL, licence TEXT NOT NULL, commercial TEXT NOT NULL,
                  seconds REAL NOT NULL, bpm INTEGER, vocals INTEGER NOT NULL, seed INTEGER NOT NULL, favourite INTEGER NOT NULL DEFAULT 0,
                  station_id TEXT, requested_by TEXT NOT NULL, created_at TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
                INSERT INTO tracks VALUES ('t1','T','/f','p',NULL,NULL,NULL,'e','l','yes',3,NULL,0,1,0,'st_1','x','2026-01-01T00:00:00Z',0);
                INSERT INTO tracks VALUES ('t2','T','/f','p',NULL,NULL,NULL,'e','l','yes',3,NULL,0,1,0,'st_1','x','2026-01-01T00:00:00Z',1);
                INSERT INTO tracks VALUES ('t3','T','/f','p',NULL,NULL,NULL,'e','l','yes',3,NULL,0,1,0,'st_1','x','2026-01-01T00:00:00Z',0);
                CREATE TABLE stations (id TEXT PRIMARY KEY, body TEXT NOT NULL);
                INSERT INTO stations VALUES ('st_1', '{"id":"st_1","name":"late night","prompt":null,"mood":"calm","genre":null,"energy":null,"seconds":[150,240],"tempo":null,"variations":[],"library_share":0.25,"engine":null}');
                CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                INSERT INTO settings VALUES ('active_station', 'st_1');
            "#).unwrap();
        }
        let s = Store::open(&path).unwrap();
        let p = s.playlist("st_1").unwrap().unwrap();
        assert_eq!((p.name.as_str(), p.kind, p.order, p.max_tracks), ("late night", Kind::Generative, Order::Shuffle, None));
        assert_eq!(p.recipe.unwrap().mood.as_deref(), Some("calm"));
        assert_eq!(s.member_ids(&s.playlist("st_1").unwrap().unwrap(), None).unwrap(), ["t1", "t3"], "live tracks, oldest first");
        assert_eq!(s.on_air("main").unwrap().as_deref(), Some("st_1"));
        assert!(s.setting("active_station:main").unwrap().is_none());
        let c = s.conn.lock().unwrap();
        assert!(!c.prepare("SELECT 1 FROM sqlite_master WHERE name = 'stations'").unwrap().exists([]).unwrap());
    }

    #[test]
    fn an_old_database_gains_song_metadata_with_artist_and_album_from_its_playlist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        Connection::open(&path).unwrap().execute_batch(
            "CREATE TABLE tracks (id TEXT PRIMARY KEY, title TEXT NOT NULL, file TEXT NOT NULL, prompt TEXT NOT NULL,
             mood TEXT, genre TEXT, energy TEXT, engine TEXT NOT NULL, licence TEXT NOT NULL, commercial TEXT NOT NULL,
             seconds REAL NOT NULL, bpm INTEGER, vocals INTEGER NOT NULL, seed INTEGER NOT NULL, favourite INTEGER NOT NULL DEFAULT 0,
             made_by TEXT, requested_by TEXT NOT NULL, created_at TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0, bytes INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE playlists (id TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, play_order TEXT NOT NULL,
               max_tracks INTEGER, recipe TEXT, rule TEXT, created_at TEXT NOT NULL);
             INSERT INTO playlists VALUES ('pl_1', 'Late night', 'generative', 'shuffle', NULL, NULL, NULL, '2026-01-01T00:00:00Z');
             INSERT INTO tracks (id, title, file, prompt, engine, licence, commercial, seconds, vocals, seed, made_by, requested_by, created_at)
               VALUES ('made', 'late night', '/f', 'p', 'e', 'l', 'yes', 3, 0, 1, 'pl_1', 'playlist', '2026-01-01T00:00:00Z'),
                      ('asked', 'calm', '/f', 'p', 'e', 'l', 'yes', 3, 0, 1, NULL, 'user:ana', '2026-01-01T00:00:00Z');",
        ).unwrap();
        let s = Store::open(&path).unwrap();
        let made = s.track("made").unwrap().unwrap();
        assert_eq!((made.artist.as_str(), made.album.as_str(), made.title_source.as_str()), ("Late night", "Late night", "words"));
        let asked = s.track("asked").unwrap().unwrap();
        assert_eq!((asked.artist.as_str(), asked.album.as_str()), ("Maio", "Requests"));
        assert_eq!((asked.lyrics, asked.key), (None, None));
    }

    #[test]
    fn renaming_marks_the_title_as_a_persons_and_titles_can_be_looked_up() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&Track { title_engine: Some("claude".into()), title_source: "engine".into(), ..sample("a", None, None) }).unwrap();
        assert!(s.title_taken("TITLE A").unwrap(), "case does not matter");
        assert!(s.rename_track("a", "Night Bus").unwrap());
        let t = s.track("a").unwrap().unwrap();
        assert_eq!((t.title.as_str(), t.title_source.as_str(), t.title_engine), ("Night Bus", "person", None));
        assert!(!s.title_taken("title a").unwrap());
        assert!(!s.rename_track("zzz", "x").unwrap());
        s.mark_deleted("a").unwrap();
        assert!(!s.title_taken("Night Bus").unwrap(), "deleted tracks free their titles");
    }

    #[test]
    fn a_metadata_migration_cut_short_finishes_on_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("half.db");
        Connection::open(&path).unwrap().execute_batch(
            "CREATE TABLE tracks (id TEXT PRIMARY KEY, title TEXT NOT NULL, file TEXT NOT NULL, prompt TEXT NOT NULL,
             mood TEXT, genre TEXT, energy TEXT, engine TEXT NOT NULL, licence TEXT NOT NULL, commercial TEXT NOT NULL,
             seconds REAL NOT NULL, bpm INTEGER, vocals INTEGER NOT NULL, seed INTEGER NOT NULL, favourite INTEGER NOT NULL DEFAULT 0,
             made_by TEXT, requested_by TEXT NOT NULL, created_at TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0, bytes INTEGER NOT NULL DEFAULT 0,
             title_source TEXT NOT NULL DEFAULT 'words');",
        ).unwrap();
        let s = Store::open(&path).expect("a half-done migration does not block startup");
        s.insert_track(&sample("a", None, None)).unwrap();
        assert_eq!(s.track("a").unwrap().unwrap().artist, "Maio");
    }

    #[test]
    fn the_crate_sorts_by_newest_last_played_or_title_and_pages_by_offset() {
        let s = Store::open_in_memory().unwrap();
        for (id, title) in [("a", "Cedar"), ("b", "apple"), ("c", "Birch")] {
            s.insert_track(&Track { title: title.into(), ..sample(id, None, None) }).unwrap();
        }
        let conn = s.conn.lock().unwrap();
        conn.execute("INSERT INTO history (track_id, title, started_at) VALUES ('a', 'Cedar', '2026-10-01T00:00:00Z'), ('c', 'Birch', '2026-10-02T00:00:00Z')", []).unwrap();
        drop(conn);
        let ids = |sort, limit, offset| s.tracks(&TrackFilter::default(), sort, limit, offset).unwrap();
        assert_eq!(ids(Sort::Newest, 50, 0).0.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
        assert_eq!(ids(Sort::Played, 50, 0).0.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["c", "a", "b"], "never played last");
        assert_eq!(ids(Sort::Title, 50, 0).0.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["b", "c", "a"], "case does not matter");
        let (page, next) = ids(Sort::Title, 2, 0);
        assert_eq!((page.len(), next), (2, Some(2)));
        let (page, next) = ids(Sort::Title, 2, 2);
        assert_eq!((page[0].id.as_str(), next), ("a", None));
        let last = s.last_played(&["a".into(), "b".into()]).unwrap();
        assert_eq!((last.get("a").map(String::as_str), last.get("b")), (Some("2026-10-01T00:00:00Z"), None));
    }

    #[test]
    fn search_covers_lyrics_and_moods_and_plays_know_if_kept() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&Track { lyrics: Some("night bus home".into()), ..sample("sung", None, None) }).unwrap();
        s.insert_track(&sample("calm", Some("tense"), None)).unwrap();
        let found = |q| s.tracks(&TrackFilter { q: Some(q), ..Default::default() }, Sort::Newest, 50, 0).unwrap().0.into_iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(found("bus"), ["sung"]);
        assert_eq!(found("tense"), ["calm"], "moods are searched too");
        s.set_favourite("calm", true).unwrap();
        s.play_started("calm", "title calm", "main").unwrap();
        assert!(s.history("main", 10, None).unwrap().0[0].favourite);
    }

    #[test]
    fn a_playlist_knows_its_length() {
        let s = Store::open_in_memory().unwrap();
        s.insert_track(&Track { seconds: 100.0, ..sample("a", None, None) }).unwrap();
        s.insert_track(&Track { seconds: 50.5, ..sample("b", None, None) }).unwrap();
        assert_eq!(s.member_seconds(&Playlist::builtin("crate").unwrap(), None).unwrap(), 150.5);
        assert_eq!(s.member_seconds(&Playlist::builtin("crate").unwrap(), Some(1)).unwrap(), 50.5, "a cap counts the newest only");
    }
}
