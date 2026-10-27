//! The Shelf's icons (arc42 8.5: icons over text). Icons are decorative; their buttons carry the label.

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
    Mic,
    AddTo,
    Volume,
    Muted,
    Live,
    Lock,
    Listeners,
    Refresh,
    Copy,
    Grip,
    Engine,
    Relay,
    Settings,
    Energy,
    Seed,
    Dice,
    Voice,
    Language,
    Pen,
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
        Engine => (false, r#"<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4"/>"#),
        Relay => (
            false,
            r#"<circle cx="12" cy="12" r="2"/><path d="M8.5 8.5a5 5 0 0 0 0 7M15.5 8.5a5 5 0 0 1 0 7M5.6 5.6a9 9 0 0 0 0 12.8M18.4 5.6a9 9 0 0 1 0 12.8"/>"#,
        ),
        Moon => (false, r#"<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>"#),
        Make => (false, r#"<path d="M3 12h2M7 8v8M11 5v14M15 9v6M19 7v10"/>"#),
        Mood => (false, r#"<path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM8.5 14.5s1.2 1.5 3.5 1.5 3.5-1.5 3.5-1.5M9 9.5h.01M15 9.5h.01"/>"#),
        Genre => (false, r#"<path d="M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM21 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z"/>"#),
        Length => (false, r#"<path d="M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2"/>"#),
        Tempo => (false, r#"<path d="M8 21h8l-2-15h-4zM12 6l5-3M9.5 14h5"/>"#),
        Mic => (false, r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>"#),
        AddTo => (false, r#"<path d="M3 6h12M3 12h12M3 18h7M18 14v7M14.5 17.5h7"/>"#),
        Volume => (false, r#"<path d="M4 10v4h4l5 4V6L8 10H4z"/><path d="M16.5 9a4 4 0 0 1 0 6"/>"#),
        Muted => (false, r#"<path d="M4 10v4h4l5 4V6L8 10H4z"/><path d="M16 9.5l5 5M21 9.5l-5 5"/>"#),
        Live => (
            false,
            r#"<circle cx="12" cy="12" r="2.2" fill="currentColor"/><path d="M7.8 7.8a6 6 0 0 0 0 8.4M16.2 7.8a6 6 0 0 1 0 8.4M4.9 4.9a10 10 0 0 0 0 14.2M19.1 4.9a10 10 0 0 1 0 14.2"/>"#,
        ),
        Lock => (false, r#"<path d="M7 11V8a5 5 0 0 1 10 0v3M5 11h14v10H5z"/>"#),
        Listeners => (false, r#"<path d="M3 18v-6a9 9 0 0 1 18 0v6"/><path d="M21 19a2 2 0 0 1-2 2h-1v-6h3zM3 19a2 2 0 0 0 2 2h1v-6H3z"/>"#),
        Refresh => (false, r#"<path d="M20 11a8 8 0 0 0-14.3-4.9L4 8M4 4v4h4M4 13a8 8 0 0 0 14.3 4.9L20 16M20 20v-4h-4"/>"#),
        Copy => (false, r#"<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>"#),
        Settings => (
            false,
            r#"<path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1"/><circle cx="15" cy="6" r="2"/><circle cx="9" cy="12" r="2"/><circle cx="17" cy="18" r="2"/>"#,
        ),
        Energy => (false, r#"<path d="M13 3L5 14h6l-1 7 8-11h-6z"/>"#),
        Seed => (false, r#"<path d="M12 21c-5-2-8-6-8-11 5 0 8 3 8 8M12 21c5-2 8-6 8-11-5 0-8 3-8 8M12 21v-8"/>"#),
        Dice => (
            false,
            r#"<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M8.5 8.5h.01M15.5 8.5h.01M12 12h.01M8.5 15.5h.01M15.5 15.5h.01" stroke-width="3"/>"#,
        ),
        Voice => (false, r#"<circle cx="12" cy="8" r="4"/><path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6"/>"#),
        Language => (false, r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3.5 3 14.5 0 18M12 3c-3 3.5-3 14.5 0 18"/>"#),
        Pen => (false, r#"<path d="M4 20h4L19 9l-4-4L4 16v4z"/>"#),
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
