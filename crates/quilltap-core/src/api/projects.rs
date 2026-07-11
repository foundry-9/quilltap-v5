//! The Projects-server dispatch handlers (P4.6k) — the Prospero projects
//! route-logic backfill. Filled unit-by-unit; see `api::groups` for the shared
//! shape. Until a handler lands, its engine arm answers the loud
//! [`not_available`] refusal (never a silent stub).

use super::types::{ErrorKind, Response};

/// The loud "recognized but not yet available" refusal for projects-family
/// variants whose handler lands in a later P4.6k unit.
pub fn not_available(action: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{action}' projects action is recognized but not yet available."),
    )
}
