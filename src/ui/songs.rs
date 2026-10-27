//! The Songs screen (arc42 8.5): the shelf of playlists, "Make a song", "Being made" and the song list.

use leptos::{ev::SubmitEvent, prelude::*};
use leptos_router::hooks::use_navigate;
use thaw::{Button, ButtonAppearance, Field, Input, ToastIntent, ToasterInjection};

use crate::{
    domain::{Song, job::JobState},
    ui::{
        api::{self, Failure, PlaylistSummary, SongView, Status},
        events::Changes,
        format,
        icons::{Icon, IconButton, Svg},
        make::{self, LANGUAGES, MakeForm},
        menu::Dropdown,
        modal::Modal,
        player::Player,
        playlist,
        queue::QSong,
        say, shelf,
    },
};

type Playlists = Signal<Option<Result<Vec<PlaylistSummary>, Failure>>>;

/// Every playlist: loaded once, then kept as events tell them (ADR 0018).
pub fn all_playlists(c: Changes) -> Playlists {
    let loaded = LocalResource::new(move || {
        let listening = c.listening();
        async move {
            listening.await;
            api::playlists().await
        }
    });
    Signal::derive(move || loaded.get().map(|r| r.map(|all| crate::ui::events::playlists_now(&all, &c.playlists.get()))))
}

#[component]
pub fn SongsPage() -> impl IntoView {
    let c = expect_context::<Changes>();
    let playlists = all_playlists(c);
    let status = LocalResource::new(move || {
        let listening = c.listening();
        async move {
            listening.await;
            api::status().await
        }
    });
    view! {
        <Shelf playlists/>
        <MakeSong status/>
        <BeingMade/>
        <SongList playlists/>
    }
}

/// Playlists (arc42 8.5): every playlist as a tile, "+" first to make one.
#[component]
pub fn PlaylistsPage() -> impl IntoView {
    let c = expect_context::<Changes>();
    let playlists = all_playlists(c);
    let open = RwSignal::new(false);
    let summary = move || {
        playlists.get().and_then(Result::ok).map(|all| match all.len() {
            0 => "None yet. Make one with New playlist, then add songs to it from Songs.".to_string(),
            1 => "1 playlist".to_string(),
            n => format!("{n} playlists"),
        })
    };
    view! {
        <div class="list-head">
            <h1>"Playlists"</h1>
            <span class="hint">{summary}</span>
            <button type="button" class="new-playlist" on:click=move |_| open.set(true)><Svg icon=Icon::Add size=18/>"New playlist"</button>
        </div>
        <NewPlaylist open/>
        <PlaylistList playlists/>
    }
}

/// Every playlist as a row (arc42 8.5): its colour, like its banner; play; its name; live; songs; length.
#[component]
fn PlaylistList(playlists: Playlists) -> impl IntoView {
    let player = expect_context::<Player>();
    // One pair of Rename and Delete dialogs for the whole list, aimed at the row whose menu was used.
    let (renaming, deleting, target) = (RwSignal::new(false), RwSignal::new(false), RwSignal::new((String::new(), String::new())));
    let rows = move || {
        let all = loaded(playlists);
        shelf::order(&all, &shelf::played()).into_iter().map(|p| (shelf::colour(&all, &p.id), p)).collect::<Vec<_>>()
    };
    view! {
        <ol class="playlist-list" aria-label="Your playlists">
            <For each=rows key=|(_, p)| (p.id.clone(), p.name.clone(), p.songs, p.live.on) children=move |(colour, p)| {
                let (id, name) = (p.id.clone(), p.name.clone());
                let play = {
                    let (id, name) = (id.clone(), name.clone());
                    move || player.play_list(crate::domain::PlaylistRef { id: id.clone(), name: name.clone() }, None)
                };
                view! {
                    <li class="playlist-row">
                        <IconButton class="play" size=14
                            icon={ let id = id.clone(); Signal::derive(move || if player.is_playing_list(&id) { Icon::Pause } else { Icon::Play }) }
                            label={ let (id, name) = (id.clone(), name.clone()); Signal::derive(move || format!("{} {name}", if player.is_playing_list(&id) { "Pause" } else { "Play" })) }
                            on_click=play/>
                        <span class="song-main">
                            <a class="song-title playlist-name" href=format!("/playlists/{id}")>
                                <i class="strip" style:background=colour aria-hidden="true"></i>{name.clone()}
                            </a>
                            <span class="song-prompt">{format::songs(p.songs)}</span>
                        </span>
                        <span class="live-chip">{p.live.on.then(|| view! { <Svg icon=Icon::Live size=14/>"Live" })}</span>
                        <span class="len">{format::length(p.seconds)}</span>
                        <Dropdown icon=Icon::More label=format!("Rename or delete {name}") items=playlist::more_items()
                            on_select={ let (id, name, pick) = (id.clone(), name.clone(), playlist::more(renaming, deleting)); move |key| { target.set((id.clone(), name.clone())); pick(key) } }/>
                    </li>
                }
            }/>
        </ol>
        <playlist::PlaylistDialogs renaming deleting
            id=Signal::derive(move || target.get().0) name=Signal::derive(move || target.get().1)
            on_renamed=Callback::new(|_| ()) on_deleted=Callback::new(|()| ())/>
    }
}

