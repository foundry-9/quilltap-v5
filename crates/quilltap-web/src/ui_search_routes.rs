//! P4.9P REST edge: the global-search endpoint.
//!
//! - `GET /api/v1/ui/search?q=…&types=…&limit=…&offset=…` → v4's RAW
//!   `SearchResponse` body (`{results, totalCount, query, types, hasMore,
//!   countsByType}`).
//!
//! All four params ride to the core as RAW strings — the dispatch handler owns
//! v4's trim/parseInt/Math.min-max body (including the NaN empty-page quirk) so
//! dispatch and REST agree byte-for-byte. The JSON verb also rides
//! `POST /api/dispatch`; this edge gives v4-URL parity for the SPA's SearchBar.

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::api::{Request as CoreRequest, Response as CoreResponse};

use crate::files_routes::error_json;
use crate::state::SharedState;
use crate::text_replacements_routes::{dispatch_core, error_to_http};

pub async fn ui_search_get(
    State(state): State<SharedState>,
    Query(query): Query<HashMap<String, String>>,
) -> AxumResponse {
    let get = |k: &str| query.get(k).cloned();
    let req = CoreRequest::UiSearch {
        q: get("q"),
        types: get("types"),
        limit: get("limit"),
        offset: get("offset"),
    };
    match dispatch_core(&state, req).await {
        Ok(CoreResponse::UiSearch(v)) => (
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
