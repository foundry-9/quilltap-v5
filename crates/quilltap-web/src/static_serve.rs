//! Static SPA serving (D5): a configurable dist directory with the
//! index-fallback SPA convention, plus the `/setup` page that must render
//! while the vault is locked. The Angular app lands in P4.5; until then the
//! embedded placeholder pages keep `/` and `/setup` readable.

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response as AxumResponse};

use crate::state::SharedState;

/// The pre-SPA landing page (P4.5 replaces it with the Angular dist).
const PLACEHOLDER_INDEX: &str = include_str!("../static/index.html");
/// The pre-P4.5 unlock placeholder — must be readable while locked.
const PLACEHOLDER_SETUP: &str = include_str!("../static/setup.html");

fn mime_for_asset(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or_default();
    match ext {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn html(status: StatusCode, body: &'static str) -> AxumResponse {
    (status, [("content-type", "text/html; charset=utf-8")], body).into_response()
}

/// `GET /setup` — the unlock/setup surface named by the readiness 503 body.
pub async fn setup(State(state): State<SharedState>) -> AxumResponse {
    // A real dist may ship its own setup route; the SPA fallback covers it.
    if state.spa_dir.is_some() {
        return spa_fallback(State(state), Uri::from_static("/setup")).await;
    }
    html(StatusCode::OK, PLACEHOLDER_SETUP)
}

/// The SPA fallback: serve the asset when it exists under the dist dir
/// (traversal-guarded), else `index.html` (the SPA router owns the path).
pub async fn spa_fallback(State(state): State<SharedState>, uri: Uri) -> AxumResponse {
    let path = uri.path().trim_start_matches('/');
    if let Some(dir) = &state.spa_dir {
        // Traversal guard: reject any dot-dot segment outright.
        if !path.is_empty() && !path.split('/').any(|seg| seg == "..") {
            let candidate = dir.join(path);
            if candidate.is_file() {
                if let Ok(bytes) = std::fs::read(&candidate) {
                    return (
                        StatusCode::OK,
                        [("content-type", mime_for_asset(path))],
                        bytes,
                    )
                        .into_response();
                }
            }
        }
        let index = dir.join("index.html");
        if let Ok(bytes) = std::fs::read(&index) {
            return (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                bytes,
            )
                .into_response();
        }
    }
    html(StatusCode::OK, PLACEHOLDER_INDEX)
}
