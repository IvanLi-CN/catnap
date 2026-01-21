use crate::config::RuntimeConfig;
use crate::models::{ErrorResponse, RefreshStatusResponse};
use axum::{
    body::Body,
    extract::{OriginalUri, State},
    http::{header, HeaderMap, HeaderValue, Request, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use bytes::Bytes;
use include_dir::{include_dir, Dir};
use mime_guess::MimeGuess;
use sqlx::SqlitePool;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use time::OffsetDateTime;
use tokio::sync::{Mutex, RwLock};
use tower_http::trace::TraceLayer;

static WEB_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/web/dist");

#[derive(Clone)]
pub struct AppState {
    pub config: RuntimeConfig,
    pub db: SqlitePool,
    pub catalog: Arc<RwLock<crate::upstream::CatalogSnapshot>>,
    pub manual_refresh_gate: Arc<Mutex<HashMap<String, OffsetDateTime>>>,
    pub manual_refresh_status: Arc<Mutex<HashMap<String, RefreshStatusResponse>>>,
}

fn unauthorized_html() -> &'static str {
    r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>401 未授权</title>
    <style>
      :root {
        color-scheme: dark;
        --bg: #0b1220;
        --surface: #0f1b33;
        --surface2: #0c172e;
        --text: #e6eefc;
        --muted: #9bb0d0;
        --line: rgba(27, 42, 74, 0.55);
        --pillWarn: #3b2f18;
        --pillWarnBorder: rgba(138, 106, 42, 0.9);
        font-family: Inter, ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica,
          Arial, sans-serif;
      }

      html,
      body {
        height: 100%;
      }

      body {
        margin: 0;
        background: var(--bg);
        color: var(--text);
        display: grid;
        place-items: center;
      }

      .card {
        position: relative;
        width: 880px;
        height: 420px;
        border-radius: 18px;
        background: linear-gradient(135deg, var(--surface), var(--surface2));
        box-shadow: 0 10px 40px rgba(0, 0, 0, 0.35);
        overflow: hidden;
      }

      .card::before {
        content: "";
        position: absolute;
        inset: 0;
        background: linear-gradient(
          90deg,
          rgba(230, 238, 252, 0.06),
          rgba(230, 238, 252, 0) 55%,
          rgba(230, 238, 252, 0.03)
        );
        clip-path: polygon(0 0, 100% 0, 100% 100%);
        opacity: 0.55;
        pointer-events: none;
      }

      .watermark {
        position: absolute;
        right: 64px;
        top: 130px;
        font-size: 180px;
        font-weight: 900;
        letter-spacing: -6px;
        color: rgba(230, 238, 252, 0.06);
        user-select: none;
        pointer-events: none;
      }

      .content {
        position: relative;
        padding: 42px 60px;
      }

      .iconBox {
        width: 56px;
        height: 56px;
        border-radius: 14px;
        display: grid;
        place-items: center;
        background: rgba(16, 31, 58, 0.75);
        border: 1px solid rgba(27, 42, 74, 0.6);
      }

      h1 {
        margin: 0;
        font-size: 34px;
        font-weight: 900;
        letter-spacing: -0.2px;
      }

      p {
        margin: 10px 0 0;
        line-height: 1.6;
      }

      .text {
        font-size: 16px;
      }

      .muted {
        font-size: 14px;
        color: var(--muted);
      }

      .row {
        margin-top: 20px;
        display: flex;
        align-items: center;
        gap: 20px;
      }

      .pill {
        height: 32px;
        border-radius: 10px;
        background: var(--pillWarn);
        border: 1px solid var(--pillWarnBorder);
        color: var(--text);
        font-size: 12px;
        font-weight: 700;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 160px;
        text-decoration: none;
        cursor: pointer;
      }

      @media (max-width: 980px) {
        body {
          padding: 18px;
        }

        .card {
          width: 100%;
          height: auto;
        }

        .content {
          padding: 28px 24px;
        }

        .watermark {
          display: none;
        }
      }
    </style>
  </head>
  <body>
    <div class="card">
      <div class="watermark">401</div>
      <div class="content">
        <div style="display: flex; align-items: center; gap: 24px;">
          <div class="iconBox" aria-hidden="true">
            <svg width="32" height="32" viewBox="0 0 24 24" aria-hidden="true">
              <path
                fill="rgba(230, 238, 252, 0.9)"
                d="M12 17a2 2 0 0 1-2-2c0-1.11.89-2 2-2a2 2 0 0 1 2 2a2 2 0 0 1-2 2m6 3V10H6v10zm0-12a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V10c0-1.11.89-2 2-2h1V6a5 5 0 0 1 5-5a5 5 0 0 1 5 5v2zm-6-5a3 3 0 0 0-3 3v2h6V6a3 3 0 0 0-3-3"
              />
            </svg>
          </div>
          <div>
            <h1>401 未授权</h1>
            <p class="text">当前请求未获得访问权限。</p>
            <p class="muted">请通过受信任入口访问，或联系管理员处理。</p>
            <p class="muted">你可以先返回首页继续浏览其它内容。</p>
            <div class="row">
              <a class="pill" href="/">返回首页</a>
              <span class="muted">或刷新页面重试</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </body>
</html>"#
}

pub fn build_app(state: AppState) -> Router {
    let api = crate::app_api::router(state.clone());

    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", api)
        .fallback(serve_embedded_ui)
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn_with_state(
            state,
            require_user_global,
        ))
}

