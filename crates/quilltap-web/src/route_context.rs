//! The request's `[METHOD pathname]` for v4's context-middleware log lines
//! (P4.162, dogfood #151).
//!
//! v4's `handleRouteError` (`lib/api/middleware/context.ts:159-207`) wraps
//! every handler built by `createContext*Handler` and, when the handler
//! THROWS a store-unavailable error, logs ERROR `` `[${request.method}
//! ${new URL(request.url).pathname}] … unavailable` `` before answering the
//! contextful 503. v5's handlers return the error as a value instead, and the
//! seams that turn it into a 503 (`dispatch_body`, `error_to_http`,
//! `core_error_status_body`, `response_error`, the files edge) never see the
//! request. [`scope`] is the router-wide middleware that lends them the route:
//! it runs the handler inside a task-local holding `"<METHOD> <pathname>"`
//! (no query string, as v4's `pathname`), and [`log_store_unavailable`] reads
//! it.
//!
//! The scope covers the handler's own future — exactly v4's `try` around
//! `handlerFn()`. An error surfacing later, inside a streamed body already
//! handed back (an SSE generator's mid-stream refusal), is outside it in BOTH
//! engines: v4's throw inside a `ReadableStream` never reaches
//! `handleRouteError`, and here the task-local is gone, so no line is logged.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use quilltap_core::api::CoreError;

tokio::task_local! {
    static ROUTE: String;
}

/// The router-wide middleware: run the rest of the stack with this request's
/// `"<METHOD> <pathname>"` in scope.
pub async fn scope(req: Request, next: Next) -> Response {
    let route = format!("{} {}", req.method(), req.uri().path());
    ROUTE.scope(route, next.run(req)).await
}

/// The route in scope, if a request is being handled on this task.
pub fn current() -> Option<String> {
    ROUTE.try_with(String::clone).ok()
}

/// Log v4's context-middleware ERROR for `e` (a no-op unless it carries the
/// store-unavailable entity) with the HTTP route in scope; with none in scope
/// (no request being handled — see the module doc) nothing is logged.
pub fn log_store_unavailable(e: &CoreError) {
    if let Some(route) = current() {
        e.log_store_unavailable(&route);
    }
}
