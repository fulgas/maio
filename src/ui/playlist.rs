//! A playlist (arc42 8.3, 8.5): its coloured band, and its songs in order, dragged to reorder.

use std::collections::HashSet;

use leptos::{ev::KeyboardEvent, prelude::*};
use leptos_router::hooks::{use_navigate, use_params_map};
use thaw::{Button, ButtonAppearance, Checkbox, CheckboxGroup, Field, Input, ToastIntent, ToasterInjection};
use wasm_bindgen::JsCast;

use crate::{
    domain::{PlaylistRef, Song},
    ui::{
        api,
        events::Changes,
        format,
        icons::{Icon, IconButton, Svg},
        menu::Dropdown,
        modal::Modal,
        player::Player,
        queue::QSong,
        say, shelf,
    },
};

#[component]
pub fn PlaylistPage() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let c = expect_context::<Changes>();
    let player = expect_context::<Player>();
    let toaster = ToasterInjection::expect_context();
    let navigate = use_navigate();
    let view_res = LocalResource::new(move || {
        c.playlists.track();
        c.songs.track();
        c.live.track();
        let id = id.get();
        async move { api::playlist(&id).await }
    });
    let all = LocalResource::new(move || {
        c.playlists.track();
        api::playlists()
    });
    // The order on screen; set from the server, changed at once on drag, then saved.
    let songs = RwSignal::new(Vec::<Song>::new());
    let name = RwSignal::new(String::new());
    Effect::new(move |_| {
        if let Some(Ok(v)) = view_res.get() {
            songs.set(v.songs);
            name.set(v.playlist.name);
        }
    });
    let missing = move || matches!(view_res.get(), Some(Err(f)) if f.status == 404);
    let failed = move || match view_res.get() {
        Some(Err(f)) if f.status != 404 => Some(f.text()),
        _ => None,
    };
    let loaded = move || matches!(view_res.get(), Some(Ok(_)));
    let is_live = move || view_res.get().and_then(Result::ok).is_some_and(|v| v.playlist.live.on);
    let colour = move || all.get().and_then(Result::ok).map_or(shelf::COLOURS[0], |a| shelf::colour(&a, &id.get()));
    let from = move || Some(PlaylistRef { id: id.get_untracked(), name: name.get_untracked() });
    let queue = move || songs.get_untracked().iter().map(QSong::from).collect::<Vec<_>>();
    // One save at a time, always of the newest order: fast arrow presses must not race each other to the server.
    let saving = StoredValue::new(false);
    let save_order = move |next: Vec<Song>| {
        songs.set(next);
        if saving.get_value() {
            return; // the save in flight sends the newest order when it is done
        }
        saving.set_value(true);
        leptos::task::spawn_local(async move {
            loop {
                let ids: Vec<String> = songs.with_untracked(|s| s.iter().map(|x| x.id.clone()).collect());
                if let Err(f) = api::reorder(&id.get_untracked(), &ids).await {
                    say(toaster, f.text(), ToastIntent::Error);
                    view_res.refetch();
                    break;
                }
                if songs.with_untracked(|s| s.iter().map(|x| &x.id).eq(ids.iter())) {
                    break;
                }
            }
            saving.set_value(false);
        });
    };
    let dragging = RwSignal::new(None::<usize>);
    let (showing, shown) = (RwSignal::new(false), RwSignal::new(None::<Song>));
    // The live stream's song now and next, from the live strip, for the marks on the rows.
    let on_air = RwSignal::new(None::<(Option<String>, Option<String>)>);

    // Band actions: rename, delete, add songs.
    let (renaming, deleting, adding) = (RwSignal::new(false), RwSignal::new(false), RwSignal::new(false));
    let on_more = more(renaming, deleting);

    view! {
        {move || failed().map(|text| view! {
            <p class="error" role="alert">{text} " " <button type="button" class="more" on:click=move |_| view_res.refetch()>"Try again"</button></p>
        })}
        {move || missing().then(|| view! { <p class="empty">"This playlist does not exist. " <a href="/">"Go to your songs"</a></p> })}
        <section class="band" aria-labelledby="pl-title" style:background=colour style:--band=colour class:hidden=move || !loaded()>
            <div class="band-text">
                <h1 id="pl-title">{move || name.get()}</h1>
                <span>{move || songs.with(|s| format!("{}, {}", format::songs(s.len()), format::length(s.iter().map(|x| x.seconds).sum())))}</span>
            </div>
            <div class="band-actions">
                // Play turns to Pause while this playlist plays; pressing it again pauses or resumes.
                <IconButton class="solid big" size=22
                    icon=Signal::derive(move || if player.is_playing_list(&id.get()) { Icon::Pause } else { Icon::Play })
                    label=Signal::derive(move || format!("{} {}", if player.is_playing_list(&id.get()) { "Pause" } else { "Play" }, name.get()))
                    on_click=move || {
                        if let Some(from) = from() {
                            player.play_list(from, Some(queue()))
                        }
                    }/>
                // A one-shot, not a second shuffle switch: the player's shuffle is the switch.
                // Live, Play plays the stream, whose order is the live strip's shuffle.
                <Show when=move || !is_live()>
                    <IconButton icon=Icon::Shuffle class="solid" label="Shuffle play: start it in random order".to_string()
                        on_click=move || if let Some(from) = from() { player.shuffle_play(queue(), from) }/>
                </Show>
                <IconButton icon=Icon::Add class="outline" label="Add songs".to_string() on_click=move || adding.set(true)/>
                <Dropdown icon=Icon::More label="Rename or delete the playlist" class="outline" on_select=on_more items=more_items()/>
            </div>
        </section>
        <crate::ui::live::LiveStrip playlist=id on_air/>
        <ol class="tracks" aria-label="Songs in play order">
            {move || {
                let list = songs.get();
                if list.is_empty() && loaded() {
                    return view! { <li class="empty">"No songs yet. Add some with the + button, or from your songs."</li> }.into_any();
                }
                let playing = player.queue.with(|q| q.current().map(|s| s.id.clone()));
                list.into_iter().enumerate().map(|(i, s)| {
                    let title = s.title.clone();
                    let sid = s.id.clone();
                    let here = playing.as_deref() == Some(s.id.as_str());
                    let keys = move |ev: KeyboardEvent| {
                        let last = songs.with_untracked(Vec::len).saturating_sub(1);
                        let to = match ev.key().as_str() {
                            "ArrowUp" if i > 0 => i - 1,
                            "ArrowDown" if i < last => i + 1,
                            _ => return,
                        };
                        ev.prevent_default();
                        save_order(shelf::moved(&songs.get_untracked(), i, to));
                        // The rows were rebuilt: put focus back on the moved song's grip, so the next arrow press works.
                        request_animation_frame(move || {
                            let grip = document().query_selector_all(".tracks .grip").ok().and_then(|g| g.item(to as u32));
                            if let Some(el) = grip.and_then(|g| g.dyn_into::<web_sys::HtmlElement>().ok()) {
                                let _ = el.focus();
                            }
                        });
                    };
                    view! {
                        <li class="track" class:here=here draggable="true"
                            on:dragstart=move |ev| {
                                dragging.set(Some(i));
                                if let Some(dt) = ev.data_transfer() { let _ = dt.set_data("text/plain", &i.to_string()); }
                            }
                            on:dragover=move |ev| ev.prevent_default()
                            on:drop=move |ev| {
                                ev.prevent_default();
                                if let Some(from) = dragging.get_untracked() { save_order(shelf::moved(&songs.get_untracked(), from, i)); }
                                dragging.set(None);
                            }>
                            <button type="button" class="grip" aria-label=format!("Move {title}: arrow keys move it up or down") data-tip="Drag, or use the arrow keys, to reorder" on:keydown=keys>
                                <Svg icon=Icon::Grip size=16/>
                            </button>
                            <span class="n">{i + 1}</span>
                            <IconButton size=14
                                icon={ let sid = s.id.clone(); Signal::derive(move || if player.is_playing(&sid) { Icon::Pause } else { Icon::Play }) }
                                label={ let (sid, title) = (s.id.clone(), title.clone()); Signal::derive(move || format!("{} {title}", if player.is_playing(&sid) { "Pause" } else { "Play" })) }
                                on_click={ let sid = s.id.clone(); move || if player.is_current(&sid) { player.toggle() } else { player.play(queue(), i, from()) } }/>
                            <span class="track-title">
                                <button type="button" aria-label=format!("Details for {title}") data-tip="How it was made, its lyrics and downloads"
                                    on:click={ let s = s.clone(); move |_| { shown.set(Some(s.clone())); showing.set(true); } }>{title.clone()}</button>
                                {
                                    // Its own closure: the on-air marks change every song, the rows must not be rebuilt.
                                    let sid = s.id.clone();
                                    move || {
                                    let (now, next) = on_air.get().unwrap_or_default();
                                    let on = now.as_deref() == Some(sid.as_str());
                                    let soon = next.as_deref() == Some(sid.as_str());
                                    view! {
                                        {on.then(|| view! { <span class="on-air" role="img" aria-label="On the live stream now" data-tip="On the live stream now"><Svg icon=Icon::Live size=16/></span> })}
                                        {soon.then(|| view! { <span class="next-label">"next"</span> })}
                                    }
                                    }
                                }
                            </span>
                            <span class="meta">{format::meta(&s)}</span>
                            <span class="meta bpm">{s.bpm.map(|b| format!("{b} bpm")).unwrap_or_default()}</span>
                            <span class="len">{format::clock(s.seconds)}</span>
                            <IconButton icon=Icon::Remove class="muted" size=18 label=format!("Remove {title} from this playlist")
                                on_click=move || {
                                    let sid = sid.clone();
                                    leptos::task::spawn_local(async move {
                                        match api::remove_song(&id.get_untracked(), &sid).await {
                                            Ok(_) => songs.update(|v| v.retain(|x| x.id != sid)),
                                            Err(f) => say(toaster, f.text(), ToastIntent::Error),
                                        }
                                    });
                                }/>
                        </li>
                    }
                }).collect_view().into_any()
            }}
        </ol>
        <AddSongs open=adding playlist=id/>
        <crate::ui::details::SongDetails open=showing song=shown/>
        <PlaylistDialogs renaming deleting id name
            on_renamed=Callback::new(move |n| name.set(n))
            on_deleted=Callback::new(move |()| navigate("/", Default::default()))/>
    }
}

