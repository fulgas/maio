//! Two decks and an equal-power crossfade (arc42 8.4). Pure: no clock, no I/O; decks are fed by their decoders.

use std::f32::consts::FRAC_PI_2;

pub const CHANNELS: usize = 2;
pub const SAMPLE_RATE: usize = 48_000;
/// 20 ms of interleaved stereo samples.
pub const CHUNK: usize = SAMPLE_RATE / 50 * CHANNELS;

/// Samples per slice a decoder sends (about 43 ms of stereo audio).
pub const SLICE: usize = 4_096;

/// A song being played, fed slice by slice by its decoder. Its length comes from the song's stored seconds,
/// so the crossfade can start on time without decoding ahead; an early end of the decoder ends the deck.
pub struct Deck {
    pub id: String,
    total: usize,
    pos: usize,
    buf: std::collections::VecDeque<i16>,
    rx: tokio::sync::mpsc::Receiver<Vec<i16>>,
    closed: bool,
    got_any: bool,
}

impl Deck {
    pub fn new(id: String, total_samples: usize, rx: tokio::sync::mpsc::Receiver<Vec<i16>>) -> Self {
        Self { id, total: total_samples / CHANNELS * CHANNELS, pos: 0, buf: Default::default(), rx, closed: false, got_any: false }
    }

    /// A deck with all its samples already there (tests).
    pub fn from_samples(id: String, mut samples: Vec<i16>) -> Self {
        samples.truncate(samples.len() / CHANNELS * CHANNELS); // whole frames only, so channels never swap
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let total = samples.len();
        let _ = tx.try_send(samples);
        Self::new(id, total, rx)
    }

    /// Moves what the decoder has sent into the buffer, without waiting.
    fn pull(&mut self) {
        while self.buf.len() < SLICE * 2 {
            match self.rx.try_recv() {
                Ok(s) => {
                    self.got_any |= !s.is_empty();
                    self.buf.extend(s);
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.closed = true;
                    break;
                }
            }
        }
    }

    /// Ready to start: some audio is buffered, or the decoder has finished.
    pub fn is_ready(&mut self) -> bool {
        self.pull();
        !self.buf.is_empty() || self.closed
    }

    /// The decoder finished without any audio: the file could not be decoded.
    pub fn is_dead(&mut self) -> bool {
        self.pull();
        self.closed && !self.got_any
    }

    fn remaining(&self) -> usize {
        if self.closed && self.buf.is_empty() { 0 } else { self.total.saturating_sub(self.pos).max(self.buf.len()) }
    }

    pub fn remaining_s(&self) -> f32 {
        self.remaining() as f32 / (SAMPLE_RATE * CHANNELS) as f32
    }

