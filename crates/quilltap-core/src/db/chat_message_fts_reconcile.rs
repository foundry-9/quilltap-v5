//! Chat-message FTS reconciliation (the boot self-heal) — a whole port of v4
//! `lib/startup/reconcile-chat-message-fts.ts` (`f45a517a9`).
//!
//! The message search index ([`super::chat_message_fts`]) is maintained by
//! triggers, which is what makes it impossible for a new write path to bypass.
//! It is not, however, impossible for the SCHEMA to go missing:
//!
//!  - Any table rebuild of `chat_messages` — the
//!    `CREATE new … INSERT … SELECT … DROP … RENAME` pattern several
//!    migrations use on other tables — reassigns every rowid and **silently
//!    drops the triggers with the old table**. No error is raised; the index
//!    simply stops being updated, and search quietly goes stale.
//!  - A restore or a hand-repaired database can arrive without the objects at
//!    all.
//!  - **And the case v4 does not have: a fresh v5-PROVISIONED instance has
//!    none of the five objects and no mechanism that could ever supply them.**
//!    v4 creates them in the migration `create-chat-message-fts-v1`, not in
//!    `ensureCollection`, so v5's D23 schema dump — which keeps only `table`
//!    and `index` rows from v4's `generateDDL`, filtering triggers and virtual
//!    tables out by construction — cannot carry them. This pass is therefore
//!    where every v5 instance GETS its index, not merely where it heals.
//!
//! That is the one silent failure mode in the design, so this turns it into a
//! self-healing one. Every boot:
//!
//!  1. Replay the `IF NOT EXISTS` DDL — a no-op on a healthy instance, and the
//!     thing that puts dropped triggers back.
//!  2. Compare eligible `chat_messages` rows against `chat_messages_fts_map`
//!     rows. Two counts on indexed columns; cheap enough to run
//!     unconditionally.
//!  3. On a mismatch, log at `warn` and rebuild.
//!
//! Runs on the writer task (v5's sole DB writer), like the other boot repairs
//! — v4 runs it in the parent for the same reason.
//!
//! ## The migration comes free
//!
//! v4's `create-chat-message-fts-v1` migration does exactly this work
//! (`shouldRun` = objects missing OR counts differ; `run` = ensure + rebuild),
//! so there is nothing else to port. Its operator-facing strings, which v5
//! emits nowhere because v5 has no migration runner, are recorded beside the
//! reconciler for the day one exists:
//!
//! - label: `Cataloguing every line ever spoken, so the search bar can find it
//!   in a blink…`
//! - message: `` `Indexed ${n} message${n === 1 ? '' : 's'} for search` `` , or
//!   `No messages needed indexing`
//! - `dependsOn: ['sqlite-initial-schema-v1']`, `introducedInVersion: '4.10.0'`
//!
//! ## One v4 arm has no v5 twin
//!
//! v4 opens with `const db = getRawDatabase(); if (!db) { debug('No SQLite
//! database available; skipping chat message FTS reconciliation'); return … }`.
//! v5 takes the connection as an argument and the writer task always has one,
//! so that arm is unreachable here BY CONSTRUCTION rather than unported — and
//! the reconcile differential records it as v4-only for the same reason (there
//! is no v5 call that could produce it).

use rusqlite::Connection;

use super::chat_message_fts::{
    count_eligible_chat_messages, count_indexed_chat_messages, ensure_chat_message_fts_schema,
    missing_chat_message_fts_objects, rebuild_chat_message_fts_index,
};
use super::DbError;

/// The `context` field on every line this pass logs (v4's
/// `createServiceLogger('Startup:ChatMessageFtsReconcile')`).
const LOG_CONTEXT: &str = "startup.chat-message-fts-reconcile";

/// What [`reconcile_chat_message_fts`] found and did (v4
/// `ChatMessageFtsReconcileResult`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatMessageFtsReconcileResult {
    /// Schema objects that had to be (re)created.
    pub restored: Vec<String>,
    /// Eligible `chat_messages` rows.
    pub eligible: i64,
    /// Rows the index actually held before any rebuild.
    pub indexed: i64,
    /// Whether a full rebuild ran.
    pub rebuilt: bool,
}

/// Ensure the message search index exists and is in step with the transcript
/// (v4 `reconcileChatMessageFts`).
///
/// Idempotent and a no-op on a healthy instance. **Total**: a broken database
/// must never keep the instance from starting, so every failure is swallowed
/// with v4's warn and the partial result is returned — search degrades to the
/// exact-scan fallback, which is slow but correct.
pub fn reconcile_chat_message_fts(conn: &Connection) -> ChatMessageFtsReconcileResult {
    let mut result = ChatMessageFtsReconcileResult::default();

    match reconcile_inner(conn, &mut result) {
        Ok(()) => {}
        Err(err) => {
            tracing::warn!(
                target: "quilltap::boot",
                context = LOG_CONTEXT,
                error = %err,
                "Chat message FTS reconciliation failed; search may be degraded",
            );
        }
    }

    result
}

/// v4's `try` body. Every `?` here is one of v4's `await`s inside that try, so
/// the outer catch above lands in exactly the same places.
fn reconcile_inner(
    conn: &Connection,
    result: &mut ChatMessageFtsReconcileResult,
) -> Result<(), DbError> {
    result.restored = missing_chat_message_fts_objects(conn)?;
    if !result.restored.is_empty() {
        tracing::warn!(
            target: "quilltap::boot",
            context = LOG_CONTEXT,
            // v4 logs an ARRAY here. `tracing` has no structured-value
            // channel (a `?`-formatted `Vec` renders Rust's `Debug`), so the
            // callsite serializes and the file layer's `…Json` convention
            // re-parses it back into an array under the unsuffixed name —
            // `quilltap_web::log_file::JSON_FIELD_SUFFIX`.
            missingJson = serde_json::to_string(&result.restored)
                .unwrap_or_else(|_| "[]".to_string())
                .as_str(),
            "Message search index objects were missing; recreating",
        );
    }
    ensure_chat_message_fts_schema(conn)?;

    result.eligible = count_eligible_chat_messages(conn)?;
    result.indexed = count_indexed_chat_messages(conn)?;
    tracing::debug!(
        target: "quilltap::boot",
        context = LOG_CONTEXT,
        eligible = result.eligible,
        indexed = result.indexed,
        "Message search index counts",
    );

    if result.eligible != result.indexed {
        tracing::warn!(
            target: "quilltap::boot",
            context = LOG_CONTEXT,
            eligible = result.eligible,
            indexed = result.indexed,
            "Message search index is out of step with the transcript; rebuilding",
        );
        let rebuild = rebuild_chat_message_fts_index(conn, None)?;
        result.rebuilt = true;
        tracing::info!(
            target: "quilltap::boot",
            context = LOG_CONTEXT,
            indexed = rebuild.indexed,
            durationMs = rebuild.duration_ms,
            "Message search index rebuilt",
        );
    }
    Ok(())
}