fn loaded(playlists: Playlists) -> Vec<PlaylistSummary> {
    playlists.get().and_then(Result::ok).unwrap_or_default()
}

/// The Songs screen's shelf of playlist tiles.
#[component]
fn Shelf(playlists: Playlists) -> impl IntoView {
    let open = RwSignal::new(false);
    // One row that never scrolls: as many playlists as fit (at most a few), then "All playlists".
    let row = NodeRef::<leptos::html::Section>::new();
    let width = RwSignal::new(None::<f64>);
    {
        let measure = move || {
            if let Some(el) = row.get_untracked() {
                width.set(Some(f64::from(el.client_width())));
            }
        };
        Effect::new(move |_| {
            if row.get().is_some() {
                measure();
            }
        });
        let resize = window_event_listener(leptos::ev::resize, move |_| measure());
        on_cleanup(move || resize.remove());
    }
    let ordered = move || {
        let all = loaded(playlists);
        shelf::order(&all, &shelf::played()).into_iter().map(|p| (shelf::colour(&all, &p.id), p)).collect::<Vec<_>>()
    };
    let shown = move || match width.get() {
        Some(w) => {
            let list = ordered();
            let (n, more) = shelf::fits(w, list.len());
            (list.into_iter().take(n).collect::<Vec<_>>(), more)
        }
        _ => (ordered(), false),
    };
    let tiles = move || shown().0;
    let total = move || loaded(playlists).len();
    view! {
        // A full shelf stretches its tiles to end where the Make box ends.
        <section class="shelf" class:full=move || { let (shown, more) = shown(); more || shown.len() == shelf::MAX_SHOWN } aria-label="Your playlists" node_ref=row>
            // Words, not captions: the shelf's scrolling row would cut a caption off.
            <button type="button" class="tile new" on:click=move |_| open.set(true)>
                <Svg icon=Icon::Add size=28/><span>"New playlist"</span>
            </button>
            <For each=tiles key=|(_, p)| (p.id.clone(), p.name.clone(), p.songs, p.live.on) children=move |(colour, p)| view! {
                <a class="tile" href=format!("/playlists/{}", p.id) style:background=colour>
                    <span class="tile-name">{p.name.clone()}</span>
                    <span class="tile-count">
                        {format::songs(p.songs)}
                        {p.live.on.then(|| view! { <span class="live-mark"><Svg icon=Icon::Live size=16/>"Live"</span> })}
                    </span>
                </a>
            }/>
            {move || shown().1.then(|| view! {
                <a class="tile all" href="/playlists">"All playlists"<span class="tile-count">{format!("{} in all", total())}</span></a>
            })}
        </section>
        <NewPlaylist open/>
    }
}

/// The "New playlist" dialog, from the shelf's "+" or the Playlists page; a new playlist opens at once.
#[component]
fn NewPlaylist(open: RwSignal<bool>) -> impl IntoView {
    let toaster = ToasterInjection::expect_context();
    let navigate = use_navigate();
    let (name, busy) = (RwSignal::new(String::new()), RwSignal::new(false));
    let create = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            let created = api::create_playlist(&name.get_untracked()).await;
            busy.set(false);
            match created {
                Ok(p) => {
                    open.set(false);
                    name.set(String::new());
                    navigate(&format!("/playlists/{}", p.id), Default::default());
                }
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        });
    };
    view! {
        <Modal open title="New playlist">
                    <Field label="Name"><Input value=name/></Field>
                    <div class="modal-actions">
                        <Button on_click=move |_| open.set(false)>"Cancel"</Button>
                        <Button appearance=ButtonAppearance::Primary on_click=move |_| create()>"Create"</Button>
                    </div>
        </Modal>
    }
}

