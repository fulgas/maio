//! Dialogs (arc42 8.5): the browser's own `<dialog>`, opened with `showModal()`. The browser moves focus in,
//! keeps it there while the dialog is open, closes it on Escape and puts focus back where it was.

use leptos::{html, prelude::*};

#[component]
pub fn Modal(
    open: RwSignal<bool>,
    #[prop(into)] title: Signal<String>,
    /// More classes for the dialog, such as `wide`.
    #[prop(optional)]
    class: &'static str,
    children: Children,
) -> impl IntoView {
    let dialog = NodeRef::<html::Dialog>::new();
    Effect::new(move |_| {
        let (Some(d), open) = (dialog.get(), open.get()) else { return };
        if open && !d.open() {
            let _ = d.show_modal();
        } else if !open && d.open() {
            d.close();
        }
    });
    view! {
        // Escape closes it in the browser; `close` tells the signal.
        <dialog node_ref=dialog class=format!("modal {class}") aria-label=move || title.get() on:close=move |_| open.set(false)>
            <h2 class="modal-title">{move || title.get()}</h2>
            {children()}
        </dialog>
    }
}
