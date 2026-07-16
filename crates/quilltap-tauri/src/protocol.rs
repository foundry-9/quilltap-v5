//! §3 — the `qtap` custom protocol, now the ONE origin (P4.7c): the SPA
//! itself is served off `qtap://localhost/` from the embedded
//! `frontendDist` (dogfood finding #12 — with the page on the same origin
//! as the API, every server-supplied RELATIVE URL, including links inside
//! pre-rendered HTML bodies, resolves through this protocol), and the full
//! `http::Request` for the server surface (method, path, query, headers,
//! body) is delegated into the reused `quilltap_web` router over the same
//! booted state; the response buffered back. Tauri custom-protocol
//! responses are buffered, not streamed — which is exactly why
//! `GET /api/events` (SSE) and the terminal WS do NOT ride this protocol
//! (§2/§4 replace them); byte routes buffering a whole file/image is
//! acceptable dev-grade.
//!
//! Split rule: `/api/*` and `/health` belong to the server router; every
//! other GET/HEAD path is the SPA's and is answered from the embedded
//! assets with Tauri's own index fallback (deep-link reloads land on
//! `index.html`, the SPA-router convention `static_serve` applies over
//! HTTP). When no dist is embedded (a plain `cargo build` wraps the
//! build.rs-materialized empty dist) the lookup misses and the request
//! falls through to the router's placeholder pages — same degradation as
//! the HTTP deployment with no `spa_dir`.
//!
//! CORS: same-origin needs none, but the documented `devUrl` dev loop
//! (the SPA on `http://localhost:4200`) still fetches `qtap://…`
//! cross-origin, so the handler keeps answering preflight and stamps
//! `Access-Control-Allow-Origin` permissively on every response.

use axum::Router;
use http::{header, HeaderValue, Method, Request, Response, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

const ALLOW_ORIGIN: HeaderValue = HeaderValue::from_static("*");

/// True when the path belongs to the server router rather than the SPA:
/// the dispatch/REST/byte surface under `/api/` plus the bare `/health`
/// probe. Everything else on GET/HEAD is a page or asset of the SPA.
fn is_server_path(path: &str) -> bool {
    path.starts_with("/api/") || path == "/health"
}

/// Serve one embedded-dist asset (`frontendDist`, compiled into the binary
/// by `tauri::generate_context!`). Tauri's resolver owns the SPA fallback
/// chain (`path` → `path.html` → `path/index.html` → `index.html`) and the
/// MIME sniff; `None` means no dist is embedded (the empty-dist build) and
/// the caller falls through to the router.
fn embedded_asset<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    path: &str,
) -> Option<Response<Vec<u8>>> {
    let asset = app.asset_resolver().get(path.to_string())?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, asset.mime_type)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, ALLOW_ORIGIN)
        .body(asset.bytes)
        .ok()
}

/// Answer one protocol request: SPA paths from the embedded assets, the
/// server surface delegated into the router, the reply buffered. Public so
/// the IPC contract suite can drive it without a webview (the registration
/// closure in `run()` is a one-liner over this).
pub async fn handle_qtap_request<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    router: Router,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    if (request.method() == Method::GET || request.method() == Method::HEAD)
        && !is_server_path(request.uri().path())
    {
        if let Some(asset) = embedded_asset(app, request.uri().path()) {
            return asset;
        }
    }
    if request.method() == Method::OPTIONS {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, ALLOW_ORIGIN)
            .header(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, PUT, PATCH, DELETE, OPTIONS",
            )
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
            .header(header::ACCESS_CONTROL_MAX_AGE, "86400")
            .body(Vec::new())
            .expect("static preflight response");
    }

    // Router is an infallible Service; oneshot cannot error.
    let response = match router.oneshot(request.map(axum::body::Body::from)).await {
        Ok(r) => r,
        Err(infallible) => match infallible {},
    };
    let (mut parts, body) = response.into_parts();
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes().to_vec(),
        Err(e) => {
            // A mid-body failure surfaces as a 500 — the protocol is
            // buffered, so there is no half-sent stream to salvage.
            let mut resp = Response::new(format!("body collection failed: {e}").into_bytes());
            *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            resp.headers_mut()
                .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, ALLOW_ORIGIN);
            return resp;
        }
    };
    parts
        .headers
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, ALLOW_ORIGIN);
    Response::from_parts(parts, bytes)
}
