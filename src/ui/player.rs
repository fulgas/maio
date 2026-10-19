//! The player (SPEC 8): one <audio> for the whole app, above the routes, so it keeps playing across pages.

use leptos::{html::Audio, prelude::*};
use leptos_router::hooks::use_location;
use thaw::{ToastIntent, ToasterInjection};

use crate::{
    domain::PlaylistRef,
    ui::{
        api, format,
        icons::{Icon, IconButton},
        queue::{QSong, Queue, Repeat},
        say, shelf,
    },
};

#[derive(Clone, Copy)]
pub struct Player {
    pub queue: RwSignal<Queue>,
    pub playing: RwSignal<bool>,
    pub at: RwSignal<f64>,
    pub duration: RwSignal<f64>,
    pub volume: RwSignal<f64>,
    pub muted: RwSignal<bool>,
    audio: NodeRef<Audio>,
}

/// The speaker icon: muted, or turned all the way down, looks the same.
pub fn speaker(volume: f64, muted: bool) -> Icon {
    if muted || volume <= 0.0 { Icon::Muted } else { Icon::Volume }
}

/// Unmuting a player turned all the way down brings it back to half.
pub fn unmuted_volume(volume: f64) -> f64 {
    if volume <= 0.0 { 0.5 } else { volume }
}

fn seed() -> u64 {
    js_sys::Date::now() as u64
}

pub fn provide() -> Player {
    let p = Player {
        queue: RwSignal::new(Queue::default()),
        playing: RwSignal::new(false),
        at: RwSignal::new(0.0),
        duration: RwSignal::new(0.0),
        volume: RwSignal::new(0.7),
        muted: RwSignal::new(false),
        audio: NodeRef::new(),
    };
    provide_context(p);
    p
}

impl Player {
    /// Starts playback. A play() cut short by the next song, or held back by the browser until a click, is not worth an error.
    fn start(a: &web_sys::HtmlMediaElement) {
        if let Ok(p) = a.play() {
            leptos::task::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(p).await;
            });
        }
    }

    /// Loads the queue's current song and plays it, or stops when there is none.
    fn load(&self) {
        let Some(a) = self.audio.get_untracked() else { return };
        match self.queue.with_untracked(|q| q.current().map(|s| s.id.clone())) {
            Some(id) => {
                a.set_src(&api::audio_url(&id));
                Self::start(&a);
            }
            None => {
                let _ = a.pause();
                self.at.set(0.0);
            }
        }
    }

    /// Plays `songs` from `start`; `from` names the playlist they come from.
    pub fn play(&self, songs: Vec<QSong>, start: usize, from: Option<PlaylistRef>) {
        if let Some(p) = &from {
            shelf::mark_played(&p.id);
        }
        let (shuffle, repeat) = self.queue.with_untracked(|q| (q.shuffle, q.repeat));
        self.queue.set(Queue::play(songs, start, from, shuffle, repeat, seed()));
        self.load();
    }

    pub fn toggle(&self) {
        let Some(a) = self.audio.get_untracked() else { return };
        if self.queue.with_untracked(|q| q.current().is_none()) {
            return;
        }
        if a.paused() {
            Self::start(&a);
        } else {
            let _ = a.pause();
        }
    }

    /// The next song; at the end of the list it pauses on the last one.
    pub fn next(&self) {
        if self.queue.try_update(|q| q.next(seed())).unwrap_or(false) {
            self.load();
        } else if let Some(a) = self.audio.get_untracked() {
            let _ = a.pause();
        }
    }

    /// True when `id` is the queue's song, playing or paused: its button then pauses and resumes it.
    pub fn is_current(&self, id: &str) -> bool {
        self.queue.with_untracked(|q| q.current().is_some_and(|s| s.id == id))
    }

    /// True while `id` is the song playing now.
    pub fn is_playing(&self, id: &str) -> bool {
        self.playing.get() && self.queue.with(|q| q.current().is_some_and(|s| s.id == id))
    }

    /// True while the queue plays the playlist `id`.
    pub fn is_playing_list(&self, id: &str) -> bool {
        self.playing.get() && self.queue.with(|q| q.from.as_ref().is_some_and(|f| f.id == id))
    }

    /// Back to the start of the song when it has played a while, else the previous song.
    pub fn prev(&self) {
        if self.at.get_untracked() > 3.0 {
            self.seek(0.0);
        } else {
            self.queue.update(Queue::prev);
            self.load();
        }
    }

    fn ended(&self) {
        let more = self.queue.try_update(|q| q.ended(seed())).unwrap_or(false);
        if more { self.load() } else { self.playing.set(false) }
    }

    pub fn seek(&self, seconds: f64) {
        if let Some(a) = self.audio.get_untracked() {
            a.set_current_time(seconds);
        }
    }

    pub fn set_volume(&self, v: f64) {
        if v > 0.0 {
            self.muted.set(false); // moving the slider up unmutes
        }
        self.volume.set(v);
        if let Some(a) = self.audio.get_untracked() {
            a.set_volume(v);
        }
    }

    /// The speaker button: mute, or bring the sound back.
    pub fn toggle_mute(&self) {
        if speaker(self.volume.get_untracked(), self.muted.get_untracked()) == Icon::Muted {
            self.muted.set(false);
            self.set_volume(unmuted_volume(self.volume.get_untracked()));
        } else {
            self.muted.set(true);
        }
    }

    pub fn shuffle(&self) {
        self.queue.update(|q| q.set_shuffle(!q.shuffle, seed()));
    }

    pub fn cycle_repeat(&self) {
        self.queue.update(|q| q.repeat = q.repeat.next());
    }

    /// A song was deleted: take it out of the queue.
    pub fn forget(&self, id: &str) {
        let was = self.queue.with_untracked(|q| q.current().map(|s| s.id.clone()));
        self.queue.update(|q| q.remove(id));
        if was.as_deref() == Some(id) {
            self.load();
        }
    }
}