/// The More menu's choices for a playlist: on its band and on its row in Playlists.
pub fn more_items() -> Vec<(String, String)> {
    vec![("rename".into(), "Rename".into()), ("delete".into(), "Delete".into())]
}

/// What a More menu pick opens.
pub fn more(renaming: RwSignal<bool>, deleting: RwSignal<bool>) -> impl Fn(String) + Clone + Send + Sync + 'static {
    move |key: String| if key == "rename" { renaming.set(true) } else { deleting.set(true) }
}

/// Rename and Delete for one playlist, shared by its band and the Playlists screen.
#[component]
pub fn PlaylistDialogs(
    renaming: RwSignal<bool>,
    deleting: RwSignal<bool>,
    #[prop(into)] id: Signal<String>,
    #[prop(into)] name: Signal<String>,
    on_renamed: Callback<String>,
    on_deleted: Callback<()>,
) -> impl IntoView {
    let c = expect_context::<Changes>();
    let toaster = ToasterInjection::expect_context();
    let new_name = RwSignal::new(String::new());
    Effect::new(move |_| {
        if renaming.get() {
            new_name.set(name.get_untracked());
        }
    });
    let rename = move || {
        leptos::task::spawn_local(async move {
            match api::rename_playlist(&id.get_untracked(), &new_name.get_untracked()).await {
                Ok(p) => {
                    renaming.set(false);
                    c.playlists.update(|n| *n += 1);
                    on_renamed.run(p.name);
                }
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        })
    };
    let delete = move || {
        deleting.set(false);
        leptos::task::spawn_local(async move {
            match api::delete_playlist(&id.get_untracked()).await {
                Ok(()) => {
                    say(toaster, format!("Deleted {}.", name.get_untracked()), ToastIntent::Success);
                    c.playlists.update(|n| *n += 1);
                    on_deleted.run(());
                }
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        });
    };
    view! {
        <Modal open=renaming title="Rename playlist">
            <Field label="Name"><Input value=new_name/></Field>
            <div class="modal-actions">
                <Button on_click=move |_| renaming.set(false)>"Cancel"</Button>
                <Button appearance=ButtonAppearance::Primary on_click=move |_| rename()>"Rename"</Button>
            </div>
        </Modal>
        <Modal open=deleting title=Signal::derive(move || format!("Delete {}?", name.get()))>
            <p>"The playlist goes; its songs stay in your library."</p>
            <div class="modal-actions">
                <Button on_click=move |_| deleting.set(false)>"Cancel"</Button>
                <Button appearance=ButtonAppearance::Primary on_click=move |_| delete()>"Delete"</Button>
            </div>
        </Modal>
    }
}

/// "Add songs": search the library, tick songs, add them at the end (songs already in it are skipped).
#[component]
fn AddSongs(open: RwSignal<bool>, playlist: Memo<String>) -> impl IntoView {
    let toaster = ToasterInjection::expect_context();
    let query = RwSignal::new(String::new());
    let picked = RwSignal::new(HashSet::<String>::new());
    let found = LocalResource::new(move || {
        let (q, open) = (query.get(), open.get());
        async move { if open { api::songs(&q, None).await.map(|p| p.songs).unwrap_or_default() } else { vec![] } }
    });
    let add = move || {
        leptos::task::spawn_local(async move {
            let ids: Vec<String> = picked.get_untracked().into_iter().collect();
            match api::add_songs(&playlist.get_untracked(), &ids).await {
                Ok(_) => {
                    say(toaster, format!("Added {}.", format::songs(ids.len())), ToastIntent::Success);
                    picked.set(HashSet::new());
                    open.set(false);
                }
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        })
    };
    view! {
        <Modal open title="Add songs">
                    <label class="search"><Svg icon=Icon::Search size=16/><span class="sr">"Search songs"</span>
                        <input type="search" placeholder="Search titles, moods, lyrics" bind:value=query/></label>
                    <div class="pick">
                        <CheckboxGroup value=picked>
                            {move || found.get().unwrap_or_default().into_iter()
                                .map(|s| view! { <Checkbox value=s.song.id.clone() label=s.song.title/> })
                                .collect_view()}
                        </CheckboxGroup>
                    </div>
                <div class="modal-actions">
                    <Button on_click=move |_| open.set(false)>"Cancel"</Button>
                    <Button appearance=ButtonAppearance::Primary on_click=move |_| add()
                        disabled=Signal::derive(move || picked.with(HashSet::is_empty))>
                        {move || format!("Add {}", format::songs(picked.with(HashSet::len)))}
                    </Button>
                </div>
        </Modal>
    }
}
