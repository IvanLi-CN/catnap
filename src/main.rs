use catnap::{build_app, RuntimeConfig};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::{collections::HashSet, net::SocketAddr, str::FromStr, sync::Arc};
use tracing::info;
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

    let catalog = catnap::db::load_catalog_snapshot(&db, &config.upstream_cart_url).await?;
    let catalog = std::sync::Arc::new(tokio::sync::RwLock::new(catalog));
    let ops = catnap::ops::OpsManager::new(config.clone(), db.clone(), catalog.clone());
    ops.start();

    let update_cache = catnap::update_check::new_cache();

    let state = catnap::AppState {
        config: config.clone(),
        db: db.clone(),
        catalog: catalog.clone(),
        catalog_refresh: catnap::catalog_refresh::CatalogRefreshManager::new(),
        ops,
        update_cache,
        lazycat_sync_users: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
    };

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