#[component]
fn MakeSong(status: LocalResource<Result<Status, Failure>>) -> impl IntoView {
    let c = expect_context::<Changes>();
    let (prompt, mood, genre, energy) =
        (RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()));
    let (seconds, tempo, seed) = (RwSignal::new(None::<u32>), RwSignal::new(None::<u32>), RwSignal::new(String::new()));
    let (vocals, voice, lyrics, language, lyrics_by) = (
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
    );
    let open = RwSignal::new(false);
    // "Make one like this" hands a form over; take it, and bring the prompt into view.
    let draft = expect_context::<crate::ui::make::Draft>();
    let prompt_box = NodeRef::<leptos::html::Textarea>::new();
    Effect::new(move |_| {
        let Some(f) = draft.0.get() else { return };
        draft.0.set(None);
        prompt.set(f.prompt);
        mood.set(f.mood);
        genre.set(f.genre);
        energy.set(f.energy);
        seconds.set(f.seconds);
        tempo.set(f.tempo);
        seed.set(f.seed);
        vocals.set(f.vocals);
        voice.set(f.voice);
        lyrics.set(f.lyrics);
        language.set(f.language);
        lyrics_by.set(f.lyrics_by);
        open.set(true); // the copied settings in view
        if let Some(t) = prompt_box.get() {
            t.scroll_into_view();
            let _ = t.focus();
        }
    });
    let (error, ask_noncommercial, busy) = (RwSignal::new(None::<String>), RwSignal::new(false), RwSignal::new(false));
    let st = move || status.get().and_then(Result::ok);
    // The slider ends where the ready engines do; a longer length picked before stops there too.
    let longest = Memo::new(move |_| st().map_or(make::LONGEST, |s| make::longest(&crate::ui::events::engines_now(&s.engines, &c.engines.get()))));
    let current = move || MakeForm {
        prompt: prompt.get(),
        mood: mood.get(),
        genre: genre.get(),
        energy: energy.get(),
        seconds: seconds.get().map(|s| s.min(longest.get())),
        tempo: tempo.get(),
        seed: seed.get(),
        vocals: vocals.get(),
        voice: voice.get(),
        lyrics: lyrics.get(),
        language: language.get(),
        lyrics_by: lyrics_by.get(),
    };
    let sings = move || vocals.get() == "vocals";
    let count = Memo::new(move |_| current().count());
    // One request at a time: a double click must not make two songs.
    let send = move |allow: bool| match untrack(current).request(allow) {
        Err(msg) => error.set(Some(msg)),
        Ok(_) if busy.get_untracked() => {}
        Ok(req) => leptos::task::spawn_local(async move {
            busy.set(true);
            let made = api::make(&req).await;
            busy.set(false);
            match made {
                Ok(_) => {
                    error.set(None);
                    prompt.set(String::new());
                    if let Ok(open) = api::jobs().await {
                        c.jobs.update(|jobs| {
                            open.into_iter().for_each(|j| {
                                crate::ui::events::upsert(jobs, j);
                            })
                        });
                    }
                }
                Err(f) if f.code == "noncommercial_engine" => ask_noncommercial.set(true),
                Err(f) => error.set(Some(f.text())),
            }
        }),
    };
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        send(false);
    };
    let ready = expect_context::<crate::ui::frame::EngineReady>().0;
    let auto_first =
        |all: Vec<String>| std::iter::once((String::new(), "Auto".to_string())).chain(all.into_iter().map(|m| (m.clone(), m))).collect::<Vec<_>>();
    let moods = Signal::derive(move || auto_first(st().map(|s| s.moods).unwrap_or_default()));
    let genres = Signal::derive(move || auto_first(st().map(|s| s.genres).unwrap_or_default()));
    let writers = Signal::derive(move || {
        let ids = st().map(|s| s.text_engines.into_iter().map(|t| t.id).collect::<Vec<_>>()).unwrap_or_default();
        [("", "Auto"), ("ace-step", "ACE-Step")]
            .map(|(v, l)| (v.to_string(), l.to_string()))
            .into_iter()
            .chain(ids.into_iter().map(|w| (w.clone(), w)))
            .collect::<Vec<_>>()
    });
    let toggle = NodeRef::<leptos::html::Button>::new();
    let toggle_label = move || match count.get() {
        0 => "Song settings: mood, tempo, length, vocals and more".to_string(),
        n => format!("Song settings: {n} set"),
    };
    let close = move || {
        open.set(false);
        if let Some(b) = toggle.get_untracked() {
            let _ = b.focus();
        }
    };
    let seed_tip = move || {
        if seed.with(|s| s.trim().is_empty()) {
            "A new random seed each time, so every song is a new take"
        } else {
            "The number the song starts from: the same seed and settings make the same song. Change one setting for a variation"
        }
    };
    view! {
        <form class="make" class:open=move || open.get() aria-labelledby="make-title" on:submit=submit>
            <div class="make-main">
                <div class="make-head">
                    <h2 id="make-title">"Make a song"</h2>
                    <span class="hint">"Describe the sound. Leave the settings on Auto and Maio picks them."</span>
                    <button type="button" class="icon settings-toggle" node_ref=toggle aria-controls="make-settings"
                        aria-expanded=move || open.get().to_string() aria-label=toggle_label data-tip=toggle_label
                        on:click=move |_| open.update(|o| *o = !*o)>
                        <Svg icon=Icon::Settings size=20/>
                        <span class="badge" aria-hidden="true" hidden=move || count.get() == 0>{move || count.get()}</span>
                    </button>
                </div>
                <div class="make-row">
                    <label class="prompt"><span class="sr">"What it sounds like"</span>
                        <textarea rows="3" maxlength="500" bind:value=prompt node_ref=prompt_box
                            placeholder=move || current().placeholder()
                                .unwrap_or_else(|| "Warm Rhodes and soft rain on the window, slow brushed drums, a late night in an empty office".into())></textarea>
                    </label>
                    // Off while no engine is ready, and says why: the server would refuse the song (arc42 8.8).
                    <button type="submit" class="make-button" disabled=move || busy.get() || !ready.get()
                        aria-label=move || if ready.get() { "Make the song" } else { "Make the song: no music engine is ready" }
                        data-tip=move || if ready.get() { "Make a song from this description" } else { "No music engine is ready" }>
                        <Svg icon=Icon::Make size=28/><span>"Make"</span>
                    </button>
                </div>
                <Show when=sings>
                    <div class="lyrics">
                        <div class="lyrics-top">
                            <label for="make-lyrics">"Lyrics, with [verse] and [chorus] sections"</label>
                            <div class="sets"><Pick icon=Icon::Pen label="Lyrics by" value=lyrics_by options=writers/></div>
                        </div>
                        <textarea id="make-lyrics" rows="4" maxlength="4000" placeholder="Leave empty and they are written for you" bind:value=lyrics></textarea>
                    </div>
                </Show>
                <p class="error" role="alert">{move || error.get()}</p>
            </div>
            <div class="make-settings" id="make-settings" role="group" aria-labelledby="make-settings-title" hidden=move || !open.get()>
                <div class="settings-head">
                    <h3 id="make-settings-title">"Settings"</h3>
                    // Every setting back on Auto; the person's words and lyrics stay.
                    <IconButton icon=Icon::Refresh size=18 label="Put every setting back on Auto".to_string() disabled=Signal::derive(move || count.get() == 0)
                        on_click=move || {
                            [mood, genre, energy, seed, vocals, voice, language, lyrics_by].into_iter().for_each(|s| s.set(String::new()));
                            seconds.set(None);
                            tempo.set(None);
                        }/>
                    <IconButton icon=Icon::Cancel size=18 label="Close the settings".to_string() on_click=close/>
                </div>
                <section class="set-group" aria-label="Style">
                    <h4 aria-hidden="true">"Style"</h4>
                    <div class="sets">
                        <Pick icon=Icon::Mood label="Mood" value=mood options=moods/>
                        <Pick icon=Icon::Genre label="Genre" value=genre options=genres/>
                        <Pick icon=Icon::Energy label="Energy" value=energy options=make::ENERGIES.map(|(v, l)| (v.to_string(), l.to_string())).to_vec()/>
                    </div>
                </section>
                <section class="set-group" aria-label="Song">
                    <h4 aria-hidden="true">"Song"</h4>
                    <div class="sets">
                        // On Auto until moved: then no tempo is sent and the engine decides.
                        <Slider icon=Icon::Tempo label="Tempo" value=tempo min=Signal::stored(make::SLOWEST) max=Signal::stored(make::FASTEST) step=1 page=10
                            rest=make::AUTO_AT shown=|b| format!("{b} bpm") said=format::beats/>
                        // On Auto until moved: then Maio picks a length (arc42 8.2).
                        <Slider icon=Icon::Length label="Length" value=seconds min=Signal::stored(make::SHORTEST) max=longest step=make::STEP page=60
                            rest=180 shown=|s| format::clock(f64::from(s)) said=format::spoken/>
                        <div class="set seed" class:auto=move || seed.with(|s| s.trim().is_empty()) data-tip=seed_tip>
                            <span class="set-name"><Svg icon=Icon::Seed size=16/>"Seed"</span>
                            <span class="set-control">
                                <input type="text" class="field" inputmode="numeric" placeholder="Auto" bind:value=seed
                                    aria-label=move || format!("Seed. {}", seed_tip())/>
                            </span>
                            <span class="set-value">
                                <IconButton icon=Icon::Dice size=18 label="Pick a random seed".to_string()
                                    on_click=move || seed.set(((js_sys::Math::random() * 4_294_967_296.0) as u64).to_string())/>
                            </span>
                            <span class="set-reset"><IconButton icon=Icon::Refresh size=14 label="Seed: back to Auto".to_string() on_click=move || seed.set(String::new())/></span>
                        </div>
                    </div>
                </section>
                <section class="set-group" aria-label="Voice">
                    <h4 aria-hidden="true">"Voice"</h4>
                    <div class="sets">
                        <Pick icon=Icon::Mic label="Vocals" value=vocals tip="Auto lets Maio pick instrumental or vocals each time"
                            options=make::VOCALS.map(|(v, l)| (v.to_string(), l.to_string())).to_vec()/>
                        <Show when=sings>
                            <Pick icon=Icon::Voice label="Voice" value=voice tip="A hint to the engine: it usually follows it, but not always"
                                options=make::VOICES.map(|(v, l)| (v.to_string(), l.to_string())).to_vec()/>
                            <Pick icon=Icon::Language label="Sung in" value=language options=LANGUAGES.map(|(v, l)| (v.to_string(), l.to_string())).to_vec()/>
                        </Show>
                    </div>
                </section>
            </div>
        </form>
        <Modal open=ask_noncommercial title="Non-commercial engine">
            <p>"The engine that can make this song allows only non-commercial use of what it makes. Make it anyway?"</p>
            <div class="modal-actions">
                <Button on_click=move |_| ask_noncommercial.set(false)>"Cancel"</Button>
                <Button appearance=ButtonAppearance::Primary on_click=move |_| { ask_noncommercial.set(false); send(true); }>"Make anyway"</Button>
            </div>
        </Modal>
    }
}

