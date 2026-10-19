//! The Songs screen (SPEC 12): the shelf of playlists, "Make a song", "Being made" and the song list.

use leptos::{ev::SubmitEvent, prelude::*};
use leptos_router::hooks::use_navigate;
use thaw::{
    Button, ButtonAppearance, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input, ToastIntent,
    ToasterInjection,
};

use crate::{
    domain::job::JobState,
    ui::{
        api::{self, Failure, PlaylistSummary, SongView, Status},
        events::Changes,
        format,
        icons::{Icon, IconButton, Svg},
        make::{LANGUAGES, LENGTHS, MakeForm, TEMPOS},
        menu::Dropdown,
        player::Player,
        queue::QSong,
        say, shelf,
    },
};

type Playlists = LocalResource<Result<Vec<PlaylistSummary>, Failure>>;

#[component]
pub fn SongsPage() -> impl IntoView {
    let c = expect_context::<Changes>();
    let playlists: Playlists = LocalResource::new(move || {
        c.playlists.track();
        api::playlists()
    });
    let status = LocalResource::new(api::status);
    view! {
        <Shelf playlists/>
        <MakeSong status/>
        <BeingMade/>
        <SongList playlists/>
    }
}

/// Playlists (SPEC 12): every playlist as a tile, "+" first to make one.
#[component]
pub fn PlaylistsPage() -> impl IntoView {
    let c = expect_context::<Changes>();
    let playlists: Playlists = LocalResource::new(move || {
        c.playlists.track();
        api::playlists()
    });
    let summary = move || {
        playlists.get().and_then(Result::ok).map(|all| match all.len() {
            0 => "None yet. Make one with +, then add songs to it from Songs.".to_string(),
            1 => "1 playlist".to_string(),
            n => format!("{n} playlists"),
        })
    };
    view! {
        <div class="list-head">
            <h1>"Playlists"</h1>
            <span class="hint">{summary}</span>
        </div>
        <Shelf playlists grid=true/>
    }
}

fn loaded(playlists: Playlists) -> Vec<PlaylistSummary> {
    playlists.get().and_then(Result::ok).unwrap_or_default()
}

/// The playlist tiles: a sideways row on Songs, a wrapping grid (`grid`) on Playlists.
#[component]
fn Shelf(playlists: Playlists, #[prop(optional)] grid: bool) -> impl IntoView {
    let toaster = ToasterInjection::expect_context();
    let navigate = use_navigate();
    let (open, name, busy) = (RwSignal::new(false), RwSignal::new(String::new()), RwSignal::new(false));
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
    let tiles = move || {
        let all = loaded(playlists);
        shelf::order(&all, &shelf::played()).into_iter().map(|p| (shelf::colour(&all, &p.id), p)).collect::<Vec<_>>()
    };
    view! {
        <section class="shelf" class:grid=grid aria-label="Your playlists">
            <button type="button" class="tile new" aria-label="New playlist" data-tip="Make a new playlist" on:click=move |_| open.set(true)>
                <Svg icon=Icon::Add size=28/>
            </button>
            <For each=tiles key=|(_, p)| (p.id.clone(), p.name.clone(), p.songs, p.live.on) children=move |(colour, p)| view! {
                <a class="tile" href=format!("/playlists/{}", p.id) style:background=colour>
                    <span class="tile-name">{p.name.clone()}</span>
                    <span class="tile-count">
                        {format::songs(p.songs)}
                        {p.live.on.then(|| view! { <span class="live-mark" role="img" aria-label="Live" data-tip="Live: streaming now"><Svg icon=Icon::Live size=16/></span> })}
                    </span>
                </a>
            }/>
        </section>
        <Dialog open>
            <DialogSurface>
                <DialogBody>
                    <DialogTitle>"New playlist"</DialogTitle>
                    <DialogContent><Field label="Name"><Input value=name/></Field></DialogContent>
                    <DialogActions>
                        <Button on_click=move |_| open.set(false)>"Cancel"</Button>
                        <Button appearance=ButtonAppearance::Primary on_click=move |_| create()>"Create"</Button>
                    </DialogActions>
                </DialogBody>
            </DialogSurface>
        </Dialog>
    }
}

