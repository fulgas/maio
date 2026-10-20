//! The header on every signed-in page: home, the engine line, the MediaMTX line, the theme switch and the account menu.

use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_navigate};
use thaw::{ToastIntent, ToasterInjection};

use crate::{
    domain::relay::RelayState,
    ui::{
        api, format,
        icons::{Icon, IconButton},
        menu::Dropdown,
        say,
        theme::Themes,
    },
};
use wasm_bindgen::JsCast;

#[component]
pub fn Frame(children: Children) -> impl IntoView {
    let t = expect_context::<Themes>();
    let toaster = ToasterInjection::expect_context();
    let changes = expect_context::<crate::ui::events::Changes>();
    let status = LocalResource::new(api::status);
    // Engine health changes on its own; look again every 30 s.
    Effect::new(move |_| {
        if let Ok(handle) = set_interval_with_handle(move || status.refetch(), std::time::Duration::from_secs(30)) {
            on_cleanup(move || handle.clear()); // a new Frame on every page must not leave its timer behind
        }
    });
    let navigate = use_navigate();
    let on_select = move |key: String| match key.as_str() {
        "password" => navigate("/password", Default::default()),
        "sign-out" => leptos::task::spawn_local(async {
            let _ = api::sign_out().await;
            let _ = window().location().set_href("/sign-in");
        }),
        _ => {}
    };
    let path = use_location().pathname;
    let engine = move || status.get().and_then(Result::ok).map(|s| format::engine_line(&s.engines));
    let engines = move || status.get().and_then(Result::ok).map(|s| s.engines).unwrap_or_default();
    let relay = move || status.get().and_then(Result::ok).and_then(|s| s.relay);
    // A toast when MediaMTX goes wrong or comes back; nothing on the first look or while it is still being checked.
    let was_ok = StoredValue::new(None::<bool>);
    Effect::new(move |_| {
        let Some(r) = relay().filter(|r| r.state != RelayState::Checking) else { return };
        let (line, ok) = format::relay_line(&r);
        match was_ok.get_value() {
            Some(true) if !ok => say(toaster, format!("{line}."), ToastIntent::Error),
            Some(false) if ok => say(toaster, "MediaMTX is back: every live playlist reaches it.".into(), ToastIntent::Success),
            _ => {}
        }
        was_ok.set_value(Some(ok));
    });
    view! {
        <header class="top">
            <a class="logo" href="/" aria-label="Maio, your songs">"m"<span>"ai"</span>"o"</a>
            <nav class="tabs" aria-label="Sections">
                <a href="/" aria-current=move || current(&path.get(), "/")>"Songs"</a>
                <a href="/playlists" aria-current=move || current(&path.get(), "/playlists")>"Playlists"</a>
            </nav>
            // The browser's popover: opens on click, closes on Escape or a click outside, no script of ours.
            <button type="button" class="engine" class:ready=move || engine().is_some_and(|e| e.1) popovertarget="engines"
                aria-label=move || engine().map(|e| format!("{}: show the engines", e.0))>
                <i aria-hidden="true"></i>{move || engine().map(|e| e.0).unwrap_or_default()}
            </button>
            <section id="engines" class="engines" popover="auto" aria-label="Engines">
                <h2>"Engines"</h2>
                <p class="queue">{move || {
                    use crate::domain::job::JobState;
                    changes.jobs.with(|j| {
                        let count = |s: JobState| j.iter().filter(|x| x.state == s).count();
                        format::queue_line(count(JobState::Making), count(JobState::Waiting))
                    })
                }}</p>
                {move || {
                    let list = engines();
                    if list.is_empty() {
                        return view! { <p>"No engine in maio.toml: add an [[engine]] to make songs."</p> }.into_any();
                    }
                    list.iter().map(|e| {
                        let (name, state, facts) = format::engine_card(e);
                        view! {
                            <div class="engine-card" class:ready=e.health == crate::domain::engine::Health::Up>
                                <h3><i aria-hidden="true"></i>{name}</h3>
                                <p>{state}</p>
                                <ul>{facts.into_iter().map(|f| view! { <li>{f}</li> }).collect_view()}</ul>
                                {e.info.clone().filter(|i| i.models.len() > 1).map(|i| {
                                    let id = e.id.clone();
                                    view! {
                                        <label class="engine-model">"Model"
                                            <select prop:value=i.model.clone() on:change=move |ev| {
                                                let (id, model) = (id.clone(), event_target_value(&ev));
                                                say(toaster, format!("Loading {model}… this can take a few minutes."), ToastIntent::Info);
                                                leptos::task::spawn_local(async move {
                                                    match api::switch_model(&id, &model).await {
                                                        Ok(()) => {
                                                            say(toaster, format!("{model} loaded."), ToastIntent::Success);
                                                            status.refetch();
                                                        }
                                                        Err(f) => say(toaster, f.text(), ToastIntent::Error),
                                                    }
                                                });
                                            }>
                                                {i.models.iter().map(|m| view! { <option value=m.clone()>{m.clone()}</option> }).collect_view()}
                                            </select>
                                        </label>
                                    }
                                })}
                            </div>
                        }
                    }).collect_view().into_any()
                }}
            </section>
            // Rendered once: a status refresh must not rebuild the panel, or an open one would close.
            <Show when=move || relay().is_some()>
                {
                    let line = move || relay().map(|r| format::relay_line(&r)).unwrap_or_default();
                    let card = move || relay().map(|r| (r.state, format::relay_card(&r), r.streams.iter().map(|s| s.arriving).collect::<Vec<_>>()));
                    let on_toggle = move |ev: leptos::ev::Event| {
                        // Looks again when opened: MediaMTX is checked every 5 s, the header only every 30 s.
                        if ev.dyn_ref::<web_sys::ToggleEvent>().is_some_and(|e| e.new_state() == "open") {
                            status.refetch();
                        }
                    };
                    view! {
                        <button type="button" class="engine relay" class:ready=move || line().1 popovertarget="relay"
                            aria-label=move || format!("{}: show MediaMTX", line().0)>
                            <i aria-hidden="true"></i>{move || line().0}
                        </button>
                        <section id="relay" class="engines" popover="auto" aria-label="MediaMTX" on:toggle=on_toggle>
                            <h2>"MediaMTX"</h2>
                            {move || card().map(|(state, (text, facts, streams), arriving)| view! {
                                <div class="engine-card" class:ready=state == RelayState::Up>
                                    <h3><i aria-hidden="true"></i>"MediaMTX"</h3>
                                    <p>{text}</p>
                                    <ul>{facts.into_iter().map(|f| view! { <li>{f}</li> }).collect_view()}</ul>
                                </div>
                                {if streams.is_empty() {
                                    view! { <p class="engine-card">"No playlist is live."</p> }.into_any()
                                } else {
                                    streams.into_iter().zip(arriving).map(|((name, f), arriving)| view! {
                                        <div class="engine-card" class:ready=arriving>
                                            <h3><i aria-hidden="true"></i>{name}</h3>
                                            <ul>{f.into_iter().map(|x| view! { <li>{x}</li> }).collect_view()}</ul>
                                        </div>
                                    }).collect_view().into_any()
                                }}
                            })}
                        </section>
                    }
                }
            </Show>
            <IconButton
                icon=Signal::derive(move || if t.dark.get() { Icon::Sun } else { Icon::Moon })
                label=Signal::derive(move || if t.dark.get() { "Switch to light theme" } else { "Switch to dark theme" }.to_string())
                on_click=move || t.dark.update(|d| *d = !*d)/>
            <Dropdown icon=Icon::More label="Account: change password or sign out" on_select
                items=vec![("password".into(), "Change password".into()), ("sign-out".into(), "Sign out".into())]/>
        </header>
        <main class="page">{children()}</main>
    }
}

/// `aria-current="page"` for the tab of the page you are on; a playlist belongs to Playlists.
fn current(path: &str, tab: &str) -> Option<&'static str> {
    let on = if tab == "/" { path == "/" } else { path.starts_with(tab) };
    on.then_some("page")
}
