//! **`/api/v1/scenario-builder`** — v4 `app/api/v1/scenario-builder/route.ts`
//! (`d1c06cd9d`, P4.D217):
//!
//! ```text
//! POST ?action=build          body = v4's request object → text/event-stream
//! GET  ?action=capabilities                               → 200 { webSearchConfigured, curlConfigured }
//! (no / unknown action)                                   → withCollectionActionDispatch's sentences
//! ```
//!
//! **The SPA does not use this edge** — it dispatches `scenarioBuilderBuild`
//! with its own `runId` and reads `scenarioBuilderProgress` frames off
//! `/api/events`. This exists because v4 has it and a REST client may want it
//! (the order's Tier 1 item 7). There is no business logic here: the body is
//! handed RAW to the same verb, so the edge cannot drift from the dispatch
//! decoder (`a-rest-edge-that-shares-the-dispatch-decoder-cannot-drift`).
//!
//! ## The stream, and v4's refusal rule
//!
//! v4 runs every refusal — the JSON parse, the Zod schema, the profile, tools,
//! the api key, the chat — BEFORE `new ReadableStream`, so each is an ordinary
//! JSON error with its status; only what the run itself enqueues is a frame.
//! The re-framer preserves exactly that: the route MINTS the runId, subscribes
//! before dispatching, and a dispatch refused before v4's `accepted` point
//! answers its status + JSON body rather than a 200 stream.
//! The frames are `data: <v4 payload>\n\n` — no `event:`, no `id:`, no
//! keep-alives — with v4's three headers, in v4's order. The pump's two
//! recorded `RecvError` divergences are inherited, not re-argued.
//!
//! ## Client disconnect = abort (v4's `req.signal`)
//!
//! v4 passes `req.signal` into the run, so closing the tab aborts the loop,
//! and its `onAbort` listener — registered as soon as the stream exists, i.e.
//! right after the refusals — logs DEBUG `Scenario Builder client
//! disconnected; aborting the run` on ANY abort of a live run. v5's run
//! executes on the host driver's OWN thread, which dropping the dispatch
//! future does not stop, so the edge carries a [`DisconnectGuard`], armed
//! BEFORE the dispatch is first polled (P4.115 item 3) and moved into the SSE
//! body once the stream commits: a client that leaves before the first frame
//! drops the handler (and with it the guard), one that leaves later drops the
//! body. The guard OWNS the decision — "was a live run cut short?" — and reads
//! it from two in-process facts rather than from the abort verb's answer:
//! the build was ACCEPTED (the engine's acceptance watch, fired at v4's
//! `request accepted` point) and its dispatch has not FINISHED. That closes the
//! race where the pump (or the dropped handler) dropped the dispatch first —
//! unregistering the run, so the abort verb answered `{aborted: false}` and the
//! line was lost. It then dispatches `scenarioBuilderAbort` to trip the token
//! (a no-op on a run that already unregistered — whose own drop tripped it).
//!
//! ## The commit point, and a failed run (v4's belt-and-braces arm)
//!
//! v4 returns its `ReadableStream` as soon as the refusals pass
//! (`route.ts:178-184`): the response head goes out before any frame, and the
//! run executes inside `start()`. P4.115 gives v5 the same point: the engine's
//! in-process acceptance watch fires at v4's `request accepted` DEBUG, and the
//! re-framer ([`crate::generator_sse::stream_frames_committed_on`]) commits the
//! stream right then. A dispatch that resolves BEFORE it is a refusal (JSON,
//! its status); everything after it rides the committed stream.
//!
//! v4's `{"error":"The Host could not complete the enquiry."}` frame, with its
//! ERROR `Scenario Builder stream failed` line, fires when `runScenarioBuilder`
//! itself THROWS — which it never does by contract (`route.ts:156-163`). v5's
//! equivalent is the dispatch resolving an ERROR after acceptance (the
//! driver's thread panicking, or no driver assembled) — before or after the
//! first frame alike, since the stream is already committed: the re-framer
//! hands the result to this route's failure tail ([`failure_tail`]): v4's
//! ERROR line with `error = <message>`, then v4's error frame as the stream's
//! LAST, then the close. A successful run's result is carried by the run's own
//! frames and adds nothing. (Before P4.115 a pre-frame failure answered a JSON
//! 500 — the recorded divergence this retires.)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::scenario_builder::Acceptance;
use quilltap_core::api::{QuilltapCore as _, Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;
use tokio_stream::StreamExt as _;

use crate::characters_routes::core_error_status_body;
use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// The route's path, for the action-dispatch warn lines.
const PATH: &str = "/api/v1/scenario-builder";

/// v4's `badRequest('Request body must be JSON')` (`route.ts:40`).
const NOT_JSON: &str = "Request body must be JSON";

/// v4 `GET = createContextHandler(withCollectionActionDispatch({ capabilities }))`.
pub async fn scenario_builder_get(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    match crate::query::action(&query) {
        Some("capabilities") => {}
        Some(other) => {
            return crate::query::unknown_action_response(other, &["capabilities"], "GET", PATH)
        }
        None => return crate::query::action_required_response(&["capabilities"], "GET", PATH),
    }
    match dispatch_core(&state, CoreRequest::ScenarioBuilderCapabilities).await {
        Ok(CoreResponse::ScenarioBuilder(v)) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        Ok(CoreResponse::Error(e)) => error_to_http(e),
        Ok(_) => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
        Err(r) => r,
    }
}

/// v4 `POST = createContextHandler(withCollectionActionDispatch({ build }))`.
pub async fn scenario_builder_post(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    match crate::query::action(&query) {
        Some("build") => {}
        Some(other) => {
            return crate::query::unknown_action_response(other, &["build"], "POST", PATH)
        }
        None => return crate::query::action_required_response(&["build"], "POST", PATH),
    }

    // v4 `try { raw = await req.json() } catch { return badRequest(...) }` — a
    // missing or unparseable body is this 400, never axum's own 415/422.
    let Ok(raw) = serde_json::from_slice::<Value>(&body) else {
        return error_json(StatusCode::BAD_REQUEST, NOT_JSON);
    };
    let Some(host) = state.host() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "The engine is not running");
    };

    // The route MINTS the scope tag a REST caller never sees.
    let run_id = uuid::Uuid::new_v4().to_string();
    let core = host.core().clone();
    // Watch for v4's "accepted" point BEFORE dispatching (the registration
    // takes the watch); a locked engine has no registry and refuses anyway.
    let watch = core
        .scenario_builder_runs()
        .ok()
        .map(|runs| runs.watch_acceptance(&run_id));
    let finished = Arc::new(AtomicBool::new(false));
    // Armed BEFORE the race (module header): its `Drop` fires on a pre-frame
    // leave too.
    let guard = DisconnectGuard {
        core: Some(core.clone()),
        run_id: run_id.clone(),
        acceptance: watch.as_ref().map(|w| w.acceptance()),
        finished: Arc::clone(&finished),
    };
    let req = CoreRequest::ScenarioBuilderBuild {
        run_id: run_id.clone(),
        body: raw,
    };
    // Handed in UN-POLLED: the re-framer subscribes first
    // (`generator_sse`'s module header, step 1).
    let dispatch = {
        let core = core.clone();
        async move {
            let out = core.dispatch(req).await;
            finished.store(true, Ordering::SeqCst);
            out
        }
    };
    // The commit point: v4's `request accepted` (module header). A locked
    // engine has no registry, and its dispatch refuses before any frame.
    let accepted: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
        match watch.as_ref().map(|w| w.acceptance()) {
            Some(a) => Box::pin(async move { a.accepted().await }),
            None => Box::pin(std::future::pending()),
        };
    let response = crate::generator_sse::stream_frames_committed_on(
        host.core().event_sender(),
        run_id.clone(),
        crate::generator_sse::scenario_builder_frame,
        dispatch,
        |resp: CoreResponse| match resp {
            CoreResponse::ScenarioBuilder(_) => Ok(()),
            // A refusal (the build never reached v4's `accepted` point)
            // answers v4's ordinary JSON error with its status — never a 200
            // stream.
            CoreResponse::Error(e) => Err(core_error_status_body(e)),
            _ => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({ "error": "Unexpected core response" }),
            )),
        },
        Box::new(failure_tail),
        accepted,
    )
    .await;

    drop(watch);
    if response.status() != StatusCode::OK {
        // A refusal: the build was never accepted, so the guard is silent.
        return response;
    }
    // The stream is committed: carry the disconnect guard in its body.
    response.map(|body| {
        Body::from_stream(body.into_data_stream().map(move |chunk| {
            let _keep_alive = &guard;
            chunk
        }))
    })
}