#[component]
fn MakeSong(status: LocalResource<Result<Status, Failure>>) -> impl IntoView {
    let c = expect_context::<Changes>();
    let f = MakeForm::default();
    let (prompt, mood, genre) = (RwSignal::new(f.prompt), RwSignal::new(f.mood), RwSignal::new(f.genre));
    let (seconds, tempo, seed) = (RwSignal::new(f.seconds), RwSignal::new(f.tempo), RwSignal::new(f.seed));
    let (vocals, lyrics, language, lyrics_by) =
        (RwSignal::new(false), RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()));
    let picking = RwSignal::new(false);
    let (error, ask_noncommercial, busy) = (RwSignal::new(None::<String>), RwSignal::new(false), RwSignal::new(false));
    let form = move || MakeForm {
        prompt: prompt.get_untracked(),
        mood: mood.get_untracked(),
        genre: genre.get_untracked(),
        seconds: seconds.get_untracked(),
        tempo: tempo.get_untracked(),
        seed: seed.get_untracked(),
        vocals: vocals.get_untracked(),
        lyrics: lyrics.get_untracked(),
        language: language.get_untracked(),
        lyrics_by: lyrics_by.get_untracked(),
    };
    // One request at a time: a double click must not make two songs.
    let send = move |allow: bool| match form().request(allow) {
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
    let st = move || status.get().and_then(Result::ok);
    let moods = move || st().map(|s| s.moods).unwrap_or_default();
    let genres = move || st().map(|s| s.genres).unwrap_or_default();
    let writers = move || st().map(|s| s.text_engines.into_iter().map(|t| t.id).collect::<Vec<_>>()).unwrap_or_default();
    view! {
        <form class="make" aria-labelledby="make-title" on:submit=submit>
            <div class="make-head">
                <h2 id="make-title">"Make a song"</h2>
                <span class="hint">"Describe the sound; the settings below are optional."</span>
            </div>
            <div class="make-row">
                <label class="prompt"><span class="sr">"What it sounds like"</span>
                    <textarea rows="3" maxlength="500" bind:value=prompt
                        placeholder="Warm Rhodes and soft rain on the window, slow brushed drums, a late night in an empty office"></textarea>
                </label>
                <button type="submit" class="make-button" aria-label="Make the song" data-tip="Make a song from this description" disabled=move || busy.get()>
                    <Svg icon=Icon::Make size=28/><span>"Make"</span>
                </button>
            </div>
            <div class="chips" role="group" aria-label="Song settings">
                <Chip icon=Icon::Mood label="Mood" value=mood options=Signal::derive(move || std::iter::once((String::new(), "Any mood".to_string())).chain(moods().into_iter().map(|m| (m.clone(), m))).collect())/>
                <Chip icon=Icon::Genre label="Genre" value=genre options=Signal::derive(move || std::iter::once((String::new(), "Any genre".to_string())).chain(genres().into_iter().map(|g| (g.clone(), g))).collect())/>
                <label class="chip" data-tip="How long the song is"><Svg icon=Icon::Length size=18/>
                    <select aria-label="Length" prop:value=move || seconds.get().to_string() on:change=move |ev| seconds.set(event_target_value(&ev).parse().unwrap_or(180))>
                        {LENGTHS.iter().map(|(s, l)| view! { <option value=s.to_string()>{*l}</option> }).collect_view()}
                    </select>
                </label>
                <Chip icon=Icon::Tempo label="Tempo" value=tempo options=Signal::derive(|| TEMPOS.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect())/>
                <button type="button" class="chip toggle" aria-pressed=move || vocals.get().to_string() data-tip=move || if vocals.get() { "Sung, with lyrics. Press for an instrumental" } else { "Instrumental. Press to add vocals and lyrics" } on:click=move |_| vocals.update(|v| *v = !*v)>
                    <Svg icon=Icon::Mic size=18/>{move || if vocals.get() { "Vocals" } else { "Instrumental" }}
                </button>
                // The dice alone means a random seed; it opens a field only for someone who wants a fixed one.
                <span class="chip seed" class:open=move || picking.get() || !seed.with(String::is_empty)>
                    <IconButton icon=Icon::Seed class="bare" size=18 on_click=move || picking.update(|p| *p = !*p)
                        label=Signal::derive(move || seed.with(|s| if s.trim().is_empty() {
                            "Random seed: every song comes out different. Press to set one and make the same song again".to_string()
                        } else {
                            format!("Seed {}: the same seed and settings make the same song", s.trim())
                        }))/>
                    <Show when=move || picking.get() || !seed.with(String::is_empty)>
                        <input aria-label="Seed" inputmode="numeric" placeholder="seed" bind:value=seed
                            on:blur=move |_| if seed.with(|s| s.trim().is_empty()) { picking.set(false) }/>
                    </Show>
                </span>
            </div>
            <Show when=move || vocals.get()>
                <div class="lyrics">
                    <label>"Lyrics, with [verse] and [chorus] sections"
                        <textarea rows="4" maxlength="4000" placeholder="Leave empty and they are written for you" bind:value=lyrics></textarea>
                    </label>
                    <div class="lyrics-side">
                        <label>"Sung in"
                            <select class="field" prop:value=move || language.get() on:change=move |ev| language.set(event_target_value(&ev))>
                                {LANGUAGES.iter().map(|(v, l)| view! { <option value=*v>{*l}</option> }).collect_view()}
                            </select>
                        </label>
                        <label>"If empty, written by"
                            <select class="field" prop:value=move || lyrics_by.get() on:change=move |ev| lyrics_by.set(event_target_value(&ev))>
                                <option value="">"Default writer"</option>
                                <option value="ace-step">"ACE-Step"</option>
                                {move || writers().into_iter().map(|w| view! { <option value=w>{w.clone()}</option> }).collect_view()}
                            </select>
                        </label>
                    </div>
                </div>
            </Show>
            <p class="error" role="alert">{move || error.get()}</p>
        </form>
        <Dialog open=ask_noncommercial>
            <DialogSurface>
                <DialogBody>
                    <DialogTitle>"Non-commercial engine"</DialogTitle>
                    <DialogContent>"The engine that can make this song allows only non-commercial use of what it makes. Make it anyway?"</DialogContent>
                    <DialogActions>
                        <Button on_click=move |_| ask_noncommercial.set(false)>"Cancel"</Button>
                        <Button appearance=ButtonAppearance::Primary on_click=move |_| { ask_noncommercial.set(false); send(true); }>"Make anyway"</Button>
                    </DialogActions>
                </DialogBody>
            </DialogSurface>
        </Dialog>
    }
}

