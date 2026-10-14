//! Admission — how a memory enters or changes in the Commonplace Book: the
//! STATEFUL half of v4's `lib/memory/admission/` at the `83334b9a5` pin
//! (`8af764514` Phase C, `af48f6cc2` Phase B's hand edit, `ac6348c5f` Phase D's
//! rate ledger and dry-run book, bugs 200 / 203 / 204 / 209 / 220 / 226 / 231 /
//! 234 / 237), over P4.D289's pure [`crate::admission`] (the decider, THE ONE
//! COMBINE, the thresholds) and P4.D288's index store over rows.
//!
//! v4's barrel (`admission/index.ts`), carried:
//!
//!  - [`admit_candidate`] — the one way a candidate becomes (or reinforces) a
//!    row;
//!  - [`crate::admission::decide_admission`] — the pure decision;
//!  - [`crate::admission::fold_into`] / [`apply_fold`] — the one combine, and
//!    its writes;
//!  - [`update_memory`] — a hand edit.
//!
//! **v5 has no job child.** v4 splits admission across processes (the child
//! prepares and embeds, the parent searches, decides and writes over
//! host-RPC). v5's in-process runner IS the parent, so every admission runs
//! the parent half in place; the child's half — the host-RPC `admitMemory` /
//! `countRecentAdmissions`, `patchChildIndex` and its DEBUG, the two
//! job-child refusals — is NO-PORT (LR-4, ratified by P4.D303's R-AL record).

mod admit;
mod apply;
mod dry_run_book;
mod links;
mod neighbours;
mod prepare;
mod rate_ledger;
mod update_memory;

pub use admit::{
    admission_content_key, admit_candidate, admit_row, reset_admission_state_for_tests,
    with_admission_lock, AdmissionKeyScope, AdmitOptions, ADMISSION_LEDGER_MAX,
    ADMISSION_LEDGER_TTL_MS, EMBEDDING_RETRY_DELAY_MS,
};
pub use apply::{apply_fold, insert_row, raise_importance, reembed_row, without_embedding};
pub use dry_run_book::{
    book_neighbours, create_dry_run_book, record_book_insert, BookEntry, DryRunBook,
};
pub use links::link_both_ways;
pub use neighbours::{
    gather_neighbours, GatheredNeighbours, NeighbourProbe, ADMISSION_MAX_SEARCH_K, MAX_DIGEST_HOPS,
};
pub use prepare::{apply_episodic_fallback_anchors, apply_name_presence_check};
pub use rate_ledger::{
    count_recent_admissions, record_admission_insert, reset_rate_ledger_for_tests,
    RATE_LEDGER_WINDOW_MS,
};
pub use update_memory::{update_memory, MemoryEditPatch, UpdateMemoryResult};

use serde_json::Value;

use crate::memory_embedding_text::{AnchorEntities, EmbeddingTextSource};

/// The embedded-text view of a memory row's JSON (v4 passes the whole
/// `Memory` to `embeddingTextFor` / `lexicalEmbeddingTextFor`).
pub(crate) fn text_source_of(memory: &Value) -> EmbeddingTextSource<'_> {
    let text = |key: &str| memory.get(key).and_then(Value::as_str);
    EmbeddingTextSource {
        summary: text("summary").unwrap_or_default(),
        content: text("content").unwrap_or_default(),
        occurred_at: text("occurredAt"),
        narrative_time: text("narrativeTime"),
        entities: memory
            .get("entities")
            .and_then(Value::as_array)
            .map_or(AnchorEntities::None, |a| AnchorEntities::Json(a.as_slice())),
    }
}

/// `String(x ?? null)` for a log field — v4 writes an explicit `null`, which
/// the file layer renders as the bare word.
pub(crate) fn or_null(value: Option<&str>) -> &str {
    value.unwrap_or("null")
}

/// The current wall clock in ms as a JS number.
pub(crate) fn now_ms() -> f64 {
    crate::clock::now_unix_ms() as f64
}

#[cfg(test)]
mod tests;