/// v4's committed-stream catch (`route.ts:156-163`, module header): a run that
/// FAILED after the stream began logs ERROR `Scenario Builder stream failed`
/// with `{ error: error.message }` and answers v4's one error frame.
fn failure_tail(resp: CoreResponse) -> Option<Value> {
    let message = match resp {
        CoreResponse::ScenarioBuilder(_) => return None,
        CoreResponse::Error(e) => e.message,
        _ => "Unexpected core response".to_string(),
    };
    tracing::error!(error = %message, "Scenario Builder stream failed");
    Some(serde_json::json!({ "error": HOST_COULD_NOT_COMPLETE }))
}

/// v4's error frame text (`route.ts:161`).
const HOST_COULD_NOT_COMPLETE: &str = "The Host could not complete the enquiry.";

/// Logs v4's disconnect DEBUG and trips the run's abort token when a LIVE run
/// loses its client (module header).
pub struct DisconnectGuard {
    core: Option<quilltap_core::api::CoreEngine>,
    run_id: String,
    /// The build's acceptance (`None` when the engine had no registry).
    acceptance: Option<Arc<Acceptance>>,
    /// Set when the build's dispatch resolved.
    finished: Arc<AtomicBool>,
}

impl Drop for DisconnectGuard {
    fn drop(&mut self) {
        let Some(core) = self.core.take() else {
            return;
        };
        // v4's `onAbort` is registered only once the stream exists (after the
        // refusals) and removed in the run's `finally` — so the line means
        // exactly "accepted, and not yet finished".
        let live = self.acceptance.as_ref().is_some_and(|a| a.is_accepted())
            && !self.finished.load(Ordering::SeqCst);
        if !live {
            return;
        }
        tracing::debug!("Scenario Builder client disconnected; aborting the run");
        let run_id = std::mem::take(&mut self.run_id);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            let _ = core
                .dispatch(CoreRequest::ScenarioBuilderAbort { run_id })
                .await;
        });
    }
}
