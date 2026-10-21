//! The web app's pages (arc42 8.5): rendered on the server, behind sign-in.

mod common;

use axum::http::{StatusCode, header};
use common::{page, sign_in, world};
use tower::ServiceExt;

#[tokio::test]
async fn pages_send_strangers_to_sign_in() {
    let w = world();
    for path in ["/", "/playlists/p1", "/password"] {
        let (status, headers, _) = page(&w, path, None).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{path}");
        assert_eq!(headers[header::LOCATION], "/sign-in", "{path}");
    }
}

#[tokio::test]
async fn an_ended_session_is_sent_to_sign_in() {
    let w = world();
    let (status, headers, _) = page(&w, "/", Some("not-a-session")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/sign-in");
}

#[tokio::test]
async fn sign_in_renders_for_strangers_and_redirects_you_home() {
    let w = world();
    let (status, _, html) = page(&w, "/sign-in", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Sign in"), "{html}");
    assert!(html.contains(r#"<link rel="icon" href="/favicon.svg" type="image/svg+xml">"#), "a favicon: {html}");
    assert!(html.contains("usic + "), "the tagline under the wordmark: {html}");
    assert!(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("public/favicon.svg").exists(), "the favicon ships with the assets");
    let session = sign_in(&w).await;
    let (status, headers, _) = page(&w, "/sign-in", Some(&session)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/");
}

#[tokio::test]
async fn sign_in_form_posts_and_waits_for_the_app() {
    // Before the wasm loads, a submit must not put the password in a URL.
    let w = world();
    let (_, _, html) = page(&w, "/sign-in", None).await;
    assert!(html.contains(r#"method="post""#), "{html}");
    assert!(html.contains("disabled"), "the submit button waits for the app: {html}");
}

#[tokio::test]
async fn signed_in_pages_render_the_shell() {
    let w = world();
    let session = sign_in(&w).await;
    let (status, _, html) = page(&w, "/", Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<title>Maio</title>"), "{html}");
    assert!(html.contains("/pkg/maio.css"), "{html}");
    assert!(html.contains("/pkg/maio.js"), "hydration script: {html}");
    assert!(html.contains("maio-theme"), "the theme boot script runs before paint: {html}");
}

#[tokio::test]
async fn unknown_pages_are_404_and_the_api_keeps_its_json_404() {
    let w = world();
    let session = sign_in(&w).await;
    let (status, _, _) = page(&w, "/no-such-page", Some(&session)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, body) = page(&w, "/api/v1/nope", Some(&session)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains(r#""code""#), "{body}");
}

#[tokio::test]
async fn the_frame_has_the_header_and_the_player() {
    let w = world();
    let session = sign_in(&w).await;
    let (_, _, html) = page(&w, "/", Some(&session)).await;
    assert!(html.contains(r#"aria-label="Switch to light theme""#), "{html}");
    assert!(html.contains(r#"aria-label="Account: change password or sign out""#), "{html}");
    assert!(html.contains(r#"aria-label="Player""#), "{html}");
    assert!(html.contains("<audio"), "{html}");
    assert!(html.contains("Nothing playing"), "{html}");
}

#[tokio::test]
async fn sign_in_has_labelled_fields_and_keep_me_signed_in() {
    let w = world();
    let (_, _, html) = page(&w, "/sign-in", None).await;
    for text in ["Username", "Password", "Keep me signed in", r#"autocomplete="current-password""#] {
        assert!(html.contains(text), "{text}: {html}");
    }
    assert!(!html.contains(r#"aria-label="Player""#) || html.contains("hidden"), "no player on sign-in");
}

#[tokio::test]
async fn change_password_asks_for_current_and_new() {
    let w = world();
    let session = sign_in(&w).await;
    let (_, _, html) = page(&w, "/password", Some(&session)).await;
    for text in ["Current password", "New password", "New password again", r#"autocomplete="new-password""#] {
        assert!(html.contains(text), "{text}: {html}");
    }
}

#[tokio::test]
async fn songs_screen_has_shelf_make_and_search() {
    let w = world();
    let session = sign_in(&w).await;
    let (_, _, html) = page(&w, "/", Some(&session)).await;
    for text in [
        r#"aria-label="Your playlists""#,
        r#"New playlist</span>"#,
        "Make a song",
        "What it sounds like",
        r#"aria-label="Make the song""#,
        "Instrumental",
        r#"aria-label="Mood""#,
        r#"aria-label="Length""#,
        "Search songs",
    ] {
        assert!(html.contains(text), "{text}: {html}");
    }
}

#[tokio::test]
async fn playlist_page_renders_its_frame() {
    let w = world();
    let session = sign_in(&w).await;
    let (status, _, html) = page(&w, "/playlists/p1", Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(r#"aria-label="Songs in play order""#), "{html}");
    assert!(html.contains(r#"aria-label="Live stream""#), "the live strip: {html}");
}

#[tokio::test]
async fn menus_are_keyboard_buttons() {
    // thaw's Menu renders click-only divs; ours is a button that says whether it is open (arc42 8.5: keyboard).
    let w = world();
    let session = sign_in(&w).await;
    let (_, _, html) = page(&w, "/", Some(&session)).await;
    assert!(
        html.contains(r#"aria-label="Account: change password or sign out" data-tip="Account: change password or sign out" aria-expanded="false""#),
        "{html}"
    );
    assert!(!html.contains("thaw-menu"), "{html}");
}

#[tokio::test]
async fn the_header_has_songs_and_playlists_tabs() {
    let w = world();
    let session = sign_in(&w).await;
    let (_, _, html) = page(&w, "/", Some(&session)).await;
    assert!(html.contains(r#"aria-label="Sections""#), "{html}");
    assert!(html.contains(r#"href="/" aria-current="page""#), "Songs is the current tab: {html}");
    assert!(html.contains(r#"href="/playlists""#), "{html}");
}

#[tokio::test]
async fn the_playlists_page_lists_them_behind_sign_in() {
    let w = world();
    let (status, headers, _) = page(&w, "/playlists", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/sign-in");
    let session = sign_in(&w).await;
    let (status, _, html) = page(&w, "/playlists", Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(r#"href="/playlists" aria-current="page""#), "{html}");
    assert!(html.contains(r#"New playlist</button>"#), "{html}");
}

#[tokio::test]
async fn the_player_bar_waits_for_a_song() {
    // With nothing chosen yet the player bar has nothing to show, so it stays out of the way.
    let w = world();
    let session = sign_in(&w).await;
    let (_, _, html) = page(&w, "/", Some(&session)).await;
    assert!(html.contains(r#"class="player hidden""#), "{html}");
}

#[tokio::test]
async fn unhashed_assets_are_checked_every_time() {
    // A plain `cargo` build has no hashed names: a cached file must never hide a new one.
    let w = world();
    let res = w.router.clone().oneshot(axum::http::Request::get("/pkg/maio.css").body(axum::body::Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.headers()[axum::http::header::CACHE_CONTROL], "no-cache");
}

#[tokio::test]
async fn the_favicon_is_served_to_anyone() {
    let w = world();
    let res = w.router.clone().oneshot(axum::http::Request::get("/favicon.svg").body(axum::body::Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "image/svg+xml");
    let body = axum::body::to_bytes(res.into_body(), 1 << 16).await.unwrap();
    assert!(body.starts_with(b"<svg"), "the icon itself");
}
