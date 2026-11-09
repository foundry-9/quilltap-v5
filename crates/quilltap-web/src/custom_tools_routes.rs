//! P4.6ay REST edge: Pascal's custom-tools route (the composer popup's surface).
//! Both edges UNWRAP the dispatch envelope to v4's RAW route body (the SPA
//! client reads the raw body). The `chatCustomToolsList` / `chatCustomToolRun`
//! verbs also ride `POST /api/dispatch` (the SPA's path); this edge gives v4-URL
//! parity.
//!
//! - `GET  /api/v1/chats/{id}/custom-tools`            → `{tools, errors, droppedForCap?}`
//! - `POST /api/v1/chats/{id}/custom-tools?action=run` → `{messages, result}`
//! - P4.D345 (operator intervention, v4 `839575d89`): `GET ?action=interventions`
//!   → `{holds}`; `POST ?action=intervention-preview | intervention-reask |
//!   intervention-resolve` → `{preview}` / `{preview, consults}` / `{resolved}`.
//! - P4.D379 (run presets, v4 `fd165fce0`): `GET ?action=presets&tool=<name>
//!   &asCharacterId=<id>` → `{presets, stores, unread}`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::Value;

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

fn unwrap_to_http(resp: CoreResponse, success_status: StatusCode) -> AxumResponse {
    match resp {
        CoreResponse::CustomToolsList(v)
        | CoreResponse::CustomToolRun(v)
        // === P4.D345 ===
        | CoreResponse::CustomToolInterventions(v)
        | CoreResponse::CustomToolInterventionPreview(v)
        | CoreResponse::CustomToolInterventionReask(v)
        | CoreResponse::CustomToolInterventionResolve(v)
        // === P4.D379 ===
        | CoreResponse::CustomToolPresets(v) => (
            success_status,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        CoreResponse::Error(e) => error_to_http(e),
        _ => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
    }
}

/// `GET /api/v1/chats/{id}/custom-tools` — the popup roster.
///
/// v4 is `withActionDispatch({}, handleList)` (`custom-tools/route.ts:585`): an
/// EMPTY action map with a default, so ANY `?action=` — a bare one included
/// since `ad1c4c37f` — answers v4's `Unknown action: <x>` envelope with an
/// empty `availableActions`, while only an absent action lists. (The §3
/// unification review of P4.67: this edge read no query at all and listed for
/// every action.)
pub async fn custom_tools_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(pairs): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // P4.D345: v4 `withActionDispatch({ interventions: handleListInterventions },
    // handleList)` (`route.ts:517`) — one action beside the default list.
    // === P4.D379 === v4 `fd165fce0` adds `presets` (`route.ts:582-584`), in
    // that literal order — the unknown-action envelope lists `Object.keys`.
    let action = match crate::query::dispatch_action(
        &pairs,
        &["interventions", "presets"],
        "GET",
        "/api/v1/chats/[id]/custom-tools",
    ) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let req = match action {
        Some("interventions") => CoreRequest::ChatCustomToolInterventionsList { chat_id: id },
        // === P4.D379 === v4 `searchParams.get` — the FIRST value of a repeated key.
        Some("presets") => CoreRequest::ChatCustomToolPresets {
            chat_id: id,
            tool: crate::query::first(&pairs, "tool").map(str::to_string),
            as_character_id: crate::query::first(&pairs, "asCharacterId").map(str::to_string),
        },
        // === end P4.D379 ===
        _ => CoreRequest::ChatCustomToolsList { chat_id: id },
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

/// `POST /api/v1/chats/{id}/custom-tools?action=run` — a manual run.
pub async fn custom_tools_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(query): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    // v4: `POST` is `withActionDispatch({ run: handleRun })` with NO default
    // handler, so the middleware answers its own two envelopes — and a bare
    // `?action=` is refused as unknown since `ad1c4c37f`.
    // P4.D345: v4's map at the pin (`route.ts:521-526`), in its literal order.
    const AVAILABLE: &[&str] = &[
        "run",
        "intervention-preview",
        "intervention-reask",
        "intervention-resolve",
    ];
    const PATH: &str = "/api/v1/chats/[id]/custom-tools";
    let action = match crate::query::dispatch_required_action(&query, AVAILABLE, "POST", PATH) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    if action != "run" {
        return intervention_post(&state, id, action, &body).await;
    }
    let json_body: Value = if body.is_empty() {
        Value::Object(Default::default())
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return error_json(StatusCode::BAD_REQUEST, &format!("Invalid body: {e}")),
        }
    };

    // v4's `runSchema.parse` — UNCAUGHT, so any failure is the middleware's flat
    // 400 `{error: 'Validation error'}` (P4.60: this used to read each key with
    // `and_then(Value::as_str)`/`as_bool`/`as_object`, which turned a
    // present-but-wrong-typed value into "the caller didn't say").
    let body = match quilltap_core::api::custom_tools::parse_run_body(&json_body) {
        Ok(b) => b,
        Err(resp) => return unwrap_to_http(resp, StatusCode::OK),
    };

    let req = CoreRequest::ChatCustomToolRun {
        chat_id: id,
        tool: body.tool,
        parameters: body.parameters,
        private: body.private,
        as_character_id: body.as_character_id,
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}

// ---------------------------------------------------------------------------
// P4.6ay unit 12: `/api/v1/custom-tools` — Pascal's Workbench collection
// resource. v4 dispatches on `?action=`; the default GET is the library and the
// default POST is not a served action.
//
// Body parsing lives HERE, mirroring v4's own split: `parseBody` is transport
// (a non-JSON body is a 400 before any Workbench code runs), while the
// definition validation, the metadata union, and the run-refusal arms are core
// logic and live behind the dispatch verbs so the route differential covers
// them.
// ---------------------------------------------------------------------------

fn workbench_unwrap(resp: CoreResponse) -> AxumResponse {
    match resp {
        CoreResponse::CustomToolsLibrary(v)
        | CoreResponse::CustomToolsDestinations(v)
        | CoreResponse::CustomToolPreview(v)
        | CoreResponse::CustomToolAudit(v) => (
            StatusCode::OK,
            [("content-type", "application/json")],
            v.to_string(),
        )
            .into_response(),
        CoreResponse::Error(e) => error_to_http(e),
        _ => error_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected core response",
        ),
    }
}

