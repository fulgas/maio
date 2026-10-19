//! The web app's pages (SPEC 12): Leptos renders them; strangers are sent to `/sign-in`.

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
    match get_configuration(None) {
        Ok(c) if from_env => c.leptos_options,
        _ => LeptosOptions::builder().output_name("maio").site_root("target/site").env(Env::PROD).build(),
    }
}

pub fn router(app: Shared) -> Router<Shared> {
    let opts = options();
    let site = PathBuf::from(opts.site_root.as_ref());
    let pages = Router::new()
        .leptos_routes(&opts, generate_route_list(App), {
            let opts = opts.clone();
            move || shell(opts.clone())
        })
        .layer(middleware::from_fn_with_state(app, guard))
        .with_state(opts);
    Router::new()
        .nest_service("/pkg", ServeDir::new(site.join("pkg")))
        .nest_service("/fonts", ServeDir::new(site.join("fonts")))
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
