//! The `maio` binary: reads the config, starts logging and telemetry, serves the API on port 8080,
//! and stops cleanly on Ctrl-C or SIGTERM.

use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use maio::web::config::Config;

fn main() -> anyhow::Result<()> {
    let path = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "maio.toml".into()));
    let cfg = maio::web::config::load(&path, env)?;
    let telemetry = maio::web::telemetry::init(cfg.log_format)?;
    let served = tokio::runtime::Runtime::new().context("starting the async runtime")?.block_on(serve(cfg));
    if let Err(e) = &served {
        tracing::error!("Maio stopped: {e:#}");
    }
    telemetry.shutdown();
    served
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

async fn serve(cfg: Config) -> anyhow::Result<()> {
    let (user, password) = (env("MAIO_ADMIN_USER"), env("MAIO_ADMIN_PASSWORD"));
    let app = maio::web::start(&cfg, (user.as_deref(), password.as_deref()))?;
    let listener = tokio::net::TcpListener::bind(&cfg.listen).await.with_context(|| format!("listening on {}", cfg.listen))?;
    tracing::info!(listen = %cfg.listen, "Maio is up");
    let mut stopping = app.stopping.subscribe();
    let signal = {
        let app = app.clone();
        async move {
            stopped().await;
            tracing::info!("stopping");
            app.stop();
        }
    };
    let server = axum::serve(listener, maio::web::router(app).into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(signal);
    // Docker kills after 10 s; stop waiting for slow requests (an audio download) before that, so telemetry still flushes.
    let deadline = async move {
        let _ = stopping.wait_for(|s| *s).await;
        tokio::time::sleep(std::time::Duration::from_secs(7)).await;
    };
    tokio::select! {
        served = server => served?,
        () = deadline => tracing::warn!("open requests did not finish in time; stopping anyway"),
    }
    tracing::info!("Maio stopped");
    Ok(())
}

/// Ctrl-C, or SIGTERM from `docker compose stop`.
async fn stopped() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = term => {}
    }
}
