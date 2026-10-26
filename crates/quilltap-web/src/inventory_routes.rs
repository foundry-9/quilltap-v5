//! P4.D325 REST edges: the Inventory route factory for all four tiers (v4
//! `lib/inventory/routes/inventory-route-factory.ts` + the eight thin route
//! files, `4fbdad186` at the `322a5488e` pin) and the transfers route.
//!
//! - `GET|POST       /api/v1/inventory`                     → Quilltap General
//! - `GET|PUT|DELETE /api/v1/inventory/{itemId}`            → one General object
//! - `POST           /api/v1/inventory/transfers`           → the transfers verb
//! - `GET|POST       /api/v1/groups/{id}/inventory`         → a group's store
//! - `GET|PUT|DELETE /api/v1/groups/{id}/inventory/{itemId}`
//! - `GET|POST       /api/v1/projects/{id}/inventory`       → a project's store
//! - `GET|PUT|DELETE /api/v1/projects/{id}/inventory/{itemId}`
//!
//! The character tier's five edges live in `characters_routes.rs` and
//! delegate here. Unlike the wardrobe (whose group / project item routes are
//! dispatch-only), every tier's ITEM route is served on REST — the SPA's
//! `inventoryItemUrl` names them (R-M; a deliberate asymmetry). (The order
//! named `groups_routes.rs` / `projects_routes.rs` for the two store tiers;
//! neither file exists in v5, so their edges live here beside the General
//! ones.)
//!
//! Each edge dispatches one `inventory*` verb and renders the result through
//! [`render`] — NEVER `wardrobe_routes::unwrap_to_http`, whose details arm is
//! hard-coded to 400: a `CAP_EXCEEDED` 409 carrying `details` must stay a 409
//! (R-2). A body v4 reads with an unguarded `await req.json()` that does not
//! parse is v4's middleware 500 `Internal server error` (R-4) — except where
//! v4 finds the object FIRST (the item PUT's edit and `clear-override`), where
//! the object's own 404 wins.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::types::{ErrorKind, InventoryContainer, InventoryScope};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};
use serde_json::{json, Value};

use crate::query::{dispatch_action, first, QueryPairs};
use crate::state::SharedState;
use crate::text_replacements_routes::dispatch_core;

/// v4's collection GET action map, in its literal order; no action is the
/// listing.
pub(crate) const COLLECTION_GET_ACTIONS: [&str; 2] = ["folders", "archetypes"];

/// v4's collection POST action map, in its literal order; no action is the
/// create.
pub(crate) const COLLECTION_POST_ACTIONS: [&str; 7] = [
    "instantiate",
    "create-folder",
    "rename-folder",
    "delete-folder",
    "move-items",
    "copy-items",
    "duplicate",
];

/// v4's item GET action map; no action is the object.
pub(crate) const ITEM_GET_ACTIONS: [&str; 1] = ["dependents"];

/// v4's item PUT action map; no action is the edit.
pub(crate) const ITEM_PUT_ACTIONS: [&str; 2] = ["clear-override", "detach"];

