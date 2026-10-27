//! Signing in and changing the password (arc42 8.1).

use leptos::{ev::SubmitEvent, html::Input, prelude::*};
use thaw::{ToastIntent, ToasterInjection};

use crate::ui::{
    api,
    icons::{Icon, Svg},
    say,
};

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
    let shown = RwSignal::new(false);
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let value = |r: NodeRef<Input>| r.get_untracked().map(|i| i.value()).unwrap_or_default();
        let field = pass;
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
                    // The disabled button dropped focus; put it where the person tries again.
                    if let Some(p) = field.get_untracked() {
                        let _ = p.focus();
                    }
                }
            }
        });
    };
    view! {
        <main class="sign-in">
            // method="post": a submit before the app loads never puts the password in the URL.
            <form method="post" on:submit=submit>
                <h1 class="logo">"m"<span>"ai"</span>"o"</h1>
                <p class="tagline"><b>"M"</b>"usic + "<b>"AI"</b>" + "<b>"O"</b>"utput"</p>
                <label>"Username"<input name="username" autocomplete="username" required node_ref=user/></label>
                <label>"Password"<span class="secret">
                    <input name="password" type=move || kind(shown) autocomplete="current-password" required node_ref=pass/><Reveal shown/>
                </span></label>
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
    let shown = [RwSignal::new(false), RwSignal::new(false), RwSignal::new(false)];
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
                    shown.iter().for_each(|s| s.set(false)); // hidden again once changed
                    say(toaster, "Password changed. Your other sessions are signed out.".into(), ToastIntent::Success);
                }
                // The account's "wrong username or password" names a field this form does not have.
                Err(f) if f.code == "bad_credentials" => error.set(Some("That is not your current password.".into())),
                Err(f) => error.set(Some(f.text())),
            }
            busy.set(false);
        });
    };
    view! {
        <h1>"Change password"</h1>
        <form class="narrow" method="post" on:submit=submit>
            <label>"Current password"<span class="secret">
                <input type=move || kind(shown[0]) autocomplete="current-password" required bind:value=current/><Reveal shown=shown[0]/>
            </span></label>
            <label>"New password"<span class="secret">
                <input type=move || kind(shown[1]) autocomplete="new-password" minlength="10" maxlength="128" required bind:value=new/><Reveal shown=shown[1]/>
            </span>
                <span class="hint">"10 to 128 characters."</span></label>
            <label>"New password again"<span class="secret">
                <input type=move || kind(shown[2]) autocomplete="new-password" required bind:value=again/><Reveal shown=shown[2]/>
            </span></label>
            <p class="error" role="alert">{move || error.get()}</p>
            <button class="primary" type="submit" disabled=busy>"Change password"</button>
        </form>
    }
}

fn kind(shown: RwSignal<bool>) -> &'static str {
    if shown.get() { "text" } else { "password" }
}

/// The eye at the end of a password field: shows what was typed, or hides it again (arc42 8.1). Its name changes
/// with what it does, so it carries no pressed state as well.
#[component]
fn Reveal(shown: RwSignal<bool>) -> impl IntoView {
    let label = move || if shown.get() { "Hide password" } else { "Show password" };
    view! {
        <button type="button" class="icon reveal" aria-label=label data-tip=label
            // A press keeps focus in the field, so typing carries on.
            on:mousedown=|ev| ev.prevent_default()
            on:click=move |_| shown.update(|s| *s = !*s)>
            <Svg icon=Signal::derive(move || if shown.get() { Icon::EyeOff } else { Icon::Eye }) size=20/>
        </button>
    }
}
