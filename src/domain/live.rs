//! Live (arc42 8.4): which song a live playlist plays next, and where it can be heard.

use crate::domain::{Playlist, PlaylistRef, Visibility};

/// A live playlist's play order. In order, it loops the playlist. With shuffle, it plays every song once in
/// random order, then draws again. A song added while live joins the current round; a removed song's place
/// passes to the song after it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PlayOrder {
    /// Index in the playlist of the song last handed out (in order).
    at: usize,
    /// Songs already played this round (shuffle).
    played: Vec<String>,
    /// The song to start with, once: the one on air before a restart.
    first: Option<String>,
}

impl PlayOrder {
    /// An order that starts with `song` when it is still in the playlist.
    pub fn resuming(song: Option<String>) -> Self {
        Self { first: song, ..Self::default() }
    }

    /// The song to play after `current`; `None` for an empty playlist. `seed` drives the shuffle.
    pub fn next(&mut self, songs: &[String], shuffle: bool, current: Option<&str>, seed: u64) -> Option<String> {
        if songs.is_empty() {
            return None;
        }
        if let Some(first) = self.first.take()
            && songs.contains(&first)
        {
            return Some(first);
        }
        if !shuffle {
            self.at = match current.and_then(|c| songs.iter().position(|s| s == c)) {
                Some(i) => (i + 1) % songs.len(),
                None if current.is_none() => 0,
                None => self.at % songs.len(), // the current song was removed: its slot holds the next one
            };
            return Some(songs[self.at].clone());
        }
        if let Some(c) = current
            && !self.played.iter().any(|p| p == c)
        {
            self.played.push(c.to_string());
        }
        let mut left: Vec<&String> = songs.iter().filter(|s| !self.played.contains(s)).collect();
        if left.is_empty() {
            self.played.clear();
            left = songs.iter().filter(|s| Some(s.as_str()) != current || songs.len() == 1).collect();
        }
        Some(left[pick(seed, left.len())].clone())
    }
}

/// An index below `n` from a seed (xorshift; no rand in the domain).
fn pick(seed: u64, n: usize) -> usize {
    let mut x = seed | 1;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    (x % n as u64) as usize
}

/// Where a live playlist's HTTP streams are: `/live/{id}` when public, `/listen/{key}` when private.
pub fn stream_base(p: &Playlist) -> String {
    match p.live.visibility {
        Visibility::Public => format!("/live/{}", p.id),
        Visibility::Private => format!("/listen/{}", p.live.key),
    }
}

/// Its path on the relay (MediaMTX).
pub fn relay_path(p: &Playlist) -> String {
    stream_base(p).trim_start_matches('/').to_string()
}

/// The two kinds of stream path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// `/live/{playlist id}`: public playlists.
    Live,
    /// `/listen/{key}`: private playlists.
    Listen,
}

/// The live playlist a stream path names, if it is on air and the path matches its visibility.
pub fn by_path<'a>(playlists: &'a [Playlist], kind: PathKind, name: &str) -> Option<&'a Playlist> {
    playlists.iter().find(|p| {
        p.live.on
            && match kind {
                PathKind::Live => p.live.visibility == Visibility::Public && p.id == name,
                PathKind::Listen => p.live.visibility == Visibility::Private && p.live.key == name,
            }
    })
}

