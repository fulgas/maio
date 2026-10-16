//! A playlist: a named, ordered collection of songs, with its live settings (SPEC 1, 8, 9).

use serde::{Deserialize, Serialize};

use crate::{Error, PlaylistId, PlaylistRef, SongId};

pub const MAX_NAME: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        [Self::Public, Self::Private].into_iter().find(|v| v.as_str() == s)
    }
}

/// A playlist's live settings (SPEC 9). `key` is the private address's secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Live {
    pub on: bool,
    pub shuffle: bool,
    pub visibility: Visibility,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    pub song_ids: Vec<SongId>,
    pub live: Live,
    pub created_at: String,
}

fn valid_name(name: &str) -> Result<String, Error> {
    let name = name.trim();
    let n = name.chars().count();
    if n == 0 || n > MAX_NAME {
        return Err(Error::invalid("bad_name", format!("a playlist name is 1 to {MAX_NAME} characters")));
    }
    Ok(name.to_string())
}

impl Playlist {
    /// A new playlist: empty, off air, shuffle off, public.
    pub fn new(id: PlaylistId, name: &str, key: String, created_at: String) -> Result<Self, Error> {
        let live = Live { on: false, shuffle: false, visibility: Visibility::Public, key };
        Ok(Self { id, name: valid_name(name)?, song_ids: Vec::new(), live, created_at })
    }

    pub fn to_ref(&self) -> PlaylistRef {
        PlaylistRef { id: self.id.clone(), name: self.name.clone() }
    }

    pub fn rename(&mut self, name: &str) -> Result<(), Error> {
        self.name = valid_name(name)?;
        Ok(())
    }

    /// Adds songs at `position` (default the end, clamped to it). Songs already in the playlist,
    /// or repeated in `ids`, are skipped (SPEC 8). Returns how many were added.
    pub fn add(&mut self, ids: &[SongId], position: Option<usize>) -> usize {
        let mut new: Vec<SongId> = Vec::new();
        for id in ids {
            if !self.song_ids.contains(id) && !new.contains(id) {
                new.push(id.clone());
            }
        }
        let at = position.unwrap_or(self.song_ids.len()).min(self.song_ids.len());
        let added = new.len();
        self.song_ids.splice(at..at, new);
        added
    }

    pub fn remove(&mut self, song_id: &str) -> Result<(), Error> {
        let i = self.song_ids.iter().position(|s| s == song_id).ok_or(Error::NotFound("song"))?;
        self.song_ids.remove(i);
        Ok(())
    }

    /// Sets the whole order: `ids` must hold exactly this playlist's songs, each once (SPEC 10).
    pub fn reorder(&mut self, ids: Vec<SongId>) -> Result<(), Error> {
        let (mut given, mut held) = (ids.clone(), self.song_ids.clone());
        given.sort();
        held.sort();
        if given != held {
            return Err(Error::invalid("bad_order", "the new order must list every song of the playlist exactly once"));
        }
        self.song_ids = ids;
        Ok(())
    }

    /// A live playlist cannot be deleted (SPEC 1).
    pub fn ensure_deletable(&self) -> Result<(), Error> {
        if self.live.on { Err(Error::PlaylistLive) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn playlist(songs: &[&str]) -> Playlist {
        let mut p = Playlist::new("p1".into(), "Late", "k".repeat(32), "2026-10-16T12:00:00Z".into()).unwrap();
        p.song_ids = ids(songs);
        p
    }

    #[test]
    fn a_new_playlist_is_empty_off_air_and_public() {
        let p = Playlist::new("p1".into(), "  Late night  ", "k".repeat(32), "t".into()).unwrap();
        assert_eq!(p.name, "Late night");
        assert!(p.song_ids.is_empty());
        assert_eq!(p.live, Live { on: false, shuffle: false, visibility: Visibility::Public, key: "k".repeat(32) });
        assert_eq!(p.to_ref(), crate::PlaylistRef { id: "p1".into(), name: "Late night".into() });
    }

    #[test]
    fn names_are_1_to_80_characters() {
        assert_eq!(Playlist::new("p".into(), " ", "k".into(), "t".into()).unwrap_err().code(), "bad_name");
        let mut p = playlist(&[]);
        p.rename(&"é".repeat(80)).unwrap();
        assert_eq!(p.rename(&"é".repeat(81)).unwrap_err().code(), "bad_name");
        assert_eq!(p.name, "é".repeat(80));
    }

    #[test]
    fn adding_goes_to_the_end_and_skips_songs_already_in() {
        let mut p = playlist(&["a", "b"]);
        assert_eq!(p.add(&ids(&["c", "a", "c", "d"]), None), 2);
        assert_eq!(p.song_ids, ids(&["a", "b", "c", "d"]));
    }

    #[test]
    fn adding_at_a_position_inserts_there_and_clamps_past_the_end() {
        let mut p = playlist(&["a", "b"]);
        p.add(&ids(&["x"]), Some(0));
        p.add(&ids(&["y"]), Some(2));
        p.add(&ids(&["z"]), Some(99));
        assert_eq!(p.song_ids, ids(&["x", "a", "y", "b", "z"]));
    }

    #[test]
    fn removing_takes_the_song_out_or_says_not_found() {
        let mut p = playlist(&["a", "b"]);
        p.remove("a").unwrap();
        assert_eq!(p.song_ids, ids(&["b"]));
        assert_eq!(p.remove("a"), Err(Error::NotFound("song")));
    }

    #[test]
    fn a_new_order_must_list_every_song_exactly_once() {
        let mut p = playlist(&["a", "b", "c"]);
        p.reorder(ids(&["c", "a", "b"])).unwrap();
        assert_eq!(p.song_ids, ids(&["c", "a", "b"]));
        for bad in [ids(&["c", "a"]), ids(&["c", "a", "a"]), ids(&["c", "a", "b", "b"]), ids(&["c", "a", "x"])] {
            assert_eq!(p.reorder(bad).unwrap_err().code(), "bad_order");
        }
        assert_eq!(p.song_ids, ids(&["c", "a", "b"]), "a refused order changes nothing");
    }

    #[test]
    fn a_live_playlist_cannot_be_deleted() {
        let mut p = playlist(&[]);
        assert_eq!(p.ensure_deletable(), Ok(()));
        p.live.on = true;
        assert_eq!(p.ensure_deletable(), Err(Error::PlaylistLive));
    }

    #[test]
    fn visibility_round_trips_as_text() {
        for v in [Visibility::Public, Visibility::Private] {
            assert_eq!(Visibility::parse(v.as_str()), Some(v));
        }
    }
}
