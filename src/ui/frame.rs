//! The header on every signed-in page: home, the engine line, the theme switch and the account menu.

use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_navigate};

use crate::ui::{
    api, format,
    icons::{Icon, IconButton},
    menu::Dropdown,
    theme::Themes,
};

#[component]
pub fn Frame(children: Children) -> impl IntoView {
    let t = expect_context::<Themes>();
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
    view! {
        <header class="top">
            <a class="logo" href="/" aria-label="Maio, your songs">"m"<span>"ai"</span>"o"</a>
            <nav class="tabs" aria-label="Sections">
                <a href="/" aria-current=move || current(&path.get(), "/")>"Songs"</a>
                <a href="/playlists" aria-current=move || current(&path.get(), "/playlists")>"Playlists"</a>
            </nav>
            <span class="engine" class:ready=move || engine().is_some_and(|e| e.1)>
                <i aria-hidden="true"></i>{move || engine().map(|e| e.0).unwrap_or_default()}
            </span>
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
