//! The memory-maintenance dispatch handlers (P4.9H2A tier 2) — memory
//! deduplication + conversation-summaries regeneration.
//!
//! **Memory dedup is LIVE** (P4.43 unit 6; v4 `?action=memory-dedup-preview` /
//! `memory-dedup` on `/api/v1/system/tools`): the synchronous inline dedup over
//! [`crate::services::memory_dedup`] (union-find + dim-grouped cosine +
//! scoreMemory + the novel-detail merge, through the delete-with-unlink
//! chokepoint).
//!
//! **Conversation-summaries regeneration is LIVE** (P4.43 unit 7; v4
//! `?action=regenerate` on `/api/v1/system/conversation-summaries`): GET reports
//! the in-flight count, POST enqueues the `REGENERATE_CONVERSATION_SUMMARIES`
//! backfill (re-mirror existing `contextSummary`s into the character vaults —
//! see [`crate::services::conversation_summaries_regen`]).

use serde_json::json;

use crate::db::runtime::Db;
use crate::services::conversation_summaries_regen::count_in_flight;
use crate::services::memory_dedup::deduplicate_all_memories;
use crate::services::queue_service::enqueue_regenerate_conversation_summaries;

use super::types::{ErrorKind, Response};

fn internal_fixed(sentence: &str, err: impl std::fmt::Display) -> Response {
    // v4 returns a FIXED per-route sentence and logs the real error (P4.D50).
    tracing::error!(error = %err, "{sentence}");
    Response::error(ErrorKind::Internal, sentence)
}

/// v4 `?action=memory-dedup-preview` (GET, dry run) / `memory-dedup` (POST,
/// apply) — `{success, result}`. Threshold defaults to 0.80 and must be in
/// `[0.5, 1.0]` (v4's exact 400 sentence).
pub async fn memory_dedup(
    db: &Db,
    user_id: &str,
    threshold: Option<f64>,
    preview: bool,
) -> Response {
    let threshold = threshold.unwrap_or(0.80);
    // v4: `isNaN(threshold) || threshold < 0.5 || threshold > 1.0`. A NaN is not
    // contained in the range, so `!contains` covers v4's `isNaN` arm too.
    if !(0.5..=1.0).contains(&threshold) {
        return Response::error(
            ErrorKind::BadRequest,
            "Invalid threshold. Must be a number between 0.5 and 1.0",
        );
    }
    match deduplicate_all_memories(db, user_id, threshold, !preview).await {
        Ok(result) => Response::MemoryMaintenance(json!({ "success": true, "result": result })),
        Err(e) => internal_fixed(
            if preview {
                "Failed to preview memory deduplication"
            } else {
                "Failed to deduplicate memories"
            },
            e,
        ),
    }
}

/// v4 conversation-summaries `?action=regenerate`: GET → the in-flight count
/// `{success, inFlight}`; POST → enqueue `{success, jobId, message}` (forking the
/// message on `isNew`).
pub async fn conversation_summaries_regenerate(db: &Db, user_id: &str, status: bool) -> Response {
    if status {
        return match count_in_flight(db, user_id) {
            Ok(in_flight) => {
                Response::MemoryMaintenance(json!({ "success": true, "inFlight": in_flight }))
            }
            // v4's GET status has no dedicated catch — a repo error surfaces as a
            // 500; the handler is a pure count read.
            Err(e) => internal_fixed("Failed to read regeneration status", e),
        };
    }

    match enqueue_regenerate_conversation_summaries(db, user_id).await {
        Ok((job_id, is_new)) => Response::MemoryMaintenance(json!({
            "success": true,
            "jobId": job_id,
            "message": if is_new {
                "Conversation summaries are being re-mirrored into the character vaults in the background."
            } else {
                "A summary regeneration is already in flight; the existing one will complete."
            },
        })),
        Err(e) => internal_fixed("Failed to enqueue summary regeneration", e),
    }
}