/// `GET /api/v1/custom-tools` (the library) and `?action=destinations`.
pub async fn workbench_get(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
) -> AxumResponse {
    // v4 `withCollectionActionDispatch({destinations}, handleLibrary)` — the
    // library IS the default handler for an ABSENT action; a bare `?action=`
    // is the `Unknown action` envelope since `ad1c4c37f`.
    let req = match crate::query::dispatch_action(
        &query,
        &["destinations"],
        "GET",
        "/api/v1/custom-tools",
    ) {
        Ok(Some(_destinations)) => CoreRequest::CustomToolsDestinations,
        Ok(None) => CoreRequest::CustomToolsLibrary,
        Err(r) => return *r,
    };
    match dispatch_core(&state, req).await {
        Ok(resp) => workbench_unwrap(resp),
        Err(r) => r,
    }
}

/// `POST /api/v1/custom-tools?action=preview` / `?action=audit`.
pub async fn workbench_post(
    State(state): State<SharedState>,
    Query(query): Query<crate::query::QueryPairs>,
    body: axum::body::Bytes,
) -> AxumResponse {
    const AVAILABLE: &[&str] = &["preview", "audit"];
    const PATH: &str = "/api/v1/custom-tools";
    let action = match crate::query::dispatch_required_action(&query, AVAILABLE, "POST", PATH) {
        Ok(a) => a,
        Err(r) => return *r,
    };

    // v4 `parseBody`: `await req.json()` throwing is the ONLY arm handled here.
    let json_body: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "Request body must be JSON"),
    };
    let Some(obj) = json_body.as_object() else {
        return error_json(StatusCode::BAD_REQUEST, "Invalid request body");
    };

    let definition = obj.get("definition").cloned().unwrap_or(Value::Null);
    let params = obj.get("params").cloned();
    let metadata = obj.get("metadata").cloned();
    // §B: the bench's scripted oracle. The shape check lives in the core (the
    // preview union admits `{live:true}`, the audit union deliberately does not),
    // so the edge only forwards it.
    let llm = obj.get("llm").cloned();
    // §B (P4.d10): the mock merged state — forwarded opaquely, validated in the
    // core (`z.record(...).nullish()`). Named apart from the web `state` handle.
    let mock_state = obj.get("state").cloned();
    // P4.D329: the mock inventory rows — forwarded opaquely, validated in the
    // core (`MockInventorySchema.nullish()`).
    let inventory = obj.get("inventory").cloned();

    let req = if action == "preview" {
        // `private` is `z.boolean().optional()`: a present non-boolean is a body
        // rejection, an absent one is simply `None`.
        let private = match obj.get("private") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => return error_json(StatusCode::BAD_REQUEST, "Invalid request body"),
        };
        CoreRequest::CustomToolPreview {
            definition,
            params,
            private,
            metadata,
            state: mock_state,
            inventory,
            llm,
        }
    } else {
        CoreRequest::CustomToolAudit {
            definition,
            params,
            metadata,
            state: mock_state,
            inventory,
            llm,
        }
    };

    match dispatch_core(&state, req).await {
        Ok(resp) => workbench_unwrap(resp),
        Err(r) => r,
    }
}

// === P4.D345: operator intervention's three POST actions ===

/// The three intervention POSTs. v4's `readBody`: a body that is not JSON is
/// the edge's own 400 `The request body is not JSON` (NOT the run arm's
/// `Invalid body: …`); a parsed body is forwarded WHOLE — its `id` and
/// `overrides` / `decision` raw, absent kept absent — so the core's Zod twin
/// answers v4's sentences on every transport (ruling R-12).
async fn intervention_post(
    state: &SharedState,
    chat_id: String,
    action: &str,
    body: &[u8],
) -> AxumResponse {
    let parsed: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return error_json(StatusCode::BAD_REQUEST, "The request body is not JSON"),
    };
    // A non-object body never yields its keys: it is the core twin's OUTER
    // `expected object` sentence, answered from the core's own parse (the
    // `parse_run_body` precedent) rather than re-worded here.
    if !parsed.is_object() {
        use quilltap_core::pascal::intervention::overrides::{
            join_messages, parse_preview_body, parse_resolve_body,
        };
        let issues = if action == "intervention-resolve" {
            parse_resolve_body(&parsed).err()
        } else {
            parse_preview_body(&parsed).err()
        };
        return error_json(
            StatusCode::BAD_REQUEST,
            &join_messages(&issues.unwrap_or_default()),
        );
    }
    let field = |k: &str| parsed.get(k).cloned();
    let req = match action {
        "intervention-preview" => CoreRequest::ChatCustomToolInterventionPreview {
            chat_id,
            id: field("id"),
            overrides: field("overrides"),
        },
        "intervention-reask" => CoreRequest::ChatCustomToolInterventionReask {
            chat_id,
            id: field("id"),
            overrides: field("overrides"),
        },
        _ => CoreRequest::ChatCustomToolInterventionResolve {
            chat_id,
            id: field("id"),
            decision: field("decision"),
        },
    };
    match dispatch_core(state, req).await {
        Ok(resp) => unwrap_to_http(resp, StatusCode::OK),
        Err(r) => r,
    }
}
