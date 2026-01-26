use catnap::{build_app, RuntimeConfig};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::{net::SocketAddr, str::FromStr};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = RuntimeConfig::from_env();

    let db_opts = SqliteConnectOptions::from_str(&config.db_url)?.create_if_missing(true);
    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(db_opts)
        .await?;
    catnap::db::init_db(&db).await?;

    let catalog = catnap::upstream::CatalogSnapshot::empty(config.upstream_cart_url.clone());

    let state = catnap::AppState {
        config: config.clone(),
        db: db.clone(),
        catalog: std::sync::Arc::new(tokio::sync::RwLock::new(catalog)),
        catalog_refresh: catnap::catalog_refresh::CatalogRefreshManager::new(),
    };

    tokio::spawn({
        let state = state.clone();
        async move {
            let upstream =
                match catnap::upstream::UpstreamClient::new(state.config.upstream_cart_url.clone())
                {
                    Ok(c) => c,
                    Err(err) => {
                        warn!(error = %err, "failed to create upstream client");
                        return;
                    }
                };
            match upstream.fetch_catalog().await {
                Ok(catalog) => {
                    if let Err(err) =
                        catnap::db::upsert_catalog_configs(&state.db, &catalog.configs).await
                    {
                        warn!(error = %err, "failed to persist upstream catalog");
                        return;
                    }
                    *state.catalog.write().await = catalog;
                    info!("upstream catalog refreshed");
                }
                Err(err) => {
                    warn!(error = %err, "failed to fetch upstream catalog");
                }
            }
        }
    });

    catnap::poller::spawn(state.clone()).await;

    let app = build_app(state);
    let addr: SocketAddr = catnap::app::parse_socket_addr(&config.bind_addr)?;

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!(
                "failed to bind to {addr}: address already in use (set BIND_ADDR to override)"
            )
        } else {
            err.into()
        }
    })?;

    let addr = listener.local_addr()?;
    info!(%addr, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutdown signal received");
}
