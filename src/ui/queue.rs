//! The player's queue (SPEC 8): play order, shuffle and repeat. Pure, so it is tested here; the player bar drives it.

use crate::domain::{PlaylistRef, Song};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    /// The repeat button cycles off → all → one.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QSong {
    pub id: String,
    pub title: String,
    pub seconds: f64,
}

impl From<&Song> for QSong {
    fn from(s: &Song) -> Self {
        Self { id: s.id.clone(), title: s.title.clone(), seconds: s.seconds }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Queue {
    songs: Vec<QSong>,
    /// Indexes into `songs` in play order.
    order: Vec<usize>,
    /// Position in `order`; `None` when nothing plays.
    pos: Option<usize>,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// The playlist being played, for the player bar; `None` for the song list.
    pub from: Option<PlaylistRef>,
}

impl Queue {
    pub fn play(songs: Vec<QSong>, start: usize, from: Option<PlaylistRef>, shuffle: bool, repeat: Repeat, seed: u64) -> Self {
        let start = start.min(songs.len().saturating_sub(1));
        let order = if shuffle { shuffled(songs.len(), start, seed) } else { (0..songs.len()).collect() };
        let pos = if songs.is_empty() { None } else { order.iter().position(|&i| i == start) };
        Self { songs, order, pos, shuffle, repeat, from }
    }

    pub fn current(&self) -> Option<&QSong> {
        self.pos.map(|p| &self.songs[self.order[p]])
    }

    /// The next song; past the end it wraps with repeat all, else stops. Returns whether a song is current.
    /// The next song; past the end it wraps with repeat all. Returns false when the list is over:
    /// the queue then stays on its last song, so Play plays it again.
    pub fn next(&mut self, seed: u64) -> bool {
        let Some(p) = self.pos else { return false };
        if p + 1 < self.order.len() {
            self.pos = Some(p + 1);
            return true;
        }
        if self.repeat == Repeat::All && !self.order.is_empty() {
            if self.shuffle {
                self.order = shuffled(self.songs.len(), self.order[0], seed);
            }
            self.pos = Some(0);
            return true;
        }
        false
    }

    pub fn prev(&mut self) {
        if let Some(p) = self.pos {
            self.pos = Some(p.saturating_sub(1));
        }
    }

    /// A song finished playing: repeat one replays it, else it is `next`.
    pub fn ended(&mut self, seed: u64) -> bool {
        if self.repeat == Repeat::One { self.pos.is_some() } else { self.next(seed) }
    }

    /// Keeps the current song current and reorders the rest.
    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        self.shuffle = on;
        let current = self.pos.map(|p| self.order[p]);
        self.order = match (on, current) {
            (true, Some(c)) => shuffled(self.songs.len(), c, seed),
            (true, None) => shuffled(self.songs.len(), 0, seed),
            (false, _) => (0..self.songs.len()).collect(),
        };
        self.pos = current.and_then(|c| self.order.iter().position(|&i| i == c));
    }

    /// A song was deleted: drop it; if it was playing, nothing plays.
    pub fn remove(&mut self, id: &str) {
        let Some(gone) = self.songs.iter().position(|s| s.id == id) else { return };
        let current = self.pos.map(|p| self.order[p]);
        self.songs.remove(gone);
        let fix = |i: usize| if i > gone { i - 1 } else { i };
        self.order.retain(|&i| i != gone);
        self.order.iter_mut().for_each(|i| *i = fix(*i));
        self.pos = match current {
            Some(c) if c == gone => None,
            Some(c) => self.order.iter().position(|&i| i == fix(c)),
            None => None,
        };
    }
}

/// A shuffled play order of `n` songs with `first` in front (Fisher-Yates on xorshift64; no rand in the browser).
pub fn shuffled(n: usize, first: usize, seed: u64) -> Vec<usize> {
    let mut rest: Vec<usize> = (0..n).filter(|&i| i != first).collect();
    let mut x = seed | 1;
    for i in (1..rest.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        rest.swap(i, (x % (i as u64 + 1)) as usize);
    }
    if first < n { std::iter::once(first).chain(rest).collect() } else { rest }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn songs(n: usize) -> Vec<QSong> {
        (0..n).map(|i| QSong { id: format!("s{i}"), title: format!("Song {i}"), seconds: 60.0 }).collect()
    }
    fn id(q: &Queue) -> Option<&str> {
        q.current().map(|s| s.id.as_str())
    }

    #[test]
    fn plays_from_the_chosen_song_in_order() {
        let mut q = Queue::play(songs(3), 1, None, false, Repeat::Off, 7);
        assert_eq!(id(&q), Some("s1"));
        q.next(7);
        assert_eq!(id(&q), Some("s2"));
        assert!(!q.next(7), "repeat off stops after the last song");
        assert_eq!(id(&q), Some("s2"), "and stays on it, so Play plays it again");
    }

    #[test]
    fn repeat_all_wraps_and_repeat_one_stays() {
        let mut q = Queue::play(songs(2), 1, None, false, Repeat::All, 7);
        q.ended(7);
        assert_eq!(id(&q), Some("s0"));
        q.repeat = Repeat::One;
        q.ended(7);
        assert_eq!(id(&q), Some("s0"), "a song that ends replays");
        q.next(7);
        assert_eq!(id(&q), Some("s1"), "next still moves on with repeat one");
    }

    #[test]
    fn prev_goes_back_and_stops_at_the_first() {
        let mut q = Queue::play(songs(3), 1, None, false, Repeat::Off, 7);
        q.prev();
        assert_eq!(id(&q), Some("s0"));
        q.prev();
        assert_eq!(id(&q), Some("s0"));
    }

    #[test]
    fn shuffle_plays_every_song_once_starting_with_the_chosen_one() {
        let mut q = Queue::play(songs(6), 4, None, true, Repeat::Off, 42);
        let mut seen = vec![id(&q).unwrap().to_string()];
        assert_eq!(seen[0], "s4");
        for _ in 1..6 {
            assert!(q.next(42));
            seen.push(id(&q).unwrap().to_string());
        }
        assert!(!q.next(42));
        seen.sort();
        assert_eq!(seen, ["s0", "s1", "s2", "s3", "s4", "s5"]);
    }

    #[test]
    fn turning_shuffle_on_or_off_keeps_the_current_song() {
        let mut q = Queue::play(songs(5), 2, None, false, Repeat::Off, 1);
        q.set_shuffle(true, 9);
        assert_eq!(id(&q), Some("s2"));
        q.set_shuffle(false, 9);
        assert_eq!(id(&q), Some("s2"));
        q.next(9);
        assert_eq!(id(&q), Some("s3"), "back in playlist order");
    }

    #[test]
    fn shuffled_is_a_permutation_with_first_in_front() {
        let order = shuffled(10, 3, 12345);
        assert_eq!(order[0], 3);
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, (0..10).collect::<Vec<_>>());
        assert_eq!(order, shuffled(10, 3, 12345), "the same seed gives the same order");
    }

    #[test]
    fn removing_a_song_keeps_playing_the_rest() {
        let mut q = Queue::play(songs(3), 0, None, false, Repeat::Off, 1);
        q.remove("s1");
        q.next(1);
        assert_eq!(id(&q), Some("s2"));
        q.remove("s2");
        assert_eq!(id(&q), None, "removing the current song stops it");
    }

    #[test]
    fn repeat_cycles_off_all_one() {
        assert_eq!(Repeat::Off.next(), Repeat::All);
        assert_eq!(Repeat::All.next(), Repeat::One);
        assert_eq!(Repeat::One.next(), Repeat::Off);
    }
}