/// A setting as an icon chip over a native select.
#[component]
fn Chip(icon: Icon, label: &'static str, value: RwSignal<String>, #[prop(into)] options: Signal<Vec<(String, String)>>) -> impl IntoView {
    view! {
        <label class="chip" data-tip=format!("{label} of the song")><Svg icon size=18/>
            <select aria-label=label prop:value=move || value.get() on:change=move |ev| value.set(event_target_value(&ev))>
                {move || options.get().into_iter().map(|(v, l)| view! { <option value=v>{l}</option> }).collect_view()}
            </select>
        </label>
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
        c.songs.track();
        let q = query.get();
        async move { api::songs(&q, None).await }
    });
    let (more, next) = (RwSignal::new(Vec::<SongView>::new()), RwSignal::new(None::<String>));
    // A change to the library reloads the list; it keeps as many rows as were shown. A new search starts over.
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
    let all = move || first.get().and_then(Result::ok).map(|p| p.songs).unwrap_or_default().into_iter().chain(more.get()).collect::<Vec<_>>();
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
        (next.get().is_none() && query.with(String::is_empty) && !songs.is_empty())
            .then(|| format!("{}, {}", format::songs(songs.len()), format::length(songs.iter().map(|s| s.song.seconds).sum())))
    };
    view! {
        <div class="list-head">
            <h1>"Songs"</h1>
            <span class="hint">{summary}</span>
            <label class="search"><Svg icon=Icon::Search size=16/><span class="sr">"Search songs"</span>
                <input type="search" placeholder="Search titles, moods, lyrics" bind:value=query/>
            </label>
        </div>
        <div class="songs" role="list">
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
                        let (id, name) = (id.clone(), name.clone());
                        move |key: String| {
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
                    let (add_label, more_label) = (format!("Add {name} to a playlist"), format!("Rename or delete {name}"));
                    let subtitle = if s.song.prompt.is_empty() { s.song.mood.clone().unwrap_or_default() } else { s.song.prompt.clone() };
                    view! {
                        <div class="song" role="listitem">
                            <IconButton class="play" size=14
                                icon={ let id = id.clone(); Signal::derive(move || if player.is_playing(&id) { Icon::Pause } else { Icon::Play }) }
                                label={ let (id, name) = (id.clone(), name.clone()); Signal::derive(move || format!("{} {name}", if player.is_playing(&id) { "Pause" } else { "Play" })) }
                                on_click={ let id = id.clone(); move || if player.is_current(&id) { player.toggle() } else { player.play(queue.clone(), i, None) } }/>
                            <span class="song-main">
                                <span class="song-title">{name}</span>
                                <span class="song-prompt">{subtitle}</span>
                            </span>
                            <span class="marks" data-tip=format::in_playlists(&s.playlists) aria-label=format::in_playlists(&s.playlists)>
                                {marks.into_iter().map(|c| view! { <i style:background=c></i> }).collect_view()}
                            </span>
                            <span class="meta">{format::meta(&s.song)}</span>
                            <span class="len">{format::clock(s.song.seconds)}</span>
                            <Dropdown icon=Icon::AddTo label=add_label items on_select=on_add empty="Make a playlist first, with + on the shelf."/>
                            <Dropdown icon=Icon::More label=more_label class="muted" size=18 on_select=on_more
                                items=vec![("rename".into(), "Rename".into()), ("delete".into(), "Delete".into())]/>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </div>
        <Show when=move || next.get().is_some()>
            <button type="button" class="more" on:click=load_more>"Show more songs"</button>
        </Show>
        <Dialog open=renaming>
            <DialogSurface><DialogBody>
                <DialogTitle>"Rename song"</DialogTitle>
                <DialogContent><Field label="Title"><Input value=title/></Field></DialogContent>
                <DialogActions>
                    <Button on_click=move |_| renaming.set(false)>"Cancel"</Button>
                    <Button appearance=ButtonAppearance::Primary on_click=move |_| rename()>"Rename"</Button>
                </DialogActions>
            </DialogBody></DialogSurface>
        </Dialog>
        <Dialog open=deleting>
            <DialogSurface><DialogBody>
                <DialogTitle>{move || format!("Delete {}?", target.get().map(|t| t.1).unwrap_or_default())}</DialogTitle>
                <DialogContent>"The song and its file are deleted. This cannot be undone."</DialogContent>
                <DialogActions>
                    <Button on_click=move |_| deleting.set(false)>"Cancel"</Button>
                    <Button appearance=ButtonAppearance::Primary on_click=move |_| delete()>"Delete"</Button>
                </DialogActions>
            </DialogBody></DialogSurface>
        </Dialog>
    }
}