    fn take(&mut self) -> f32 {
        if self.buf.is_empty() {
            self.pull();
        }
        self.pos += 1;
        // An empty buffer with the decoder still going is an underrun: silence, position kept moving.
        self.buf.pop_front().map_or(0.0, f32::from)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MixEvent {
    Started(String),
    Ended(String),
}

/// The deck playing, the deck fading in, and where the fade is.
pub struct Mixer {
    pub cur: Option<Deck>,
    pub next: Option<Deck>,
    fade_len: usize,
    fade_pos: Option<usize>,
}

impl Mixer {
    pub fn new(fade_s: f32) -> Self {
        let frames = (fade_s.max(0.001) * SAMPLE_RATE as f32) as usize;
        Self { cur: None, next: None, fade_len: frames * CHANNELS, fade_pos: None }
    }

    pub fn fade_s(&self) -> f32 {
        self.fade_len as f32 / (SAMPLE_RATE * CHANNELS) as f32
    }

    /// Seconds left of the current song; 0 when nothing plays.
    pub fn remaining_s(&self) -> f32 {
        self.cur.as_ref().map_or(0.0, Deck::remaining_s)
    }

    pub fn render(&mut self, out: &mut [i16], events: &mut Vec<MixEvent>) {
        for o in out.iter_mut() {
            if self.cur.is_none() {
                match self.next.take() {
                    Some(n) => {
                        events.push(MixEvent::Started(n.id.clone()));
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
                events.push(MixEvent::Ended(old.id));
                if self.fade_pos.take().is_some()
                    && let Some(n) = self.next.take()
                {
                    events.push(MixEvent::Started(n.id.clone()));
                    self.cur = Some(n);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(id: &str, value: i16, frames: usize) -> Deck {
        Deck::from_samples(id.into(), vec![value; frames * CHANNELS])
    }

    #[test]
    fn plays_a_deck_then_silence() {
        let mut m = Mixer::new(0.001);
        m.next = Some(deck("a", 1000, 2));
        let (mut out, mut ev) = (vec![7i16; 8], vec![]);
        m.render(&mut out, &mut ev);
        assert_eq!(&out[..4], &[1000; 4]);
        assert_eq!(&out[4..], &[0; 4], "nothing next: silence");
        assert_eq!(ev, [MixEvent::Started("a".into()), MixEvent::Ended("a".into())]);
    }

    #[test]
    fn crossfades_with_equal_power() {
        // A 2 ms fade (96 frames; the mixer's floor is 1 ms) between two 1 000-frame decks.
        let mut m = Mixer::new(0.002);
        m.next = Some(deck("a", 10_000, 1_000));
        let mut ev = vec![];
        m.render(&mut [0i16; 2], &mut ev); // a starts
        m.next = Some(deck("b", 10_000, 1_000));
        let mut rest = vec![0i16; 1_850 * CHANNELS]; // the two decks overlap by the fade: about 1 904 frames in all
        m.render(&mut rest, &mut ev);
        // Equal power: the sum never dips below one deck, and peaks near 1.41 × at the middle of the fade.
        let (min, max) = (*rest.iter().min().unwrap(), *rest.iter().max().unwrap());
        assert!(min >= 9_999, "no dip during the fade, got {min}");
        assert!((13_900..=14_200).contains(&max), "peak near 14 142, got {max}");
        assert!(ev.contains(&MixEvent::Ended("a".into())) && ev.contains(&MixEvent::Started("b".into())));
    }

    #[test]
    fn odd_sample_counts_keep_channels_paired() {
        let mut d = Deck::from_samples("x".into(), vec![1, 2, 3]);
        assert_eq!(d.remaining_s() * (SAMPLE_RATE * CHANNELS) as f32, 2.0);
        assert!(d.is_ready());
    }

    #[test]
    fn a_deck_plays_slices_as_they_arrive_and_underruns_as_silence() {
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let mut d = Deck::new("s".into(), 8, rx);
        assert!(!d.is_ready(), "nothing decoded yet");
        tx.try_send(vec![5, 5, 5, 5]).unwrap();
        assert!(d.is_ready());
        let mut m = Mixer::new(0.001);
        m.next = Some(d);
        let (mut out, mut ev) = (vec![0i16; 6], vec![]);
        m.render(&mut out, &mut ev);
        assert_eq!(out, [5, 5, 5, 5, 0, 0], "the decoder is behind: silence, position kept");
        tx.try_send(vec![6, 6, 6, 6]).unwrap();
        drop(tx);
        let mut more = vec![0i16; 4];
        m.render(&mut more, &mut ev);
        assert_eq!(more, [6, 6, 6, 6]);
    }

    #[test]
    fn a_decoder_that_ends_early_ends_the_deck() {
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let d = Deck::new("s".into(), 1_000, rx); // says long, decodes short
        tx.try_send(vec![1, 1]).unwrap();
        drop(tx);
        let mut m = Mixer::new(0.001);
        m.next = Some(d);
        let (mut out, mut ev) = (vec![0i16; 6], vec![]);
        m.render(&mut out, &mut ev);
        assert!(ev.contains(&MixEvent::Ended("s".into())));
        let mut dead = Deck::new("x".into(), 100, tokio::sync::mpsc::channel(1).1);
        assert!(dead.is_dead(), "closed with no audio: could not be decoded");
    }
}
