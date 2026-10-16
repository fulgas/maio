use std::time::Duration;

use maio::{api, config, engine, state::AppState, store::Store};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let path = std::env::args().nth(1).unwrap_or_else(|| "maio.toml".into());
    let cfg = config::load(&path)?;
    std::fs::create_dir_all(&cfg.data_dir)?;
    let store = Store::open(&cfg.data_dir.join("maio.db"))?;
    maio::users::ensure_admin(&store, |k| std::env::var(k).ok())?;
    maio::storage::backfill(&store);
    let state = AppState::new(cfg, store)?;
    for e in &state.engines {
        tokio::spawn(engine::health_loop(e.clone()));
    }
    for t in &state.text_engines {
        tokio::spawn(maio::text::health_loop(t.clone()));
    }
    tokio::spawn(retention_loop(state.clone()));
    if state.cfg.relay.enabled {
        tokio::spawn(maio::relay::watch(state.clone()));
    }
    for o in &state.cfg.outputs {
        if state.radio(&o.radio).is_none() {
            tracing::warn!(radio = %o.radio, "an output names a radio that does not exist yet; it starts when that radio is created");
        }
    }
    for r in state.radios() {
        maio::radio::start(&state, &r);
    }
    let listener = tokio::net::TcpListener::bind(&state.cfg.listen).await?;
    tracing::info!("listening on {}", state.cfg.listen);
    axum::serve(listener, api::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>()).await?;
    Ok(())
}

/// Daily retention sweep (SPEC 6): deletes old tracks that are not favourites and not queued or playing.
async fn retention_loop(state: AppState) {
    loop {
        if state.cfg.retention_days > 0 {
            let keep = state.busy_track_ids();
            match state.store.expired(state.cfg.retention_days, &keep) {
                Ok(list) => {
                    for (id, file) in list {
                        let _ = tokio::fs::remove_file(&file).await;
                        if let Err(e) = state.store.mark_deleted(&id) {
                            tracing::warn!("retention: {e}");
                        }
                    }
                }
                Err(e) => tracing::warn!("retention: {e}"),
            }
        }
        tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
    }
}
