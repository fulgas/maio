//! A small dropdown of buttons: reachable by keyboard, closed by a choice, Escape, or focus leaving it.
//! thaw's Menu renders click-only divs that a keyboard cannot reach (SPEC 12), so this replaces it.

use std::sync::Arc;

use leptos::{
    html::{Button, Div},
    prelude::*,
};
use wasm_bindgen::JsCast;

use crate::ui::icons::{Icon, Svg};

#[component]
pub fn Dropdown(
    icon: Icon,
    #[prop(into)] label: String,
    /// (value, text) for each choice.
    items: Vec<(String, String)>,
    on_select: impl Fn(String) + Send + Sync + 'static,
    /// Extra classes on the trigger button.
    #[prop(optional)]
    class: &'static str,
    #[prop(default = 20)] size: u32,
    /// Shown when there are no choices.
    #[prop(optional)]
    empty: &'static str,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let root = NodeRef::<Div>::new();
    let trigger = NodeRef::<Button>::new();
    // Safari does not focus a clicked button, so focus never leaves: close on any press outside too.
    let outside = window_event_listener(leptos::ev::pointerdown, move |ev| {
        let at = ev.target().and_then(|t| t.dyn_into::<web_sys::Node>().ok());
        if open.get_untracked() && !root.get_untracked().is_some_and(|r| r.contains(at.as_ref())) {
            open.set(false);
        }
    });
    on_cleanup(move || outside.remove());
    let on_select = Arc::new(on_select);
    let items = StoredValue::new(items);
    let leaves = move |ev: leptos::ev::FocusEvent| {
        let to = ev.related_target().and_then(|t| t.dyn_into::<web_sys::Node>().ok());
        let inside = root.get_untracked().is_some_and(|r| r.contains(to.as_ref()));
        if !inside {
            open.set(false);
        }
    };
    view! {
        <div class="dropdown" node_ref=root on:focusout=leaves
            on:keydown=move |ev| {
                if ev.key() == "Escape" && open.get_untracked() {
                    open.set(false);
                    if let Some(b) = trigger.get_untracked() {
                        let _ = b.focus(); // back where the person was
                    }
                }
            }>
            <button type="button" node_ref=trigger class=format!("icon {class}") aria-label=label.clone() data-tip=label
                aria-expanded=move || open.get().to_string() on:click=move |_| open.update(|o| *o = !*o)>
                <Svg icon size/>
            </button>
            <Show when=move || open.get()>
                <div class="dropdown-list">
                    {
                        let on_select = on_select.clone();
                        move || {
                            let list = items.get_value();
                            if list.is_empty() {
                                return view! { <p class="dropdown-empty">{empty}</p> }.into_any();
                            }
                            list.into_iter().map(|(value, text)| {
                                let pick = on_select.clone();
                                view! {
                                    <button type="button" on:click=move |_| {
                                        open.set(false);
                                        pick(value.clone());
                                    }>{text}</button>
                                }
                            }).collect_view().into_any()
                        }
                    }
                </div>
            </Show>
        </div>
    }
}
