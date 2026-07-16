//! §3 — the `qtap` custom protocol: the full `http::Request` (method,
//! path, query, headers, body) delegated into the reused `quilltap_web`
//! router over the same booted state; the response buffered back. Tauri
//! custom-protocol responses are buffered, not streamed — which is exactly
//! why `GET /api/events` (SSE) and the terminal WS do NOT ride this
//! protocol (§2/§4 replace them); byte routes buffering a whole file/image
//! is acceptable dev-grade.
//!
//! CORS: the webview origin (`tauri://localhost` on macOS,
//! `http://tauri.localhost` on Windows) fetches `qtap://…` cross-origin, so
//! the handler answers preflight and stamps `Access-Control-Allow-Origin`
//! permissively on every response — otherwise the SPA's raw fetches fail
//! silently.

use axum::Router;
use http::{header, HeaderValue, Method, Request, Response, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

const ALLOW_ORIGIN: HeaderValue = HeaderValue::from_static("*");

/// Delegate one protocol request into the router and buffer the reply.
/// Public so the IPC contract suite can drive it without a webview (the
/// registration closure in `run()` is a one-liner over this).
pub async fn handle_qtap_request(router: Router, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
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