/// A setting as a pill (arc42 8.5): its icon and name, then a dropdown with Auto first. A caption is said with the name.
#[component]
fn Pick(
    icon: Icon,
    label: &'static str,
    value: RwSignal<String>,
    #[prop(into)] options: Signal<Vec<(String, String)>>,
    #[prop(optional)] tip: Option<&'static str>,
) -> impl IntoView {
    let name = tip.map_or(label.to_string(), |t| format!("{label}. {t}"));
    view! {
        <label class="set" data-tip=tip>
            <span class="set-name"><Svg icon size=16/>{label}</span>
            <span class="set-control">
                <select class="field" aria-label=name prop:value=move || value.get() on:change=move |ev| value.set(event_target_value(&ev))>
                    {move || options.get().into_iter().map(|(v, l)| view! { <option value=v.clone() selected=value.get_untracked() == v>{l}</option> }).collect_view()}
                </select>
            </span>
            <span class="set-value"></span>
            <span class="set-reset"></span>
        </label>
    }
}

/// A setting as a pill holding a slider, on Auto (dimmed, "Auto" in place of the value) until it is moved; ↺ goes back.
/// The value shows in a fixed width, so a drag never resizes the pill. Not a label: the reset is a second control.
#[component]
fn Slider(
    icon: Icon,
    label: &'static str,
    value: RwSignal<Option<u32>>,
    min: Signal<u32>,
    #[prop(into)] max: Signal<u32>,
    step: u32,
    /// What Page Up and Page Down move; the browser's own move is a tenth of the range.
    page: u32,
    /// Where the thumb rests on Auto.
    rest: u32,
    shown: fn(u32) -> String,
    said: fn(u32) -> String,
) -> impl IntoView {
    let at = move || value.get().unwrap_or(rest).clamp(min.get(), max.get());
    view! {
        <div class="set range" class:auto=move || value.get().is_none()>
            <span class="set-name"><Svg icon size=16/>{label}</span>
            <span class="set-control">
                <input type="range" aria-label=label min=move || min.get().to_string() max=move || max.get().to_string() step=step.to_string()
                    prop:value=move || at().to_string()
                    // How much of the track is filled, for the styled track (the native fill has no colour of its own).
                    style=move || format!("--fill: {}%", (at() - min.get()) * 100 / (max.get() - min.get()).max(1))
                    aria-valuetext=move || value.get().map_or("Auto".to_string(), |_| said(at()))
                    on:input=move |ev| value.set(event_target_value(&ev).parse().ok())
                    // A press on the thumb where it rests takes that value too, though the value does not move.
                    on:pointerdown=move |_| if value.get_untracked().is_none() { value.set(Some(at())) }
                    on:keydown=move |ev| {
                        let by: i64 = match ev.key().as_str() { "PageUp" => page.into(), "PageDown" => -i64::from(page), _ => return };
                        ev.prevent_default();
                        value.set(Some((i64::from(at()) + by).clamp(min.get_untracked().into(), max.get_untracked().into()) as u32));
                    }/>
            </span>
            <output class="set-value" aria-hidden="true">{move || value.get().map_or("Auto".to_string(), |_| shown(at()))}</output>
            // Hidden on Auto, its place kept.
            <span class="set-reset"><IconButton icon=Icon::Refresh size=14 label=format!("{label}: back to Auto") on_click=move || value.set(None)/></span>
        </div>
    }
}

