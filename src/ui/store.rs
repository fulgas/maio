//! This browser's storage, for choices that belong to the device: the theme, and when each playlist was last played.
//! On the server (rendering a page) there is no storage: reads find nothing and writes do nothing.

pub fn get(key: &str) -> Option<String> {
    if cfg!(not(target_arch = "wasm32")) {
        return None;
    }
    web_sys::window()?.local_storage().ok()??.get_item(key).ok()?
}

/// Storage may be off (a private window); the choice is then forgotten on reload.
pub fn set(key: &str, value: &str) {
    if cfg!(not(target_arch = "wasm32")) {
        return;
    }
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(key, value);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_server_has_no_storage() {
        super::set("k", "v");
        assert_eq!(super::get("k"), None);
    }
}
