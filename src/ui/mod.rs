//! The web app (SPEC 12): Leptos components rendered on the server and hydrated in the browser.
//! It talks to Maio only through the JSON API (SPEC 10), like any script.

pub mod api;
mod events;
pub mod format;
mod frame;
mod icons;
pub mod make;
mod menu;
mod player;
mod playlist;
pub mod queue;
pub mod shelf;
mod sign_in;
mod songs;
mod store;
mod theme;

use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};
use thaw::{ConfigProvider, ToasterProvider};

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let themes = theme::provide();
    events::provide();
    player::provide();
    view! {
        <Title text="Maio"/>
        <Stylesheet id="main" href="/pkg/maio.css"/>
        <ConfigProvider theme=themes.thaw>
            <ToasterProvider>
                <Router>
                    <Routes fallback=|| view! { <p class="empty">"There is nothing here. " <a href="/">"Go to your songs"</a></p> }>
                        <Route path=path!("/sign-in") view=sign_in::SignIn/>
                        <Route path=path!("/") view=|| view! { <frame::Frame><songs::SongsPage/></frame::Frame> }/>
                        <Route path=path!("/playlists") view=|| view! { <frame::Frame><songs::PlaylistsPage/></frame::Frame> }/>
                        <Route path=path!("/playlists/:id") view=|| view! { <frame::Frame><playlist::PlaylistPage/></frame::Frame> }/>
                        <Route path=path!("/password") view=|| view! { <frame::Frame><sign_in::Password/></frame::Frame> }/>
                    </Routes>
                    <player::PlayerBar/>
                </Router>
            </ToasterProvider>
        </ConfigProvider>
    }
}

/// A short message in the corner (thaw's toaster).
pub(crate) fn say(toaster: thaw::ToasterInjection, text: String, intent: thaw::ToastIntent) {
    use thaw::{Toast, ToastBody, ToastOptions};
    toaster.dispatch_toast(move || view! { <Toast><ToastBody>{text}</ToastBody></Toast> }, ToastOptions::default().with_intent(intent));
}

/// The whole HTML document around the app (server only).
#[cfg(feature = "ssr")]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    use leptos_meta::MetaTags;
    view! {
        <thaw::ssr::SSRMountStyleProvider>
            <!DOCTYPE html>
            <html lang="en">
                <head>
                    <meta charset="utf-8"/>
                    <meta name="viewport" content="width=device-width, initial-scale=1"/>
                    <script inner_html=theme::BOOT></script>
                    <AutoReload options=options.clone()/>
                    <HydrationScripts options/>
                    <MetaTags/>
                </head>
                <body>
                    <App/>
                </body>
            </html>
        </thaw::ssr::SSRMountStyleProvider>
    }
}
