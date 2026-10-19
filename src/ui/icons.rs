//! The Shelf's icons (SPEC 12: icons over text). Icons are decorative; their buttons carry the label.

use leptos::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Prev,
    Next,
    Shuffle,
    Repeat,
    RepeatOne,
    Add,
    More,
    Cancel,
    Remove,
    Search,
    Sun,
    Moon,
    Make,
    Mood,
    Genre,
    Length,
    Tempo,
    Seed,
    Mic,
    AddTo,
    Volume,
    Muted,
    Grip,
}

/// (filled, inner SVG markup) on a 24×24 grid.
fn shape(icon: Icon) -> (bool, &'static str) {
    use Icon::*;
    match icon {
        Play => (true, r#"<path d="M7 4l13 8-13 8z"/>"#),
        Pause => (true, r#"<path d="M6 5h4v14H6zM14 5h4v14h-4z"/>"#),
        Prev => (true, r#"<path d="M6 5h2v14H6zM20 5v14L9 12z"/>"#),
        Next => (true, r#"<path d="M16 5h2v14h-2zM4 5v14l11-7z"/>"#),
        Shuffle => {
            (false, r#"<path d="M4 7h3c4 0 6 10 10 10h3M4 17h3c1.6 0 2.8-1.6 4-3.5M13 9.5C14.2 8 15.4 7 17 7h3M18 4l3 3-3 3M18 14l3 3-3 3"/>"#)
        }
        Repeat => (false, r#"<path d="M17 2l3 3-3 3M4 11V9a4 4 0 0 1 4-4h12M7 22l-3-3 3-3M20 13v2a4 4 0 0 1-4 4H4"/>"#),
        RepeatOne => (false, r#"<path d="M17 2l3 3-3 3M4 11V9a4 4 0 0 1 4-4h12M7 22l-3-3 3-3M20 13v2a4 4 0 0 1-4 4H4"/><path d="M11 10h1.5v5"/>"#),
        Add => (false, r#"<path d="M12 5v14M5 12h14"/>"#),
        More => (true, r#"<circle cx="5" cy="12" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="19" cy="12" r="1.8"/>"#),
        Cancel => (false, r#"<path d="M6 6l12 12M18 6L6 18"/>"#),
        Remove => (false, r#"<path d="M5 12h14"/>"#),
        Search => (false, r#"<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>"#),
        Sun => (
            false,
            r#"<circle cx="12" cy="12" r="4.5"/><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"/>"#,
        ),
        Moon => (false, r#"<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>"#),
        Make => (false, r#"<path d="M3 12h2M7 8v8M11 5v14M15 9v6M19 7v10"/>"#),
        Mood => (false, r#"<path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM8.5 14.5s1.2 1.5 3.5 1.5 3.5-1.5 3.5-1.5M9 9.5h.01M15 9.5h.01"/>"#),
        Genre => (false, r#"<path d="M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM21 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z"/>"#),
        Length => (false, r#"<path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2"/>"#),
        Tempo => (false, r#"<path d="M8 21h8l-2-15h-4zM12 6l5-3M9.5 14h5"/>"#),
        Seed => (
            false,
            r#"<rect x="3" y="3" width="18" height="18" rx="4"/><circle cx="8.5" cy="8.5" r="1.2" fill="currentColor"/><circle cx="15.5" cy="15.5" r="1.2" fill="currentColor"/><circle cx="15.5" cy="8.5" r="1.2" fill="currentColor"/><circle cx="8.5" cy="15.5" r="1.2" fill="currentColor"/>"#,
        ),
        Mic => (false, r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>"#),
        AddTo => (false, r#"<path d="M3 6h12M3 12h12M3 18h7M18 14v7M14.5 17.5h7"/>"#),
        Volume => (false, r#"<path d="M4 10v4h4l5 4V6L8 10H4z"/><path d="M16.5 9a4 4 0 0 1 0 6"/>"#),
        Muted => (false, r#"<path d="M4 10v4h4l5 4V6L8 10H4z"/><path d="M16 9.5l5 5M21 9.5l-5 5"/>"#),
        Grip => (
            true,
            r#"<circle cx="9" cy="6" r="1.6"/><circle cx="15" cy="6" r="1.6"/><circle cx="9" cy="12" r="1.6"/><circle cx="15" cy="12" r="1.6"/><circle cx="9" cy="18" r="1.6"/><circle cx="15" cy="18" r="1.6"/>"#,
        ),
    }
}

#[component]
pub fn Svg(#[prop(into)] icon: Signal<Icon>, #[prop(default = 20)] size: u32) -> impl IntoView {
    view! {
        <svg width=size height=size viewBox="0 0 24 24" aria-hidden="true"
            fill=move || if shape(icon.get()).0 { "currentColor" } else { "none" }
            stroke=move || if shape(icon.get()).0 { "none" } else { "currentColor" }
            stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
            inner_html=move || shape(icon.get()).1></svg>
    }
}

/// A round icon button: the label is both its accessible name and its caption (shown on hover and keyboard focus).
#[component]
pub fn IconButton(
    #[prop(into)] icon: Signal<Icon>,
    #[prop(into)] label: Signal<String>,
    #[prop(optional, into)] pressed: Option<Signal<bool>>,
    #[prop(optional)] class: &'static str,
    #[prop(default = 20)] size: u32,
    #[prop(optional, into)] disabled: Signal<bool>,
    on_click: impl Fn() + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <button type="button" class=format!("icon {class}") aria-label=label data-tip=label disabled=disabled
            aria-pressed=move || pressed.map(|p| p.get().to_string())
            on:click=move |_| on_click()>
            <Svg icon size/>
        </button>
    }
}