#[component]
fn BeingMade() -> impl IntoView {
    let c = expect_context::<Changes>();
    let toaster = ToasterInjection::expect_context();
    view! {
        <Show when=move || c.jobs.with(|j| !j.is_empty())>
            <section class="making" aria-labelledby="making-title">
                <h2 id="making-title">"Being made"</h2>
                // Keyed by job (and whether it failed, which changes its button), so progress updates
                // change the row in place and never take keyboard focus off its Cancel button.
                <For each=move || c.jobs.get() key=|j| (j.id.clone(), j.state == JobState::Failed) children=move |j| {
                    let (id, label) = (j.id.clone(), j.label.clone());
                    let failed = j.state == JobState::Failed;
                    let now = {
                        let id = id.clone();
                        Memo::new(move |_| c.jobs.with(|jobs| jobs.iter().find(|x| x.id == id).cloned()))
                    };
                    let pct = move || now.get().map_or(0, |j| (j.progress * 100.0).round() as u32);
                    let detail = move || match now.get() {
                        Some(j) if j.state == JobState::Failed => j.error.unwrap_or_else(|| "It did not work.".into()),
                        Some(j) if j.state == JobState::Waiting => "waiting".into(),
                        Some(j) => format::time_left(j.eta_s, j.progress),
                        None => String::new(),
                    };
                    let act = move || {
                        let id = id.clone();
                        if failed {
                            c.jobs.update(|jobs| jobs.retain(|x| x.id != id));
                        } else {
                            leptos::task::spawn_local(async move {
                                if let Err(f) = api::cancel_job(&id).await {
                                    say(toaster, f.text(), ToastIntent::Error);
                                }
                            });
                        }
                    };
                    view! {
                        <div class="job" class:failed=failed>
                            <span class="job-label">{label.clone()}</span>
                            <span class="bar" role="progressbar" aria-label=format!("Making {label}") aria-valuemin="0" aria-valuemax="100" aria-valuenow=pct>
                                <span style:width=move || format!("{}%", pct())></span>
                            </span>
                            <span class="job-detail">{detail}</span>
                            <IconButton icon=Icon::Cancel size=16 on_click=act
                                label=if failed { format!("Dismiss {label}") } else { format!("Cancel {label}") }/>
                        </div>
                    }
                }/>
            </section>
        </Show>
    }
}

