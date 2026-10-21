//! The web app's pages (arc42 8.5): Leptos renders them; strangers are sent to `/sign-in`.

use std::path::PathBuf;

use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
};
use leptos::config::{Env, LeptosOptions, get_configuration};
use leptos_axum::{LeptosRoutes, generate_route_list};
use tower_http::services::ServeDir;

use crate::{
    ui::{App, shell},
    web::{
        api::blocking,
        auth::{SESSION_COOKIE, cookie},
        state::Shared,
    },
};

/// From cargo-leptos's `LEPTOS_*` variables when it built or runs us (and in the container), else the defaults
/// for a plain `cargo` build, such as the tests. Leptos reads an empty name instead of failing, hence the check.
pub fn options() -> LeptosOptions {
    let from_env = std::env::var_os("LEPTOS_OUTPUT_NAME").is_some() || option_env!("LEPTOS_OUTPUT_NAME").is_some();
    let mut opts = match get_configuration(None) {
        Ok(c) if from_env => c.leptos_options,
        _ => LeptosOptions::builder().output_name("maio").site_root("target/site").env(Env::PROD).build(),
    };
    // Built by cargo-leptos with hash-files: the names are hashed even when this binary runs without its variables.
    if option_env!("LEPTOS_HASH_FILES") == Some("true") {
        opts.hash_files = true;
    }
    opts
}

/// The browser tab's icon (DESIGN.md), built into the binary so it needs no site folder; kept for a day.
async fn favicon() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "image/svg+xml"), (axum::http::header::CACHE_CONTROL, "public, max-age=86400")],
        include_str!("../../public/favicon.svg"),
    )
}

pub fn router(app: Shared) -> Router<Shared> {
    let opts = options();
    let hashed = opts.hash_files;
    let site = PathBuf::from(opts.site_root.as_ref());
    let pages = Router::new()
        .leptos_routes(&opts, generate_route_list(App), {
            let opts = opts.clone();
            move || shell(opts.clone())
        })
        .layer(middleware::from_fn_with_state(app, guard))
        .with_state(opts);
    // Hashed names never change content, so they are kept for good; unhashed ones must be checked every time.
    let cache = if hashed { "public, max-age=31536000, immutable" } else { "no-cache" };
    let pkg = Router::new().fallback_service(ServeDir::new(site.join("pkg"))).layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static(cache),
    ));
    Router::new()
        .nest("/pkg", pkg)
        .nest_service("/fonts", ServeDir::new(site.join("fonts")))
        .route("/favicon.svg", axum::routing::get(favicon))
        .merge(pages)
        .fallback(|| async {
            (StatusCode::NOT_FOUND, Html(r#"<!doctype html><title>Not found</title><p>There is nothing here. <a href="/">Go to your songs</a></p>"#))
        })
}

/// Signed out: only `/sign-in`. Signed in: everything but `/sign-in`.
async fn guard(State(app): State<Shared>, req: Request, next: Next) -> Response {
    let signed_in = match cookie(req.headers(), SESSION_COOKIE) {
        Some(s) => blocking(&app, move |a| a.account.is_signed_in(&s)).await.unwrap_or(false),
        None => false,
    };
    match (signed_in, req.uri().path() == "/sign-in") {
        (false, false) => Redirect::to("/sign-in").into_response(),
        (true, true) => Redirect::to("/").into_response(),
        _ => next.run(req).await,
    }
}
