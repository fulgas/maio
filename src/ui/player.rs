//! The player (arc42 8.3): one <audio> for the whole app, above the routes, so it keeps playing across pages.
//! A live playlist plays its stream (arc42 8.4): what everyone listening hears, with no skipping.

use leptos::{html::Audio, prelude::*};
use leptos_router::hooks::use_location;
use thaw::{ToastIntent, ToasterInjection};

use crate::{
    domain::PlaylistRef,
    ui::{
        api, format,
        icons::{Icon, IconButton, Svg},
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
    /// Set while listening to a live playlist's stream instead of the queue.
    pub live: RwSignal<Option<OnStream>>,
    /// Something to tell the person; the player bar shows it as a toast.
    pub notice: RwSignal<Option<(String, ToastIntent)>>,
    audio: NodeRef<Audio>,
}

/// A live stream being listened to.
#[derive(Debug, Clone, PartialEq)]
pub struct OnStream {
    pub playlist: PlaylistRef,
    /// The song on air, from the live API.
    pub now: Option<String>,
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
        live: RwSignal::new(None),
        notice: RwSignal::new(None),
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
        self.live.set(None);
        if let Some(p) = &from {
            shelf::mark_played(&p.id);
        }
        let (shuffle, repeat) = self.queue.with_untracked(|q| (q.shuffle, q.repeat));
        self.queue.set(Queue::play(songs, start, from, shuffle, repeat, seed()));
        self.load();
    }

    /// "Shuffle play": the playlist from a random song, with the player's shuffle on.
    pub fn shuffle_play(&self, songs: Vec<QSong>, from: PlaylistRef) {
        if songs.is_empty() {
            return self.notice.set(Some((format!("{} has no songs yet.", from.name), ToastIntent::Info)));
        }
        let start = (seed() % songs.len() as u64) as usize;
        self.queue.update(|q| q.shuffle = true);
        self.play(songs, start, Some(from));
    }

    /// Play (or Pause) on a playlist. Playing from it, it pauses; a paused stream resumes at the live edge;
    /// otherwise a live playlist tunes into its stream and any other resumes or plays its songs (`songs`, or fetched).
    pub fn play_list(&self, from: PlaylistRef, songs: Option<Vec<QSong>>) {
        let streaming = self.live.with_untracked(|l| l.as_ref().is_some_and(|l| l.playlist.id == from.id));
        if streaming || (self.is_from(&from.id) && self.playing.get_untracked()) {
            return self.toggle();
        }
        let p = *self;
        leptos::task::spawn_local(async move {
            if p.tune(&from).await {
                shelf::mark_played(&from.id);
                return;
            }
            if p.is_from(&from.id) {
                return p.toggle(); // its songs, paused: carry on where they were
            }
            let songs = match songs {
                Some(s) => s,
                None => match api::playlist(&from.id).await {
                    Ok(v) => v.songs.iter().map(QSong::from).collect(),
                    Err(f) => return p.notice.set(Some((f.text(), ToastIntent::Error))),
                },
            };
            if songs.is_empty() {
                return p.notice.set(Some((format!("{} has no songs yet.", from.name), ToastIntent::Info)));
            }
            p.play(songs, 0, Some(from));
        });
    }

    /// Opens a live playlist's stream at its live edge; `false` when it is off air or out of reach.
    async fn tune(self, playlist: &PlaylistRef) -> bool {
        let Ok(live) = api::live(&playlist.id).await else { return false };
        let Some(src) = live.addresses.filter(|_| live.live).and_then(|a| a.mp3) else { return false };
        let Some(a) = self.audio.get_untracked() else { return false };
        self.queue.update(|q| *q = Queue::play(Vec::new(), 0, None, q.shuffle, q.repeat, 0));
        self.live.set(Some(OnStream { playlist: playlist.clone(), now: live.on_air.and_then(|o| o.now).map(|n| n.title) }));
        a.set_src(&src);
        Self::start(&a);
        true
    }

    /// The stream ended or broke: open it again (a new address after a visibility change), or stop when it is off air.
    fn retune(&self) {
        let Some(on) = self.live.get_untracked() else { return };
        let p = *self;
        leptos::task::spawn_local(async move {
            if !p.tune(&on.playlist).await {
                p.live.set(None);
                p.playing.set(false);
                p.notice.set(Some((format!("{} went off air.", on.playlist.name), ToastIntent::Info)));
            }
        });
    }

    /// Stops listening without leaving the connection open: a paused listener is not a listener.
    fn hang_up(&self, a: &web_sys::HtmlMediaElement) {
        let _ = a.pause();
        let _ = a.remove_attribute("src");
        a.load();
        // load() drops the "pause" event pause() queued, so say it here.
        self.playing.set(false);
    }

    pub fn toggle(&self) {
        let Some(a) = self.audio.get_untracked() else { return };
        if self.live.with_untracked(Option::is_some) {
            // Resuming a stream starts again at its live edge, where everyone else is.
            if a.paused() {
                self.retune()
            } else {
                self.hang_up(&a)
            }
            return;
        }
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
        if self.live.with_untracked(Option::is_some) {
            return; // a broadcast plays the same for everyone
        }
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

    /// True while the queue plays the playlist `id`, or its live stream plays.
    pub fn is_playing_list(&self, id: &str) -> bool {
        self.playing.get()
            && (self.queue.with(|q| q.from.as_ref().is_some_and(|f| f.id == id))
                || self.live.with(|l| l.as_ref().is_some_and(|l| l.playlist.id == id)))
    }

    /// True when the player holds the playlist `id`, playing or paused: its Play button then pauses and resumes.
    pub fn is_from(&self, id: &str) -> bool {
        self.queue.with_untracked(|q| q.from.as_ref().is_some_and(|f| f.id == id))
            || self.live.with_untracked(|l| l.as_ref().is_some_and(|l| l.playlist.id == id))
    }

    /// Back to the start of the song when it has played a while, else the previous song.
    pub fn prev(&self) {
        if self.live.with_untracked(Option::is_some) {
            return;
        }
        if self.at.get_untracked() > 3.0 {
            self.seek(0.0);
        } else {
            self.queue.update(Queue::prev);
            self.load();
        }
    }

    fn ended(&self) {
        if self.live.with_untracked(Option::is_some) {
            return self.retune();
        }
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
        if self.live.with_untracked(Option::is_some) {
            return;
        }
        self.queue.update(|q| q.set_shuffle(!q.shuffle, seed()));
    }

    pub fn cycle_repeat(&self) {
        if self.live.with_untracked(Option::is_some) {
            return;
        }
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
    let on_stream = move || p.live.with(Option::is_some);
    let title = move || match p.live.get() {
        Some(l) => Some(l.now.unwrap_or(l.playlist.name)),
        None => p.queue.with(|q| q.current().map(|s| s.title.clone())),
    };
    let from = move || p.live.get().map(|l| l.playlist.name).or_else(|| p.queue.with(|q| q.from.as_ref().map(|f| f.name.clone())));
    let not_live =
        |what: &'static str| Signal::derive(move || if on_stream() { format!("{what}: not while listening live") } else { what.to_string() });
    Effect::new(move |_| {
        if let Some((text, intent)) = p.notice.get() {
            say(toaster, text, intent);
            p.notice.set(None);
        }
    });
    // While listening live, what is on air: looked at when a live event says it changed.
    let changes = expect_context::<crate::ui::events::Changes>();
    Effect::new(move |_| {
        changes.live.track();
        let look = move || {
            let Some(on) = p.live.get_untracked() else { return };
            leptos::task::spawn_local(async move {
                if let Ok(l) = api::live(&on.playlist.id).await {
                    // Off air while paused: no stream event says so, this look does.
                    if !l.live {
                        p.live.set(None);
                        p.notice.set(Some((format!("{} went off air.", on.playlist.name), ToastIntent::Info)));
                        return;
                    }
                    let now = l.on_air.and_then(|o| o.now).map(|n| n.title);
                    p.live.update(|cur| {
                        if let Some(c) = cur.as_mut().filter(|c| c.playlist.id == on.playlist.id) {
                            c.now = now;
                        }
                    });
                }
            });
        };
        look();
    });
    let shuffle = Signal::derive(move || p.queue.with(|q| q.shuffle));
    let repeat = move || p.queue.with(|q| q.repeat);
    let audio_time = |ev: &leptos::ev::Event| event_target::<web_sys::HtmlMediaElement>(ev);
    view! {
        <footer class="player" aria-label="Player" class:hidden=move || location.pathname.get() == "/sign-in" || title().is_none()>
            <audio node_ref=p.audio preload="metadata" prop:volume=move || p.volume.get() prop:muted=move || p.muted.get()
                on:timeupdate=move |ev| p.at.set(audio_time(&ev).current_time())
                on:durationchange=move |ev| {
                    let d = audio_time(&ev).duration();
                    if d.is_finite() { p.duration.set(d) }
                }
                on:play=move |_| p.playing.set(true)
                on:pause=move |_| p.playing.set(false)
                on:ended=move |_| p.ended()
                on:error=move |ev| {
                    p.playing.set(false);
                    if p.live.with_untracked(Option::is_some) {
                        // Hanging up empties the source on purpose; anything else lost the stream.
                        if audio_time(&ev).has_attribute("src") {
                            p.retune();
                        }
                        return;
                    }
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
                <span class="now-from">
                    {move || on_stream().then(|| view! { <span class="live-mark"><Svg icon=Icon::Live size=14/>"Live"</span> })}
                    {move || from().unwrap_or_default()}
                </span>
            </span>
            <IconButton icon=Icon::Shuffle label=not_live("Shuffle: play in random order") pressed=shuffle class="small" size=18
                disabled=Signal::derive(on_stream) on_click=move || p.shuffle()/>
            <IconButton icon=Icon::Prev label=not_live("Previous song") class="bare" disabled=Signal::derive(on_stream) on_click=move || p.prev()/>
            <IconButton class="big"
                icon=Signal::derive(move || if p.playing.get() { Icon::Pause } else { Icon::Play })
                label=Signal::derive(move || if p.playing.get() { "Pause" } else { "Play" }.to_string())
                size=18 on_click=move || p.toggle()/>
            <IconButton icon=Icon::Next label=not_live("Next song") class="bare" disabled=Signal::derive(on_stream) on_click=move || p.next()/>
            <IconButton class="bare" size=18
                icon=Signal::derive(move || if repeat() == Repeat::One { Icon::RepeatOne } else { Icon::Repeat })
                label=Signal::derive(move || {
                    let r = match repeat() { Repeat::Off => "Repeat: off", Repeat::All => "Repeat: all", Repeat::One => "Repeat: this song" };
                    if on_stream() { format!("{r}: not while listening live") } else { r.to_string() }
                })
                pressed=Signal::derive(move || repeat() != Repeat::Off)
                disabled=Signal::derive(on_stream)
                on_click=move || p.cycle_repeat()/>
            // A stream has no length and no seeking.
            <span class="time at" class:hidden=on_stream>{move || format::clock(p.at.get())}</span>
            <input class="seek" type="range" aria-label="Seek" min="0" step="1" class:hidden=on_stream
                prop:max=move || p.duration.get().max(1.0) prop:value=move || p.at.get()
                on:input=move |ev| p.seek(event_target_value(&ev).parse().unwrap_or(0.0))/>
            <span class="time total" class:hidden=on_stream>{move || format::clock(p.duration.get())}</span>
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