#[component]
fn SongList(playlists: Playlists) -> impl IntoView {
    let c = expect_context::<Changes>();
    let player = expect_context::<Player>();
    let toaster = ToasterInjection::expect_context();
    let query = RwSignal::new(String::new());
    let first = LocalResource::new(move || {
        let listening = c.listening();
        let q = query.get();
        async move {
            listening.await;
            api::songs(&q, None).await
        }
    });
    let (more, next) = (RwSignal::new(Vec::<SongView>::new()), RwSignal::new(None::<String>));
    // A reconnect reloads the list; it keeps as many rows as were shown. A new search starts over. Songs made,
    // renamed or deleted in between change it by their events.
    let searched = StoredValue::new(String::new());
    Effect::new(move |_| {
        if let Some(Ok(page)) = first.get() {
            let q = query.get_untracked();
            let keep = if searched.get_value() == q { more.with_untracked(Vec::len) } else { 0 };
            searched.set_value(q.clone());
            more.set(vec![]);
            next.set(page.next);
            if keep > 0 {
                leptos::task::spawn_local(async move {
                    while more.with_untracked(Vec::len) < keep {
                        let Some(cursor) = next.get_untracked() else { break };
                        let Ok(page) = api::songs(&q, Some(&cursor)).await else { break };
                        more.update(|m| m.extend(page.songs));
                        next.set(page.next);
                    }
                });
            }
        }
    });
    let all = move || {
        let rows: Vec<SongView> = first.get().and_then(Result::ok).map(|p| p.songs).unwrap_or_default().into_iter().chain(more.get()).collect();
        crate::ui::events::songs_now(&rows, &c.songs.get(), query.with(String::is_empty))
    };
    let load_more = move |_| {
        let Some(cursor) = next.get_untracked() else { return };
        leptos::task::spawn_local(async move {
            match api::songs(&query.get_untracked(), Some(&cursor)).await {
                Ok(page) => {
                    more.update(|m| m.extend(page.songs));
                    next.set(page.next);
                }
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        });
    };
    // One details panel for the list; `shown` is the song it shows.
    let (showing, shown) = (RwSignal::new(false), RwSignal::new(None::<Song>));
    // Rename and delete share one dialog each; `target` is the song they act on.
    let target = RwSignal::new(None::<(String, String)>);
    let (renaming, deleting, title) = (RwSignal::new(false), RwSignal::new(false), RwSignal::new(String::new()));
    let rename = move || {
        let Some((id, _)) = target.get_untracked() else { return };
        leptos::task::spawn_local(async move {
            match api::rename_song(&id, &title.get_untracked()).await {
                Ok(_) => renaming.set(false),
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        });
    };
    let delete = move || {
        let Some((id, name)) = target.get_untracked() else { return };
        deleting.set(false);
        leptos::task::spawn_local(async move {
            match api::delete_song(&id).await {
                Ok(()) => {
                    player.forget(&id);
                    say(toaster, format!("Deleted {name}."), ToastIntent::Success);
                }
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        });
    };
    let summary = move || {
        let songs = all();
        if !matches!(first.get(), Some(Ok(_))) {
            return None;
        }
        // A search says how many it found; it is a status, so screen readers hear it too.
        if !query.with(String::is_empty) {
            let more = if next.get().is_some() { "+" } else { "" };
            return Some(if songs.is_empty() { "No songs found".to_string() } else { format!("{}{more} found", format::songs(songs.len())) });
        }
        (next.get().is_none() && !songs.is_empty())
            .then(|| format!("{}, {}", format::songs(songs.len()), format::length(songs.iter().map(|s| s.song.seconds).sum())))
    };
    view! {
        <div class="list-head">
            <h1>"Songs"</h1>
            <span class="hint" role="status">{summary}</span>
            <label class="search"><Svg icon=Icon::Search size=16/><span class="sr">"Search songs"</span>
                <input type="search" placeholder="Search titles, moods, lyrics" bind:value=query/>
            </label>
        </div>
        <div class="songs" role=move || (matches!(first.get(), Some(Ok(_))) && !all().is_empty()).then_some("list")>
            {move || {
                // Loading shows nothing and a failure says so: "No songs yet" only when the library really is empty.
                match first.get() {
                    None => return ().into_any(),
                    Some(Err(f)) => return view! {
                        <p class="error" role="alert">{f.text()} " " <button type="button" class="more" on:click=move |_| first.refetch()>"Try again"</button></p>
                    }.into_any(),
                    Some(Ok(_)) => {}
                }
                let songs = all();
                if songs.is_empty() {
                    let text = if query.with(String::is_empty) { "No songs yet. Describe one above and press Make." } else { "No song matches that search." };
                    return view! { <p class="empty">{text}</p> }.into_any();
                }
                let queue: Vec<QSong> = songs.iter().map(|s| QSong::from(&s.song)).collect();
                let lists = loaded(playlists);
                songs.into_iter().enumerate().map(|(i, s)| {
                    let queue = queue.clone();
                    let (id, name) = (s.song.id.clone(), s.song.title.clone());
                    let marks: Vec<&'static str> = s.playlists.iter().map(|p| shelf::colour(&lists, &p.id)).collect();
                    let add_to = lists.clone();
                    let on_add = {
                        let (id, name) = (id.clone(), name.clone());
                        move |pid: String| {
                            let (id, name) = (id.clone(), name.clone());
                            let to = add_to.iter().find(|p| p.id == pid).map(|p| p.name.clone()).unwrap_or_default();
                            leptos::task::spawn_local(async move {
                                match api::add_songs(&pid, &[id]).await {
                                    Ok(_) => say(toaster, format!("Added {name} to {to}."), ToastIntent::Success),
                                    Err(f) => say(toaster, f.text(), ToastIntent::Error),
                                }
                            });
                        }
                    };
                    let on_more = {
                        let (id, name, song) = (id.clone(), name.clone(), s.song.clone());
                        move |key: String| {
                            if key == "details" {
                                shown.set(Some(song.clone()));
                                showing.set(true);
                                return;
                            }
                            target.set(Some((id.clone(), name.clone())));
                            if key == "rename" {
                                title.set(name.clone());
                                renaming.set(true);
                            } else {
                                deleting.set(true);
                            }
                        }
                    };
                    let items: Vec<(String, String)> = lists.iter().map(|p| (p.id.clone(), p.name.clone())).collect();
                    let (add_label, more_label) = (format!("Add {name} to a playlist"), format!("Details, rename or delete {name}"));
                    let subtitle = if s.song.prompt.is_empty() { s.song.mood.clone().unwrap_or_default() } else { s.song.prompt.clone() };
                    view! {
                        <div class="song" role="listitem">
                            <IconButton class="play" size=14
                                icon={ let id = id.clone(); Signal::derive(move || if player.is_playing(&id) { Icon::Pause } else { Icon::Play }) }
                                label={ let (id, name) = (id.clone(), name.clone()); Signal::derive(move || format!("{} {name}", if player.is_playing(&id) { "Pause" } else { "Play" })) }
                                on_click={ let id = id.clone(); move || if player.is_current(&id) { player.toggle() } else { player.play(queue.clone(), i, None) } }/>
                            <span class="song-main">
                                <button type="button" class="song-title" aria-label=format!("Details for {name}") data-tip="How it was made, its lyrics and downloads"
                                    on:click={ let s = s.song.clone(); move |_| { shown.set(Some(s.clone())); showing.set(true); } }>{name.clone()}</button>
                                <span class="song-prompt">{subtitle}</span>
                            </span>
                            <span class="marks" role="img" data-tip=format::in_playlists(&s.playlists) aria-label=format::in_playlists(&s.playlists)>
                                {marks.into_iter().take(3).map(|c| view! { <i style:background=c></i> }).collect_view()}
                                <span class="marks-name">{format::playlist_mark(&s.playlists)}</span>
                            </span>
                            <span class="meta">{format::meta(&s.song)}</span>
                            <span class="len">{format::clock(s.song.seconds)}</span>
                            <Dropdown icon=Icon::AddTo label=add_label items on_select=on_add empty="Make a playlist first, with + on the shelf."/>
                            <Dropdown icon=Icon::More label=more_label class="muted" size=18 on_select=on_more
                                items=vec![("details".into(), "Details".into()), ("rename".into(), "Rename".into()), ("delete".into(), "Delete".into())]/>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </div>
        <Show when=move || next.get().is_some()>
            <button type="button" class="more" on:click=load_more>"Show more songs"</button>
        </Show>
        <crate::ui::details::SongDetails open=showing song=shown/>
        <Modal open=renaming title="Rename song">
            <Field label="Title"><Input value=title/></Field>
            <div class="modal-actions">
                <Button on_click=move |_| renaming.set(false)>"Cancel"</Button>
                <Button appearance=ButtonAppearance::Primary on_click=move |_| rename()>"Rename"</Button>
            </div>
        </Modal>
        <Modal open=deleting title=Signal::derive(move || format!("Delete {}?", target.get().map(|t| t.1).unwrap_or_default()))>
            <p>"The song and its file are deleted. This cannot be undone."</p>
            <div class="modal-actions">
                <Button on_click=move |_| deleting.set(false)>"Cancel"</Button>
                <Button appearance=ButtonAppearance::Primary on_click=move |_| delete()>"Delete"</Button>
            </div>
        </Modal>
    }
}
