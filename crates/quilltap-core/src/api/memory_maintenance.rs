//! The memory-maintenance dispatch handlers (P4.9H2A tier 2) — memory
//! deduplication + conversation-summaries regeneration.
//!
//! **Both surfaces are DEFERRED LOUDLY this round** (the loud `not_available`
//! idiom). Their deps are already ported (`cosine_similarity`,
//! `extract_novel_details`, the `delete_many_with_unlink` chokepoint,
//! `update_for_character`, `characters_read::find_all`, the vault-mirror
//! machinery, the `REGENERATE_CONVERSATION_SUMMARIES` job-type registration) and
//! the committed `embedding-profiles-{main,mount}.db` fixture already carries the
//! crafted near-duplicate memories — so the follow-up is the port of v4's
//! `deduplicateAllMemories` (446 LOC — union-find + dim-grouped cosine +
//! scoreMemory + the novel-detail merge) and the summaries handler, each with its
//! differential. Until then these four §1 verbs answer a named refusal so the SPA
//! reaches a clean, honest arm rather than a parse error.

use super::types::{ErrorKind, Response};

fn not_available(surface: &str) -> Response {
    Response::error(
        ErrorKind::Internal,
        format!("The '{surface}' memory-maintenance action is recognized but not yet available."),
    )
}

/// v4 `?action=memory-dedup-preview` / `memory-dedup` — the synchronous inline
/// dedup (dry run / apply). DEFERRED LOUDLY.
pub fn memory_dedup(preview: bool) -> Response {
    not_available(if preview {
        "memory-dedup-preview"
    } else {
        "memory-dedup"
    })
}

/// v4 conversation-summaries `?action=regenerate` (GET status / POST enqueue).
/// DEFERRED LOUDLY.
pub fn conversation_summaries_regenerate(status: bool) -> Response {
    not_available(if status {
        "conversation-summaries-status"
    } else {
        "conversation-summaries-regenerate"
    })
}
