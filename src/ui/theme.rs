//! Dark by default, light on request, remembered on this device (SPEC 12).

use leptos::prelude::*;
use thaw::Theme;

use super::store;

pub const KEY: &str = "maio-theme";

/// Runs in <head> before the first paint, so a light-theme browser never flashes dark.
#[cfg(feature = "ssr")]
pub const BOOT: &str = "try{if(localStorage.getItem('maio-theme')==='light')document.documentElement.dataset.theme='light'}catch(e){}";

#[derive(Clone, Copy)]
pub struct Themes {
    pub dark: RwSignal<bool>,
    pub thaw: RwSignal<Theme>,
}

/// Server and browser both start dark, so hydration matches; the browser then applies the saved choice.
pub fn provide() -> Themes {
    let t = Themes { dark: RwSignal::new(true), thaw: RwSignal::new(thaw_theme(true)) };
    Effect::new(move |prev: Option<()>| {
        let dark = t.dark.get();
        if prev.is_none() && dark && store::get(KEY).as_deref() == Some("light") {
            t.dark.set(false); // reruns this effect with the saved choice
            return;
        }
        let name = if dark { "dark" } else { "light" };
        t.thaw.set(thaw_theme(dark));
        if let Some(html) = document().document_element() {
            let _ = html.set_attribute("data-theme", name);
        }
        store::set(KEY, name);
    });
    provide_context(t);
    t
}

/// thaw's dialogs, menus and toasts in the Shelf colours.
fn thaw_theme(dark: bool) -> Theme {
    let mut t = if dark { Theme::dark() } else { Theme::light() };
    let (panel, ink) = if dark { ("#28252C", "#EDEAF0") } else { ("#FFFFFF", "#1E1C21") };
    t.color.set_color_brand_background("#E8B04A".into());
    t.color.set_color_brand_background_hover("#F0C46A".into());
    t.color.set_color_brand_background_pressed("#C9952F".into());
    t.color.set_color_neutral_foreground_on_brand("#1E1C21".into());
    t.color.set_color_neutral_background_1(panel.into());
    t.color.set_color_neutral_foreground_1(ink.into());
    t.common.set_font_family_base("'Albert Sans', 'Helvetica Neue', sans-serif".into());
    t
}
