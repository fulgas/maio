//! The web app (arc42 8.5): Leptos components rendered on the server and hydrated in the browser.
//! It talks to Maio only through the JSON API (arc42 3.5), like any script.

pub mod api;
mod details;
mod events;
pub mod format;
mod frame;
mod icons;
mod live;
pub mod make;
mod menu;
mod modal;
mod player;
mod playlist;
pub mod queue;
pub mod shelf;
mod sign_in;
mod songs;
mod store;
mod theme;

use leptos::prelude::*;
use leptos_meta::{Title, provide_meta_context};
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
    provide_context(make::Draft(RwSignal::new(None)));
    view! {
        <Title text="Maio"/>
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
                // What toasts say, for screen readers: thaw's toaster is not a live region.
                <div id="announce" class="sr" role="status" aria-live="polite"></div>
            </ToasterProvider>
        </ConfigProvider>
    }
}

/// A short message in the corner (thaw's toaster).
pub(crate) fn say(toaster: thaw::ToasterInjection, text: String, intent: thaw::ToastIntent) {
    use thaw::{Toast, ToastBody, ToastOptions};
    if let Some(region) = document().get_element_by_id("announce") {
        region.set_text_content(Some(&text));
    }
    toaster.dispatch_toast(move || view! { <Toast><ToastBody>{text}</ToastBody></Toast> }, ToastOptions::default().with_intent(intent));
}

/// The whole HTML document around the app (server only).
#[cfg(feature = "ssr")]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    use leptos_meta::{HashedStylesheet, MetaTags};
    view! {
        <thaw::ssr::SSRMountStyleProvider>
            <!DOCTYPE html>
            <html lang="en">
                <head>
                    <meta charset="utf-8"/>
                    <meta name="viewport" content="width=device-width, initial-scale=1"/>
                    <link rel="icon" href="/favicon.svg" type="image/svg+xml"/>
                    <script inner_html=theme::BOOT></script>
                    <AutoReload options=options.clone()/>
                    <HashedStylesheet options=options.clone() id="leptos"/>
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