/// `Some(live playlists)` when `id` going live would pass `max` (arc42 1.2, `409 live_limit`).
pub fn limit_reached(live_now: &[Playlist], id: &str, max: usize) -> Option<Vec<PlaylistRef>> {
    let live: Vec<&Playlist> = live_now.iter().filter(|p| p.live.on).collect();
    (live.len() >= max && !live.iter().any(|p| p.id == id))
        .then(|| live.iter().map(|p| PlaylistRef { id: p.id.clone(), name: p.name.clone() }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Live, Visibility};

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("s{i}")).collect()
    }

    fn playlist(id: &str, on: bool, visibility: Visibility) -> Playlist {
        let mut p = Playlist::new(id.into(), id, "k".repeat(32), "2026-10-19T12:00:00Z".into()).unwrap();
        p.live = Live { on, shuffle: false, visibility, key: format!("key-{id}") };
        p
    }

    #[test]
    fn a_resumed_stream_starts_with_the_song_that_was_on_air() {
        let songs = ids(4);
        let mut o = PlayOrder::resuming(Some("s2".into()));
        let first = o.next(&songs, false, None, 1);
        assert_eq!(first.as_deref(), Some("s2"));
        assert_eq!(o.next(&songs, false, first.as_deref(), 1).as_deref(), Some("s3"), "then on in playlist order");
        let mut gone = PlayOrder::resuming(Some("removed".into()));
        assert_eq!(gone.next(&songs, false, None, 1).as_deref(), Some("s0"), "a song no longer in it starts from the top");
        let mut shuffled = PlayOrder::resuming(Some("s1".into()));
        assert_eq!(shuffled.next(&songs, true, None, 7).as_deref(), Some("s1"));
    }

    #[test]
    fn in_order_it_loops_the_playlist() {
        let (mut o, songs) = (PlayOrder::default(), ids(3));
        let mut cur = o.next(&songs, false, None, 1);
        let mut seen = vec![];
        for _ in 0..4 {
            seen.push(cur.clone().unwrap());
            cur = o.next(&songs, false, cur.as_deref(), 1);
        }
        assert_eq!(seen, ["s0", "s1", "s2", "s0"]);
    }

    #[test]
    fn a_removed_song_hands_its_place_to_the_next() {
        let mut o = PlayOrder::default();
        let songs = ids(4);
        assert_eq!(o.next(&songs, false, None, 1).as_deref(), Some("s0"));
        assert_eq!(o.next(&songs, false, Some("s0"), 1).as_deref(), Some("s1"));
        let without_s1 = ["s0", "s2", "s3"].map(String::from);
        assert_eq!(o.next(&without_s1, false, Some("s1"), 1).as_deref(), Some("s2"));
    }

    #[test]
    fn shuffle_plays_every_song_once_then_draws_again() {
        let (mut o, songs) = (PlayOrder::default(), ids(5));
        let mut cur = None;
        let mut round = vec![];
        for _ in 0..5 {
            cur = o.next(&songs, true, cur.as_deref(), 42);
            round.push(cur.clone().unwrap());
        }
        let mut sorted = round.clone();
        sorted.sort();
        assert_eq!(sorted, songs, "each song once per round");
        let next = o.next(&songs, true, cur.as_deref(), 43).unwrap();
        assert_ne!(Some(next), cur, "a new round does not repeat the last song first");
    }

    #[test]
    fn a_song_added_while_live_joins_the_round() {
        let mut o = PlayOrder::default();
        let mut cur = o.next(&ids(2), true, None, 7);
        cur = o.next(&ids(2), true, cur.as_deref(), 7);
        let with_new: Vec<String> = ids(3);
        let third = o.next(&with_new, true, cur.as_deref(), 7);
        assert_eq!(third.as_deref(), Some("s2"), "the only song not played this round");
    }

    #[test]
    fn an_empty_playlist_has_nothing_next() {
        assert_eq!(PlayOrder::default().next(&[], false, None, 1), None);
        assert_eq!(PlayOrder::default().next(&[], true, None, 1), None);
    }

    #[test]
    fn addresses_follow_visibility() {
        let public = playlist("p1", true, Visibility::Public);
        let private = playlist("p2", true, Visibility::Private);
        assert_eq!(stream_base(&public), "/live/p1");
        assert_eq!(stream_base(&private), "/listen/key-p2");
        assert_eq!(relay_path(&public), "live/p1");
        assert_eq!(relay_path(&private), "listen/key-p2");
    }

    #[test]
    fn a_path_finds_only_a_live_playlist_of_that_visibility() {
        let all = [playlist("p1", true, Visibility::Public), playlist("p2", true, Visibility::Private), playlist("p3", false, Visibility::Public)];
        assert_eq!(by_path(&all, PathKind::Live, "p1").map(|p| p.id.as_str()), Some("p1"));
        assert!(by_path(&all, PathKind::Live, "p2").is_none(), "private: the public path is 404");
        assert_eq!(by_path(&all, PathKind::Listen, "key-p2").map(|p| p.id.as_str()), Some("p2"));
        assert!(by_path(&all, PathKind::Listen, "key-p1").is_none(), "public: no listen path");
        assert!(by_path(&all, PathKind::Live, "p3").is_none(), "off air");
    }

    #[test]
    fn the_limit_names_the_live_playlists() {
        let live = [playlist("a", true, Visibility::Public), playlist("b", true, Visibility::Public)];
        assert_eq!(limit_reached(&live, "c", 3), None);
        let named = limit_reached(&live, "c", 2).unwrap();
        assert_eq!(named.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(limit_reached(&live, "a", 2), None, "already live: not a new one");
    }
}
