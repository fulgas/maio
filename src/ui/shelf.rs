//! The shelf of playlists (arc42 8.5): each playlist's colour, and the order of the tiles.

use std::collections::HashMap;

use crate::ui::{api::PlaylistSummary, store};

pub const COLOURS: [&str; 4] = ["#E8B04A", "#6FA8DC", "#D9706C", "#7FB89A"];
const PLAYED: &str = "maio-played";

/// A playlist's colour, by when it was made, so it stays put as the shelf reorders.
pub fn colour(playlists: &[PlaylistSummary], id: &str) -> &'static str {
    let mut by_age: Vec<&PlaylistSummary> = playlists.iter().collect();
    by_age.sort_by(|a, b| (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id)));
    by_age.iter().position(|p| p.id == id).map_or(COLOURS[0], |i| COLOURS[i % COLOURS.len()])
}

/// Live playlists first, then by most recently played in this browser, then newest.
pub fn order(playlists: &[PlaylistSummary], played: &HashMap<String, f64>) -> Vec<PlaylistSummary> {
    let mut v = playlists.to_vec();
    let at = |p: &PlaylistSummary| played.get(&p.id).copied().unwrap_or(f64::MIN);
    v.sort_by(|a, b| b.live.on.cmp(&a.live.on).then(at(b).total_cmp(&at(a))).then(b.created_at.cmp(&a.created_at)));
    v
}

/// At most this many playlists on the Songs screen's shelf; the rest are a click away. Five fill the widest page.
pub const MAX_SHOWN: usize = 5;
const NEW_W: f64 = 104.0;
const TILE_W: f64 = 200.0;
const GAP: f64 = 12.0;

/// How many of `count` playlists the shelf shows in a row `width` px wide beside "+", and whether it ends with
/// "All playlists" (when some are left out). Keep the sizes in step with `.tile` in style/main.css.
pub fn fits(width: f64, count: usize) -> (usize, bool) {
    let room = |n: usize, all: bool| NEW_W + n as f64 * (GAP + TILE_W) + if all { GAP + NEW_W } else { 0.0 };
    if count <= MAX_SHOWN && room(count, false) <= width {
        return (count, false);
    }
    let mut n = count.min(MAX_SHOWN);
    while n > 0 && room(n, true) > width {
        n -= 1;
    }
    (n, true)
}

/// `items` with the one at `from` moved to `to` (drag to reorder).
pub fn moved<T: Clone>(items: &[T], from: usize, to: usize) -> Vec<T> {
    let mut v = items.to_vec();
    if from < v.len() {
        let item = v.remove(from);
        v.insert(to.min(v.len()), item);
    }
    v
}

/// When this browser last played each playlist (ms since 1970).
pub fn played() -> HashMap<String, f64> {
    store::get(PLAYED).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn mark_played(id: &str) {
    let mut all = played();
    all.insert(id.to_string(), js_sys::Date::now());
    if let Ok(s) = serde_json::to_string(&all) {
        store::set(PLAYED, &s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Live, Visibility};

    fn p(id: &str, created_at: &str, live: bool) -> PlaylistSummary {
        PlaylistSummary {
            id: id.into(),
            name: id.into(),
            songs: 0,
            seconds: 0.0,
            live: Live { on: live, shuffle: false, visibility: Visibility::Public, key: String::new() },
            created_at: created_at.into(),
        }
    }

    #[test]
    fn colours_follow_creation_order_and_repeat() {
        let all = [p("e", "5", false), p("a", "1", false), p("b", "2", false), p("c", "3", false), p("d", "4", false)];
        assert_eq!(colour(&all, "a"), "#E8B04A");
        assert_eq!(colour(&all, "b"), "#6FA8DC");
        assert_eq!(colour(&all, "c"), "#D9706C");
        assert_eq!(colour(&all, "d"), "#7FB89A");
        assert_eq!(colour(&all, "e"), "#E8B04A", "the fifth repeats the first");
    }

    #[test]
    fn shelf_puts_live_first_then_recently_played_then_newest() {
        let all = [p("old", "1", false), p("new", "3", false), p("played", "2", false), p("on-air", "0", true)];
        let played = HashMap::from([("played".to_string(), 100.0), ("old".to_string(), 50.0)]);
        let ids: Vec<_> = order(&all, &played).into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["on-air", "played", "old", "new"]);
    }

    #[test]
    fn moved_moves_one_item() {
        let v = ["a", "b", "c", "d"];
        assert_eq!(moved(&v, 0, 2), ["b", "c", "a", "d"]);
        assert_eq!(moved(&v, 3, 0), ["d", "a", "b", "c"]);
        assert_eq!(moved(&v, 1, 1), v);
        assert_eq!(moved(&v, 1, 9), ["a", "c", "d", "b"], "past the end means the end");
    }

    #[test]
    fn the_shelf_shows_what_fits_and_ends_with_all_playlists_when_some_do_not() {
        assert_eq!(fits(1000.0, 2), (2, false), "everything fits: no All playlists tile");
        assert_eq!(fits(1000.0, 10), (3, true), "+, three playlists, All playlists");
        assert_eq!(fits(300.0, 5), (0, true), "a narrow screen: + and All playlists");
        assert_eq!(fits(1000.0, 0), (0, false));
        assert_eq!(MAX_SHOWN, 5);
        assert_eq!(fits(3000.0, 10), (MAX_SHOWN, true), "at most five, then All playlists");
        assert_eq!(fits(1352.0, 10), (5, true), "five fit beside + and All playlists on the widest page");
        assert_eq!(fits(3000.0, MAX_SHOWN), (MAX_SHOWN, false));
    }
}
