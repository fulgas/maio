//! Signing in and changing the password (SPEC 4).

use leptos::{ev::SubmitEvent, html::Input, prelude::*};
use thaw::{ToastIntent, ToasterInjection};

use crate::ui::{api, say};

/// True once the wasm runs; the server renders false, so a submit before then does nothing.
fn hydrated() -> RwSignal<bool> {
    let h = RwSignal::new(false);
    Effect::new(move |_| h.set(true));
    h
}

#[component]
pub fn SignIn() -> impl IntoView {
    // Read from the fields at submit, not bound to signals: hydration would reset bound fields to empty,
    // wiping what a fast typist or the browser's autofill put there before the app loaded.
    let (user, pass, remember) = (NodeRef::<Input>::new(), NodeRef::<Input>::new(), NodeRef::<Input>::new());
    let (error, busy, ready) = (RwSignal::new(None::<String>), RwSignal::new(false), hydrated());
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let value = |r: NodeRef<Input>| r.get_untracked().map(|i| i.value()).unwrap_or_default();
        let (user, pass) = (value(user), value(pass));
        let remember = remember.get_untracked().is_some_and(|i| i.checked());
        busy.set(true);
        leptos::task::spawn_local(async move {
            match api::sign_in(&user, &pass, remember).await {
                // A full load: the app starts again signed in, with its event stream.
                Ok(()) => {
                    let _ = window().location().set_href("/");
                }
                Err(f) => {
                    error.set(Some(f.text()));
                    busy.set(false);
                }
            }
        });
    };
    view! {
        <main class="sign-in">
            // method="post": a submit before the app loads never puts the password in the URL.
            <form method="post" on:submit=submit>
                <h1 class="logo">"m"<span>"ai"</span>"o"</h1>
                <label>"Username"<input name="username" autocomplete="username" required node_ref=user/></label>
                <label>"Password"<input name="password" type="password" autocomplete="current-password" required node_ref=pass/></label>
                <label class="check"><input type="checkbox" node_ref=remember/>"Keep me signed in"</label>
                <p class="error" role="alert">{move || error.get()}</p>
                <button class="primary" type="submit" disabled=move || busy.get() || !ready.get()>"Sign in"</button>
            </form>
        </main>
    }
}

#[component]
pub fn Password() -> impl IntoView {
    let toaster = ToasterInjection::expect_context();
    let (current, new, again) = (RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()));
    let (error, busy) = (RwSignal::new(None::<String>), RwSignal::new(false));
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        if new.get_untracked() != again.get_untracked() {
            error.set(Some("The new passwords are not the same.".into()));
            return;
        }
        busy.set(true);
        leptos::task::spawn_local(async move {
            match api::change_password(&current.get_untracked(), &new.get_untracked()).await {
                Ok(()) => {
                    error.set(None);
                    [current, new, again].iter().for_each(|s| s.set(String::new()));
                    say(toaster, "Password changed. Your other sessions are signed out.".into(), ToastIntent::Success);
                }
                Err(f) => error.set(Some(f.text())),
            }
            busy.set(false);
        });
    };
    view! {
        <h1>"Change password"</h1>
        <form class="narrow" method="post" on:submit=submit>
            <label>"Current password"<input type="password" autocomplete="current-password" required bind:value=current/></label>
            <label>"New password"<input type="password" autocomplete="new-password" minlength="10" maxlength="128" required bind:value=new/>
                <span class="hint">"10 to 128 characters."</span></label>
            <label>"New password again"<input type="password" autocomplete="new-password" required bind:value=again/></label>
            <p class="error" role="alert">{move || error.get()}</p>
            <button class="primary" type="submit" disabled=busy>"Change password"</button>
        </form>
    }
}