async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

fn content_type_for_path(path: &str) -> HeaderValue {
    if path.ends_with(".html") {
        return HeaderValue::from_static("text/html; charset=utf-8");
    }

    let mime = MimeGuess::from_path(path).first_or_octet_stream();
    HeaderValue::from_str(mime.essence_str())
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"))
}

async fn serve_embedded_ui(OriginalUri(uri): OriginalUri) -> Response<Body> {
    let path = uri.path();
    let path = path.trim_start_matches('/');
    let requested = if path.is_empty() { "index.html" } else { path };

    let file = WEB_DIST
        .get_file(requested)
        .or_else(|| WEB_DIST.get_file("index.html"));

    let Some(file) = file else {
        let mut res = Response::new(Body::from("missing embedded UI (run `bun run build`)"));
        *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        return res;
    };

    let served_path = file.path().to_string_lossy();
    let mut res = Response::new(Body::from(Bytes::from_static(file.contents())));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        content_type_for_path(served_path.as_ref()),
    );
    if served_path.as_ref() == "index.html" {
        res.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    } else if served_path.as_ref().starts_with("assets/") {
        res.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    res
}

async fn require_user_global(
    State(state): State<AppState>,
    req: Request<Body>,
    next: axum::middleware::Next,
) -> Response<Body> {
    if req.uri().path() == "/healthz" {
        return next.run(req).await;
    }

    let Some(user_id) = user_id_from_headers(&state, req.headers()) else {
        if req.uri().path().starts_with("/api/") {
            return json_unauthorized().into_response();
        }

        let mut res = Response::new(Body::from(unauthorized_html()));
        *res.status_mut() = StatusCode::UNAUTHORIZED;
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        return res;
    };

    let mut req = req;
    req.extensions_mut().insert(crate::models::UserView {
        id: user_id,
        display_name: None,
    });
    next.run(req).await
}

pub fn user_id_from_headers(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let header_name = state.config.auth_user_header.as_deref()?;
    let header_name = header::HeaderName::from_bytes(header_name.as_bytes()).ok()?;
    let value = headers.get(header_name)?;
    let value = value.to_str().ok()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

pub fn json_unauthorized() -> (StatusCode, axum::Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(ErrorResponse {
            error: crate::models::ErrorInfo {
                code: "UNAUTHORIZED",
                message: "Unauthorized",
            },
        }),
    )
}

pub fn json_forbidden() -> (StatusCode, axum::Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        axum::Json(ErrorResponse {
            error: crate::models::ErrorInfo {
                code: "FORBIDDEN",
                message: "Forbidden",
            },
        }),
    )
}

pub fn json_invalid_argument() -> (StatusCode, axum::Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(ErrorResponse {
            error: crate::models::ErrorInfo {
                code: "INVALID_ARGUMENT",
                message: "Invalid argument",
            },
        }),
    )
}

pub fn parse_socket_addr(bind_addr: &str) -> anyhow::Result<SocketAddr> {
    Ok(bind_addr.parse()?)
}
