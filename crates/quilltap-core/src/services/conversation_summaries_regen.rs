//! Conversation-summaries regeneration (v4
//! `lib/background-jobs/handlers/regenerate-conversation-summaries.ts` +
//! `app/api/v1/system/conversation-summaries/route.ts`) — a backfill that
//! re-mirrors every summarized chat's `contextSummary` into its participant
//! character vaults under "Conversation Summaries/" (the files the Commonplace
//! Book's relevant-conversations retrieval reads).
//!
//! It does NOT regenerate summaries and enqueues no other job: it walks the
//! user's chats that already carry a non-empty `contextSummary`, and for each
//! writes the summary into every participant vault via the existing
//! [`write_conversation_summary_to_vaults`] bridge (idempotent — the bridge
//! replaces a conversation's prior file by its frontmatter UUID). Best-effort
//! per chat: one bad chat never aborts the rest.
//!
//! v4's forked-child host-RPC short-circuit is a locked v5 non-port — v5 writes
//! directly on the writer task like every other fold write.

use serde_json::Value;

use crate::db::background_jobs::{BackgroundJob, BackgroundJobsRepository};
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, DbError};
use crate::jsstr::js_trim;
use crate::services::conversation_summary_vault_bridge::{
    compute_conversation_stats, write_conversation_summary_to_vaults, WriteConversationSummaryInput,
};
use crate::services::job_runner::{JobFuture, JobHandler, JobOutcome};

/// v4's GET `?action=regenerate` status: count PENDING+PROCESSING
/// `REGENERATE_CONVERSATION_SUMMARIES` jobs for the user.
pub fn count_in_flight(db: &Db, user_id: &str) -> Result<i64, DbError> {
    let uid = user_id.to_string();
    db.read_main(move |conn| {
        let repo = BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs
            .iter()
            .filter(|j| j.job_type == "REGENERATE_CONVERSATION_SUMMARIES")
            .count() as i64)
    })
}

/// The `REGENERATE_CONVERSATION_SUMMARIES` job handler — re-mirror every
/// summarized chat's summary into its participant vaults.
pub async fn handle_regenerate_conversation_summaries(
    db: &Db,
    job: &BackgroundJob,
) -> Result<(), DbError> {
    let uid = job.user_id.clone();
    let user_chats = db.read_main(move |conn| chats_read::find_by_user_id(conn, &uid))?;

    // v4: `typeof c.contextSummary === 'string' && c.contextSummary.trim().length > 0`.
    let summarized: Vec<&Value> = user_chats
        .iter()
        .filter(|c| {
            c.get("contextSummary")
                .and_then(Value::as_str)
                .is_some_and(|s| !js_trim(s).is_empty())
        })
        .collect();

    let now_iso = crate::clock::now_iso();
    let summarized_count = summarized.len();
    let mut mirrored = 0usize;
    let mut failed = 0usize;

    for chat in summarized {
        let chat_id = chat
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // participantCharacterIds = Array.from(new Set(participants.map(p => p.characterId))).
        let mut participant_character_ids: Vec<String> = Vec::new();
        if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
            for p in parts {
                if let Some(cid) = p.get("characterId").and_then(Value::as_str) {
                    if !participant_character_ids.iter().any(|x| x == cid) {
                        participant_character_ids.push(cid.to_string());
                    }
                }
            }
        }
        if participant_character_ids.is_empty() {
            continue;
        }

        // v4 wraps the per-chat body in try/catch (best-effort). The message read
        // is the only fallible step; the vault bridge swallows its own errors.
        let cid_for_read = chat_id.clone();
        let all_messages = match db
            .read_main(move |conn| chats_messages_read::get_messages(conn, &cid_for_read))
        {
            Ok(m) => m,
            Err(e) => {
                failed += 1;
                tracing::warn!(
                    chat_id = %chat_id,
                    error = %e,
                    "[RegenerateConversationSummaries] Failed to re-mirror a chat summary",
                );
                continue;
            }
        };
        let stats = compute_conversation_stats(&all_messages);

        let input = WriteConversationSummaryInput {
            chat_id: chat_id.clone(),
            chat_title: chat
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            summary: chat
                .get("contextSummary")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            // v4: `chat.compactionGeneration ?? 0`.
            summary_generation: chat
                .get("compactionGeneration")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            participant_character_ids,
            message_count: stats.message_count,
            first_message_at: stats.first_message_at,
            last_message_at: stats.last_message_at,
            updated_at: now_iso.clone(),
        };
        write_conversation_summary_to_vaults(db, &input).await;
        mirrored += 1;
    }

    tracing::info!(
        job_id = %job.id,
        user_id = %job.user_id,
        summarized_chats = summarized_count,
        mirrored,
        failed,
        "[RegenerateConversationSummaries] Backfill complete",
    );
    Ok(())
}

/// The seam-free (DB-only, no model wire) `REGENERATE_CONVERSATION_SUMMARIES`
/// handler for the host registry.
pub struct RegenerateConversationSummariesHandler;

impl JobHandler for RegenerateConversationSummariesHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(async move {
            match handle_regenerate_conversation_summaries(db, job).await {
                Ok(()) => JobOutcome::Completed(None),
                Err(e) => JobOutcome::Failed(e.to_string()),
            }
        })
    }
}
