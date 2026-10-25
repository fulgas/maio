//! The live strip under a playlist's band (arc42 8.4, 8.5): the switch, listeners, public or private, stream shuffle,
//! a new private link, the addresses with copy buttons, and how many streams are in use.

use leptos::prelude::*;
use thaw::{ToastIntent, ToasterInjection};

use crate::{
    domain::Visibility,
    ui::{
        api,
        events::Changes,
        icons::{Icon, IconButton, Svg},
        say,
    },
};

#[component]
pub fn LiveStrip(playlist: Memo<String>, on_air: RwSignal<Option<(Option<String>, Option<String>)>>) -> impl IntoView {
    let c = expect_context::<Changes>();
    let toaster = ToasterInjection::expect_context();
    let state = LocalResource::new(move || {
        c.live.track();
        let id = playlist.get();
        async move { api::live(&id).await }
    });
    // A memo, so a refetch that changes nothing redraws nothing (and keeps keyboard focus where it is).
    let current = Memo::new(move |_| state.get().and_then(Result::ok));
    // The song on air and the next one, for the marks in the song list; set only when they change.
    Effect::new(move |_| {
        let v = current.get().and_then(|s| s.on_air).map(|o| (o.now.map(|n| n.id), o.next.map(|n| n.id)));
        if on_air.get_untracked() != v {
            on_air.set(v);
        }
    });
    let change = move |body: serde_json::Value| {
        leptos::task::spawn_local(async move {
            match api::set_live(&playlist.get_untracked(), &body).await {
                Ok(_) => state.refetch(),
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        });
    };
    let new_link = move || {
        leptos::task::spawn_local(async move {
            match api::new_key(&playlist.get_untracked()).await {
                Ok(_) => {
                    say(toaster, "New private link made. The old one has stopped working.".into(), ToastIntent::Success);
                    state.refetch();
                }
                Err(f) => say(toaster, f.text(), ToastIntent::Error),
            }
        });
    };
    let copy = move |full: String, kind: &'static str| {
        leptos::task::spawn_local(async move {
            if clipboard_write(&full).await || copy_by_selection(&full) {
                say(toaster, format!("{kind} link copied."), ToastIntent::Success);
            } else {
                say(toaster, format!("Could not copy the {kind} link. Here it is: {full}"), ToastIntent::Error);
            }
        });
    };
    view! {
        <section class="live-strip" aria-label="Live stream">
            {move || {
                let Some(s) = current.get() else { return ().into_any() };
                let on = s.live;
                let private = s.visibility == Visibility::Private;
                let shuffle = s.shuffle;
                view! {
                    <button type="button" role="switch" class="live-switch" class:on=on aria-checked=on.to_string()
                        aria-label=if on { "Live stream on. Take it off air" } else { "Go live: stream this playlist" }
                        data-tip=if on { "On air: anyone with the link can listen. Press to stop" } else { "Go live: stream this playlist to anyone with the link" }
                        on:click=move |_| change(serde_json::json!({"live": !on}))>
                        <Svg icon=Icon::Live size=20/>"Live"
                    </button>
                    <span class="listeners" data-tip="Listening now">
                        <Svg icon=Icon::Listeners size=18/>
                        <span><strong>{s.on_air.as_ref().map_or(0, |o| o.listeners)}</strong>" listening"</span>
                    </span>
                    <i class="sep" aria-hidden="true"></i>
                    <IconButton icon=Icon::Lock class="chipish" pressed=Signal::derive(move || private)
                        label=if private { "Private: only people with the link can listen. Make public" } else { "Public: anyone can find it. Make private" }.to_string()
                        on_click=move || change(serde_json::json!({"live": on, "visibility": if private { "public" } else { "private" }}))/>
                    <IconButton icon=Icon::Shuffle class="chipish" pressed=Signal::derive(move || shuffle)
                        label=if shuffle { "The stream plays in random order. Press for playlist order" } else { "The stream plays in playlist order. Press to shuffle" }.to_string()
                        on_click=move || change(serde_json::json!({"live": on, "shuffle": !shuffle}))/>
                    {private.then(|| view! {
                        <IconButton icon=Icon::Refresh class="chipish" label="Make a new private link; the old one stops working".to_string() on_click=new_link/>
                    })}
                    <i class="sep" aria-hidden="true"></i>
                    {s.addresses.clone().map(|a| {
                        [("MP3", a.mp3), ("AAC", a.aac), ("HLS", a.hls), ("RTSP", a.rtsp), ("SRT", a.srt), ("RTMP", a.rtmp)]
                            .into_iter()
                            .filter_map(|(k, v)| v.map(|v| (k, v)))
                            .map(|(kind, url)| {
                                // The whole address, as copied: HTTP paths come relative from the API.
                                let url = if url.starts_with('/') { format!("{}{url}", window().location().origin().unwrap_or_default()) } else { url };
                                let tip = url.clone();
                                view! {
                                    <button type="button" class="copy" aria-label=format!("Copy the {kind} link") data-tip=tip
                                        on:click=move |_| copy(url.clone(), kind)>{kind}<Svg icon=Icon::Copy size=16/></button>
                                }
                            })
                            .collect_view()
                    })}
                    <span class="in-use">{format!("{} of {} streams in use", s.live_count, s.max_live)}</span>
                }.into_any()
            }}
        </section>
    }
}

/// The Clipboard API; it is missing on plain-HTTP pages (not a secure context), and may refuse.
async fn clipboard_write(text: &str) -> bool {
    use wasm_bindgen::JsCast;
    let navigator = window().navigator();
    let Ok(clipboard) = js_sys::Reflect::get(&navigator, &"clipboard".into()) else { return false };
    if clipboard.is_undefined() {
        return false;
    }
    let promise = clipboard.unchecked_into::<web_sys::Clipboard>().write_text(text);
    wasm_bindgen_futures::JsFuture::from(promise).await.is_ok()
}

/// The old way, which plain-HTTP pages still allow: select the text in a hidden text box and copy it.
fn copy_by_selection(text: &str) -> bool {
    use wasm_bindgen::JsCast;
    let document = document();
    let Some(area) = document.create_element("textarea").ok().and_then(|e| e.dyn_into::<web_sys::HtmlTextAreaElement>().ok()) else { return false };
    area.set_value(text);
    let _ = area.set_attribute("style", "position:fixed;opacity:0");
    let Some(body) = document.body() else { return false };
    let _ = body.append_child(&area);
    area.select();
    let ok = document.unchecked_ref::<web_sys::HtmlDocument>().exec_command("copy").unwrap_or(false);
    area.remove();
    ok
}
