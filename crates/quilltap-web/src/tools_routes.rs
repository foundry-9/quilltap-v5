//! P4.9E3B REST edge: the tool inventory — v4 `GET /api/v1/tools`
//! (`app/api/v1/tools/route.ts`). One of the two §1-contract REST edges (the
//! SPA fetches it directly): query params `chatId` and `includeSchemas`
//! (string-compared to `"true"`, exactly as v4 does).

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

/// `GET /api/v1/tools?chatId=…&includeSchemas=true`
pub async fn tools_get(
    state: State<SharedState>,
    query: Query<HashMap<String, String>>,
) -> AxumResponse {
    let chat_id = query.0.get("chatId").cloned();
    // v4: `searchParams.get('includeSchemas') === 'true'`.
    let include_schemas = query.0.get("includeSchemas").map(String::as_str) == Some("true");
    let req = CoreRequest::ToolsList {
        chat_id,
        include_schemas: Some(include_schemas),
    };
    match dispatch_core(&state.0, req).await {
        Ok(CoreResponse::ToolsInventory(v)) => (
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