#[component]
pub fn PlayerBar() -> impl IntoView {
    let p = expect_context::<Player>();
    let toaster = ToasterInjection::expect_context();
    let location = use_location();
    let title = move || p.queue.with(|q| q.current().map(|s| s.title.clone()));
    let from = move || p.queue.with(|q| q.from.as_ref().map(|f| f.name.clone()));
    let shuffle = Signal::derive(move || p.queue.with(|q| q.shuffle));
    let repeat = move || p.queue.with(|q| q.repeat);
    let audio_time = |ev: &leptos::ev::Event| event_target::<web_sys::HtmlMediaElement>(ev);
    view! {
        <footer class="player" aria-label="Player" class:hidden=move || location.pathname.get() == "/sign-in">
            <audio node_ref=p.audio preload="metadata" prop:volume=move || p.volume.get() prop:muted=move || p.muted.get()
                on:timeupdate=move |ev| p.at.set(audio_time(&ev).current_time())
                on:durationchange=move |ev| {
                    let d = audio_time(&ev).duration();
                    if d.is_finite() { p.duration.set(d) }
                }
                on:play=move |_| p.playing.set(true)
                on:pause=move |_| p.playing.set(false)
                on:ended=move |_| p.ended()
                on:error=move |_| {
                    p.playing.set(false);
                    let title = p.queue.with_untracked(|q| q.current().map(|s| s.title.clone()));
                    // An ended session also breaks the audio: asking for the status sends you to sign-in then.
                    leptos::task::spawn_local(async move {
                        if let (Ok(_), Some(t)) = (api::status().await, title) {
                            say(toaster, format!("Could not play {t}."), ToastIntent::Error);
                        }
                    });
                }></audio>
            <span class="now">
                <span class="now-title">{move || title().unwrap_or_else(|| "Nothing playing".into())}</span>
                <span class="now-from">{move || from().unwrap_or_default()}</span>
            </span>
            <IconButton icon=Icon::Shuffle label="Shuffle: play in random order".to_string() pressed=shuffle class="small" size=18 on_click=move || p.shuffle()/>
            <IconButton icon=Icon::Prev label="Previous song".to_string() class="bare" on_click=move || p.prev()/>
            <IconButton class="big"
                icon=Signal::derive(move || if p.playing.get() { Icon::Pause } else { Icon::Play })
                label=Signal::derive(move || match (p.playing.get(), title().is_some()) {
                    (true, _) => "Pause",
                    (false, true) => "Play",
                    (false, false) => "Play: pick a song first",
                }.to_string())
                disabled=Signal::derive(move || title().is_none())
                size=18 on_click=move || p.toggle()/>
            <IconButton icon=Icon::Next label="Next song".to_string() class="bare" on_click=move || p.next()/>
            <IconButton class="bare" size=18
                icon=Signal::derive(move || if repeat() == Repeat::One { Icon::RepeatOne } else { Icon::Repeat })
                label=Signal::derive(move || match repeat() { Repeat::Off => "Repeat: off", Repeat::All => "Repeat: all", Repeat::One => "Repeat: this song" }.to_string())
                pressed=Signal::derive(move || repeat() != Repeat::Off)
                on_click=move || p.cycle_repeat()/>
            <span class="time at">{move || format::clock(p.at.get())}</span>
            <input class="seek" type="range" aria-label="Seek" min="0" step="1"
                prop:max=move || p.duration.get().max(1.0) prop:value=move || p.at.get()
                on:input=move |ev| p.seek(event_target_value(&ev).parse().unwrap_or(0.0))/>
            <span class="time total">{move || format::clock(p.duration.get())}</span>
            <span class="volume">
                <IconButton class="bare" size=18
                    icon=Signal::derive(move || speaker(p.volume.get(), p.muted.get()))
                    label=Signal::derive(move || if speaker(p.volume.get(), p.muted.get()) == Icon::Muted { "Unmute" } else { "Mute" }.to_string())
                    on_click=move || p.toggle_mute()/>
                <input type="range" aria-label="Volume" min="0" max="1" step="0.05"
                    prop:value=move || if p.muted.get() { 0.0 } else { p.volume.get() }
                    on:input=move |ev| p.set_volume(event_target_value(&ev).parse().unwrap_or(0.7))/>
            </span>
        </footer>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muted_or_silent_shows_the_muted_speaker() {
        assert_eq!(speaker(0.7, false), Icon::Volume);
        assert_eq!(speaker(0.7, true), Icon::Muted);
        assert_eq!(speaker(0.0, false), Icon::Muted, "the minimum volume is the same as muted");
    }

    #[test]
    fn unmuting_at_zero_brings_the_sound_back() {
        assert_eq!(unmuted_volume(0.0), 0.5);
        assert_eq!(unmuted_volume(0.3), 0.3);
    }
}
