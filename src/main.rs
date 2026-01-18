use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::{env, net::SocketAddr, path::PathBuf};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: String,
}

async fn health() -> Json<HealthResponse> {
    let version = env::var("APP_EFFECTIVE_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    Json(HealthResponse {
        status: "ok",
        version,
    })
}

fn build_app(static_dir: PathBuf) -> Router {
    let api = Router::new().route("/health", get(health));

    let index = ServeFile::new(static_dir.join("index.html"));
    let static_files = ServeDir::new(static_dir).fallback(index);

    Router::new()
        .nest("/api", api)
        .fallback_service(static_files)
        .layer(TraceLayer::new_for_http())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let static_dir = env::var("STATIC_DIR").unwrap_or_else(|_| "web/dist".to_string());
    let app = build_app(PathBuf::from(static_dir));

    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:18080".to_string());
    let addr: SocketAddr = bind_addr.parse()?;

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
    axum::serve(listener, app).await?;

    Ok(())
}