fn json_response(status: StatusCode, body: &Value) -> AxumResponse {
    (
        status,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}

/// A dispatch [`CoreResponse`] → v4's HTTP status and body. `success` is the
/// verb's success status (201 for v4's `created(...)` — create, instantiate,
/// duplicate — else 200). An error keeps its kind's status and renders v4's
/// `errorResponse` body: the store-unavailable 503's contextful keys, the
/// `ALREADY_SAVED` 409's flat riders, `{error, details}` when the error
/// carries a bag (the Inventory's `{code, …}` object or a Zod issue array),
/// else `{error}`.
pub fn render(resp: CoreResponse, success: StatusCode) -> (StatusCode, Value) {
    match resp {
        CoreResponse::Inventory(v) => (success, v),
        CoreResponse::Error(e) => {
            crate::route_context::log_store_unavailable(&e);
            let status = match e.kind {
                ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
                ErrorKind::NotFound => StatusCode::NOT_FOUND,
                ErrorKind::Conflict => StatusCode::CONFLICT,
                ErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
                ErrorKind::Forbidden => StatusCode::FORBIDDEN,
                ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
                ErrorKind::Locked | ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
                ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let body = e
                .unavailable_wire_body()
                .or_else(|| e.already_saved_wire_body())
                .or_else(|| e.validation_wire_body())
                .unwrap_or_else(|| json!({ "error": e.message }));
            (status, body)
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": "Unexpected core response" }),
        ),
    }
}

pub(crate) fn rendered(resp: CoreResponse, success: StatusCode) -> AxumResponse {
    let (status, body) = render(resp, success);
    json_response(status, &body)
}

/// v4's middleware 500 for anything a handler throws that is not a ZodError
/// (an unparseable `await req.json()` among them).
pub(crate) fn v4_500() -> AxumResponse {
    json_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        &json!({ "error": "Internal server error" }),
    )
}

/// Parse a body v4 reads with an unguarded `await req.json()`; a failure is
/// v4's middleware 500.
pub(crate) fn parse_json_or_v4_500(body: &str) -> Result<Value, Box<AxumResponse>> {
    serde_json::from_str::<Value>(body).map_err(|_| Box::new(v4_500()))
}

async fn dispatched(state: &SharedState, req: CoreRequest, success: StatusCode) -> AxumResponse {
    match dispatch_core(state, req).await {
        Ok(resp) => rendered(resp, success),
        Err(r) => r,
    }
}

fn opt(pairs: &QueryPairs, key: &str) -> Option<String> {
    first(pairs, key).map(str::to_string)
}

fn is_true(pairs: &QueryPairs, key: &str) -> bool {
    first(pairs, key) == Some("true")
}

// ===========================================================================
// The tier-agnostic helpers (one `container` argument)
// ===========================================================================

/// The collection `GET` — `?action=folders | archetypes`, else the listing.
pub(crate) async fn collection_get(
    state: &SharedState,
    pairs: &QueryPairs,
    route: &str,
    container: InventoryContainer,
) -> AxumResponse {
    let action = match dispatch_action(pairs, &COLLECTION_GET_ACTIONS, "GET", route) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let req = match action {
        Some("folders") => CoreRequest::InventoryFolders { container },
        Some(_archetypes) => CoreRequest::InventoryArchetypes {
            container,
            project_id: opt(pairs, "projectId"),
            chat_id: opt(pairs, "chatId"),
        },
        None => CoreRequest::InventoryList {
            container,
            include_archived: crate::wardrobe_routes::read_include_archived(
                &crate::query::first_map(pairs),
            ),
            shared: is_true(pairs, "shared"),
            project_id: opt(pairs, "projectId"),
            chat_id: opt(pairs, "chatId"),
        },
    };
    dispatched(state, req, StatusCode::OK).await
}

/// The collection `POST` — the seven actions, else the create.
pub(crate) async fn collection_post(
    state: &SharedState,
    pairs: &QueryPairs,
    route: &str,
    container: InventoryContainer,
    body: &str,
) -> AxumResponse {
    let action = match dispatch_action(pairs, &COLLECTION_POST_ACTIONS, "POST", route) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let body = match parse_json_or_v4_500(body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let (req, success) = match action {
        None => (
            CoreRequest::InventoryCreate {
                container,
                project_id: opt(pairs, "projectId"),
                chat_id: opt(pairs, "chatId"),
                body,
            },
            StatusCode::CREATED,
        ),
        Some("instantiate") => (
            CoreRequest::InventoryInstantiate { container, body },
            StatusCode::CREATED,
        ),
        Some("create-folder") => (
            CoreRequest::InventoryFolderCreate { container, body },
            StatusCode::OK,
        ),
        Some("rename-folder") => (
            CoreRequest::InventoryFolderRename { container, body },
            StatusCode::OK,
        ),
        Some("delete-folder") => (
            CoreRequest::InventoryFolderDelete { container, body },
            StatusCode::OK,
        ),
        Some("move-items") => (
            CoreRequest::InventoryItemsMove { container, body },
            StatusCode::OK,
        ),
        Some("copy-items") => (
            CoreRequest::InventoryItemsCopy { container, body },
            StatusCode::OK,
        ),
        Some(_duplicate) => (
            CoreRequest::InventoryDuplicate { container, body },
            StatusCode::CREATED,
        ),
    };
    dispatched(state, req, success).await
}

/// The item `GET` — `?action=dependents`, else the object.
pub(crate) async fn item_get(
    state: &SharedState,
    pairs: &QueryPairs,
    route: &str,
    container: InventoryContainer,
    item_id: String,
) -> AxumResponse {
    let action = match dispatch_action(pairs, &ITEM_GET_ACTIONS, "GET", route) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let req = match action {
        Some(_dependents) => CoreRequest::InventoryDependents { container, item_id },
        None => CoreRequest::InventoryItemGet { container, item_id },
    };
    dispatched(state, req, StatusCode::OK).await
}

/// The item `PUT` — `?action=clear-override | detach`, else the edit. v4 finds
/// the object BEFORE `await req.json()`, so an unparseable body answers the
/// object's 404 (or the owner's) when there is one, else the middleware 500.
pub(crate) async fn item_put(
    state: &SharedState,
    pairs: &QueryPairs,
    route: &str,
    container: InventoryContainer,
    item_id: String,
    body: &str,
) -> AxumResponse {
    let action = match dispatch_action(pairs, &ITEM_PUT_ACTIONS, "PUT", route) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    if action == Some("detach") {
        return dispatched(
            state,
            CoreRequest::InventoryDetach { container, item_id },
            StatusCode::OK,
        )
        .await;
    }
    let body = match serde_json::from_str::<Value>(body) {
        Ok(v) => v,
        Err(_) => {
            // `findRecord` first: its refusal wins over the throw.
            let probe = CoreRequest::InventoryItemGet { container, item_id };
            return match dispatch_core(state, probe).await {
                Ok(resp @ CoreResponse::Error(_)) => rendered(resp, StatusCode::OK),
                Ok(_) => v4_500(),
                Err(r) => r,
            };
        }
    };
    let req = match action {
        Some(_clear_override) => CoreRequest::InventoryClearOverride {
            container,
            item_id,
            body,
        },
        None => CoreRequest::InventoryUpdate {
            container,
            item_id,
            project_id: opt(pairs, "projectId"),
            chat_id: opt(pairs, "chatId"),
            body,
        },
    };
    dispatched(state, req, StatusCode::OK).await
}

/// The item `DELETE` (`?force=true`; no action map in v4).
pub(crate) async fn item_delete(
    state: &SharedState,
    pairs: &QueryPairs,
    container: InventoryContainer,
    item_id: String,
) -> AxumResponse {
    dispatched(
        state,
        CoreRequest::InventoryDelete {
            container,
            item_id,
            force: is_true(pairs, "force"),
        },
        StatusCode::OK,
    )
    .await
}

fn general() -> InventoryContainer {
    InventoryContainer {
        scope: InventoryScope::General,
        id: None,
    }
}

fn tier(scope: InventoryScope, id: String) -> InventoryContainer {
    InventoryContainer {
        scope,
        id: Some(id),
    }
}

// ===========================================================================
// /api/v1/inventory — Quilltap General
// ===========================================================================

const GENERAL_ROUTE: &str = "/api/v1/inventory";
const GENERAL_ITEM_ROUTE: &str = "/api/v1/inventory/[itemId]";

pub async fn inventory_get(
    State(state): State<SharedState>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    collection_get(&state, &pairs, GENERAL_ROUTE, general()).await
}

pub async fn inventory_post(
    State(state): State<SharedState>,
    Query(pairs): Query<QueryPairs>,
    body: String,
) -> AxumResponse {
    collection_post(&state, &pairs, GENERAL_ROUTE, general(), &body).await
}

pub async fn inventory_item_get(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    item_get(&state, &pairs, GENERAL_ITEM_ROUTE, general(), item_id).await
}

pub async fn inventory_item_put(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    Query(pairs): Query<QueryPairs>,
    body: String,
) -> AxumResponse {
    item_put(
        &state,
        &pairs,
        GENERAL_ITEM_ROUTE,
        general(),
        item_id,
        &body,
    )
    .await
}

pub async fn inventory_item_delete(
    State(state): State<SharedState>,
    Path(item_id): Path<String>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    item_delete(&state, &pairs, general(), item_id).await
}

/// `POST /api/v1/inventory/transfers` — v4 parses `await req.json()` OUTSIDE
/// its try (an unparseable body is the middleware 500).
pub async fn inventory_transfers_post(
    State(state): State<SharedState>,
    body: String,
) -> AxumResponse {
    let body = match parse_json_or_v4_500(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    dispatched(
        &state,
        CoreRequest::InventoryTransfer { body },
        StatusCode::OK,
    )
    .await
}

// ===========================================================================
// /api/v1/groups/{id}/inventory and /api/v1/projects/{id}/inventory
// ===========================================================================

const GROUP_ROUTE: &str = "/api/v1/groups/[id]/inventory";
const GROUP_ITEM_ROUTE: &str = "/api/v1/groups/[id]/inventory/[itemId]";
const PROJECT_ROUTE: &str = "/api/v1/projects/[id]/inventory";
const PROJECT_ITEM_ROUTE: &str = "/api/v1/projects/[id]/inventory/[itemId]";

pub async fn groups_inventory_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    collection_get(&state, &pairs, GROUP_ROUTE, tier(InventoryScope::Group, id)).await
}

pub async fn groups_inventory_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(pairs): Query<QueryPairs>,
    body: String,
) -> AxumResponse {
    collection_post(
        &state,
        &pairs,
        GROUP_ROUTE,
        tier(InventoryScope::Group, id),
        &body,
    )
    .await
}

pub async fn groups_inventory_item_get(
    State(state): State<SharedState>,
    Path((id, item_id)): Path<(String, String)>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    item_get(
        &state,
        &pairs,
        GROUP_ITEM_ROUTE,
        tier(InventoryScope::Group, id),
        item_id,
    )
    .await
}

pub async fn groups_inventory_item_put(
    State(state): State<SharedState>,
    Path((id, item_id)): Path<(String, String)>,
    Query(pairs): Query<QueryPairs>,
    body: String,
) -> AxumResponse {
    item_put(
        &state,
        &pairs,
        GROUP_ITEM_ROUTE,
        tier(InventoryScope::Group, id),
        item_id,
        &body,
    )
    .await
}

pub async fn groups_inventory_item_delete(
    State(state): State<SharedState>,
    Path((id, item_id)): Path<(String, String)>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    item_delete(&state, &pairs, tier(InventoryScope::Group, id), item_id).await
}

pub async fn projects_inventory_get(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    collection_get(
        &state,
        &pairs,
        PROJECT_ROUTE,
        tier(InventoryScope::Project, id),
    )
    .await
}

pub async fn projects_inventory_post(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Query(pairs): Query<QueryPairs>,
    body: String,
) -> AxumResponse {
    collection_post(
        &state,
        &pairs,
        PROJECT_ROUTE,
        tier(InventoryScope::Project, id),
        &body,
    )
    .await
}

pub async fn projects_inventory_item_get(
    State(state): State<SharedState>,
    Path((id, item_id)): Path<(String, String)>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    item_get(
        &state,
        &pairs,
        PROJECT_ITEM_ROUTE,
        tier(InventoryScope::Project, id),
        item_id,
    )
    .await
}

pub async fn projects_inventory_item_put(
    State(state): State<SharedState>,
    Path((id, item_id)): Path<(String, String)>,
    Query(pairs): Query<QueryPairs>,
    body: String,
) -> AxumResponse {
    item_put(
        &state,
        &pairs,
        PROJECT_ITEM_ROUTE,
        tier(InventoryScope::Project, id),
        item_id,
        &body,
    )
    .await
}

pub async fn projects_inventory_item_delete(
    State(state): State<SharedState>,
    Path((id, item_id)): Path<(String, String)>,
    Query(pairs): Query<QueryPairs>,
) -> AxumResponse {
    item_delete(&state, &pairs, tier(InventoryScope::Project, id), item_id).await
}

#[cfg(test)]
mod render_tests {
    use super::*;

    fn with_details(kind: ErrorKind, message: &str, details: Option<Value>) -> CoreResponse {
        let mut resp = CoreResponse::error(kind, message);
        if let CoreResponse::Error(e) = &mut resp {
            e.details = details.map(Box::new);
        }
        resp
    }

    fn cap_exceeded() -> CoreResponse {
        with_details(
            ErrorKind::Conflict,
            "\"Excalibur\" can be carried at most 1 at a time",
            Some(
                json!({ "code": "CAP_EXCEEDED", "itemId": "x", "itemName": "Excalibur", "cap": 1 }),
            ),
        )
    }

    /// The reason these edges never call `wardrobe_routes::unwrap_to_http`:
    /// its details arm answers 400 for ANY details-bearing error, so v4's
    /// `CAP_EXCEEDED` 409 would become a 400 there. `render` keeps the 409.
    #[tokio::test]
    async fn a_details_bearing_409_stays_409_here_and_not_in_unwrap_to_http() {
        let wardrobe = crate::wardrobe_routes::unwrap_to_http(cap_exceeded(), StatusCode::OK);
        assert_eq!(wardrobe.status(), StatusCode::BAD_REQUEST);
        let (status, body) = render(cap_exceeded(), StatusCode::OK);
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            body.to_string(),
            r#"{"error":"\"Excalibur\" can be carried at most 1 at a time","details":{"code":"CAP_EXCEEDED","itemId":"x","itemName":"Excalibur","cap":1}}"#
        );
    }

    /// Every `ErrorKind` × with / without `details` → the status and the body
    /// shape (R-2's table; the factory's `Internal` + details keeps its 500 —
    /// only the images edge restores a 502).
    #[test]
    fn every_kind_with_and_without_details() {
        let table = [
            (ErrorKind::BadRequest, StatusCode::BAD_REQUEST),
            (ErrorKind::NotFound, StatusCode::NOT_FOUND),
            (ErrorKind::Conflict, StatusCode::CONFLICT),
            (ErrorKind::Unprocessable, StatusCode::UNPROCESSABLE_ENTITY),
            (ErrorKind::Forbidden, StatusCode::FORBIDDEN),
            (ErrorKind::Unauthorized, StatusCode::UNAUTHORIZED),
            (ErrorKind::Locked, StatusCode::SERVICE_UNAVAILABLE),
            (ErrorKind::Internal, StatusCode::INTERNAL_SERVER_ERROR),
        ];
        for (kind, want) in table {
            let (status, body) = render(with_details(kind, "m", None), StatusCode::OK);
            assert_eq!(status, want, "{kind:?}");
            assert_eq!(body.to_string(), r#"{"error":"m"}"#, "{kind:?}");
            let (status, body) = render(
                with_details(kind, "m", Some(json!({ "code": "C", "refused": false }))),
                StatusCode::OK,
            );
            assert_eq!(status, want, "{kind:?} with details");
            assert_eq!(
                body.to_string(),
                r#"{"error":"m","details":{"code":"C","refused":false}}"#,
                "{kind:?} with details"
            );
        }
    }

    #[test]
    fn a_plain_404_and_the_success_status() {
        let (status, body) = render(
            CoreResponse::error(ErrorKind::NotFound, "Inventory object not found"),
            StatusCode::OK,
        );
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            body.to_string(),
            r#"{"error":"Inventory object not found"}"#
        );
        let (status, body) = render(
            CoreResponse::Inventory(json!({ "item": { "id": "a" } })),
            StatusCode::CREATED,
        );
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body.to_string(), r#"{"item":{"id":"a"}}"#);
    }

    #[test]
    fn the_already_saved_riders_render_flat() {
        let mut resp = CoreResponse::error(ErrorKind::Conflict, "already in this album");
        if let CoreResponse::Error(e) = &mut resp {
            e.code = Some("ALREADY_SAVED".into());
            e.already_saved = Some(Box::new(quilltap_core::api::types::AlreadySavedRiders {
                relative_path: Some("Photos/a.webp".into()),
                kept_at: Some("2026-10-01T00:00:00.000Z".into()),
            }));
        }
        let (status, body) = render(resp, StatusCode::OK);
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            body.to_string(),
            r#"{"error":"already in this album","code":"ALREADY_SAVED","relativePath":"Photos/a.webp","keptAt":"2026-10-01T00:00:00.000Z"}"#
        );
    }

    #[tokio::test]
    async fn an_unparseable_body_is_v4s_500() {
        let resp = match parse_json_or_v4_500("{not json") {
            Err(r) => *r,
            Ok(v) => panic!("parsed {v}"),
        };
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&bytes).unwrap(),
            r#"{"error":"Internal server error"}"#
        );
    }

    /// v4's action maps' literal orders (the `availableActions` arrays).
    #[test]
    fn the_action_lists_are_v4s() {
        assert_eq!(COLLECTION_GET_ACTIONS, ["folders", "archetypes"]);
        assert_eq!(
            COLLECTION_POST_ACTIONS,
            [
                "instantiate",
                "create-folder",
                "rename-folder",
                "delete-folder",
                "move-items",
                "copy-items",
                "duplicate"
            ]
        );
        assert_eq!(ITEM_GET_ACTIONS, ["dependents"]);
        assert_eq!(ITEM_PUT_ACTIONS, ["clear-override", "detach"]);
    }
}
