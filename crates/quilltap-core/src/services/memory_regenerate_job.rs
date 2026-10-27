//! The memory-regenerate job handlers — ports of v4
//! `lib/background-jobs/handlers/memory-regenerate-chat.ts` and
//! `memory-regenerate-all.ts` (pin `83334b9a5`; work order P4.D296 R-V).
//!
//! `MEMORY_REGENERATE_CHAT` wipes every memory tied to one chat (C1-7's
//! `retire_chat_memories`, which spares MANUAL rows, cold rows and digests with
//! a surviving member) and rebuilds them through the extraction pipeline, as v4
//! does:
//!
//!  - one `MEMORY_EXTRACTION` turn job per turn ([`list_extraction_turns`]: one
//!    per USER opener; one per closing ASSISTANT line in an autonomous room;
//!    one null-opener job for a greeting-only chat), so the rebuilt turns tile
//!    the chat exactly as the live pipeline does;
//!  - both fold passes' cursors cleared, so their rows (removed by the wipe)
//!    are rebuilt from the chat's start: an `other-fold` catch-up is enqueued
//!    now (in the `fold` / `hybrid` modes, where it owns the observations of
//!    others), and an `episode-fold` catch-up over the folded history.
//!
//! Each catch-up reads its backlog in slices, advancing its cursor past each and
//! enqueuing its own successor, so the rebuilt fold-grain rows cover the whole
//! history (bug 206).
//!
//! `MEMORY_REGENERATE_ALL` is the fan-out the HTTP action enqueues: one
//! `MEMORY_REGENERATE_CHAT` per chat the user owns (dangerous chats routed
//! through the dangerous-compatible cheap profile when one is set) and per
//! orphan chat id the memories still name, against ONE snapshot of in-flight
//! chat wipes.

use std::collections::HashSet;

use serde_json::Value;

use crate::db::background_jobs::BackgroundJobsRepository;
use crate::db::fallback::error_text;
use crate::db::runtime::Db;
use crate::db::{chats_read, connection_profiles, instance_settings, memories_read};
use crate::services::dangerous_content::chat_override::should_use_uncensored_route;
use crate::services::memory_retire::{announce_retirement, retire_chat_memories, RetireReport};
use crate::services::queue_service::{
    enqueue_memory_extraction_with, enqueue_memory_regenerate_chat, MemoryExtractionEnqueue,
};
use crate::services::turn_transcript::{list_extraction_turns, load_chat_message_events};

/// The `MEMORY_REGENERATE_CHAT` payload (v4 `MemoryRegenerateChatPayload`).
#[derive(Debug, Clone)]
pub struct MemoryRegenerateChatPayload {
    pub chat_id: String,
    pub connection_profile_id: String,
}

impl MemoryRegenerateChatPayload {
    pub fn decode(payload: &Value) -> Self {
        let s = |k: &str| {
            payload
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            chat_id: s("chatId"),
            connection_profile_id: s("connectionProfileId"),
        }
    }
}

/// The `MEMORY_REGENERATE_ALL` payload (v4 `MemoryRegenerateAllPayload`).
#[derive(Debug, Clone)]
pub struct MemoryRegenerateAllPayload {
    pub standard_profile_id: String,
    pub dangerous_profile_id: String,
}

impl MemoryRegenerateAllPayload {
    pub fn decode(payload: &Value) -> Self {
        let s = |k: &str| {
            payload
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Self {
            standard_profile_id: s("standardProfileId"),
            dangerous_profile_id: s("dangerousProfileId"),
        }
    }
}

/// The wipe, then the announcement after the commit (R-AF).
async fn wipe_chat(db: &Db, chat_id: &str) -> Result<RetireReport, String> {
    let cid = chat_id.to_string();
    let report = db
        .write(move |w| retire_chat_memories(w.main().connection(), &cid))
        .await
        .map_err(|e| error_text(&e))?;
    announce_retirement(db, report.announcement.as_ref()).await;
    Ok(report)
}

/// v4 `handleMemoryRegenerateChat(job)`. `Err(message)` is v4's `throw`.
pub async fn handle_memory_regenerate_chat(
    db: &Db,
    job_id: &str,
    user_id: &str,
    payload: &MemoryRegenerateChatPayload,
) -> Result<(), String> {
    let cid = payload.chat_id.clone();
    let chat = db
        .read_main(move |c| Ok(chats_read::find_by_id_or_none(c, &cid)))
        .map_err(|e| error_text(&e))?;
    let Some(chat) = chat else {
        // Orphaned memories: the chat is gone, just wipe whatever is left and
        // stop.
        let wiped = wipe_chat(db, &payload.chat_id).await?;
        tracing::info!(
            jobId = %job_id,
            chatId = %payload.chat_id,
            deleted = wiped.retired,
            spared = wiped.spared,
            vectorsRemoved = wiped.vectors_removed,
            "[MemoryRegenerateChat] Chat no longer exists; deleted orphan memories"
        );
        return Ok(());
    };

    // Verify the connection profile is still resolvable. If it's not, fail
    // loud rather than silently skipping all the extraction passes downstream.
    let pid = payload.connection_profile_id.clone();
    let profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .map_err(|e| error_text(&e))?;
    if profile.is_none() {
        return Err(format!(
            "Connection profile not found for memory regeneration: {}",
            payload.connection_profile_id
        ));
    }

    let wiped = wipe_chat(db, &payload.chat_id).await?;
    let cid = payload.chat_id.clone();
    let cleared_cursors = db
        .write(move |w| w.main().memory_extraction_cursors().clear_for_chat(&cid))
        .await
        .map_err(|e| error_text(&e))?;
    tracing::debug!(
        jobId = %job_id,
        chatId = %payload.chat_id,
        clearedCursors = cleared_cursors,
        "[MemoryRegenerateChat] Cleared extraction cursors"
    );

    let messages = load_chat_message_events(db, &payload.chat_id).map_err(|e| error_text(&e))?;
    let chat_type = chat.get("chatType").and_then(Value::as_str);
    let turns = list_extraction_turns(&messages, chat_type);

    // The wipe removed this chat's rows, and nothing else enqueues extractions
    // for a chat mid-regenerate, so the per-opener dedupe scan would only cost
    // two queries a turn.
    for turn in &turns {
        enqueue_memory_extraction_with(
            db,
            user_id,
            &MemoryExtractionEnqueue {
                chat_id: &payload.chat_id,
                turn_opener_message_id: turn.turn_opener_message_id.as_deref(),
                extraction_anchor_message_id: turn.extraction_anchor_message_id.as_deref(),
                // An autonomous room's speaker reads past the previous one (bug
                // 245).
                extract_after_message_id: turn.extract_after_message_id.as_deref(),
                connection_profile_id: &payload.connection_profile_id,
                backfill: true,
                skip_dedup_check: true,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| error_text(&e))?;
    }

    let mode = db
        .read_main(|c| Ok(instance_settings::get_memory_extraction_mode_settings(c)))
        .map_err(|e| error_text(&e))?;
    let catchups: &[&str] = if mode.other_pass == "turn" {
        &["episode-fold"]
    } else {
        &["other-fold", "episode-fold"]
    };
    let anchor = format!("regenerate:{job_id}");
    for pass in catchups {
        enqueue_memory_extraction_with(
            db,
            user_id,
            &MemoryExtractionEnqueue {
                chat_id: &payload.chat_id,
                turn_opener_message_id: None,
                extraction_anchor_message_id: Some(&anchor),
                connection_profile_id: &payload.connection_profile_id,
                pass: Some(pass),
                ignore_idle: true,
                backfill: true,
                skip_dedup_check: true,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| error_text(&e))?;
    }

    let chat_type_field = chat_type.unwrap_or("null");
    let catchups_json = serde_json::to_string(catchups).unwrap_or_default();
    tracing::info!(
        jobId = %job_id,
        chatId = %payload.chat_id,
        chatType = %chat_type_field,
        deleted = wiped.retired,
        spared = wiped.spared,
        vectorsRemoved = wiped.vectors_removed,
        turnJobs = turns.len(),
        catchupsJson = %catchups_json,
        "[MemoryRegenerateChat] Wiped chat and enqueued its rebuild"
    );
    Ok(())
}

/// v4 `handleMemoryRegenerateAll(job)` — the fan-out. A failed per-chat
/// enqueue logs v4's ERROR and moves on.
///
/// P4.168 (survey B5) — the three reads v4 answers with repository fallbacks:
///
/// - **the chat list — KEPT STRICT** (P-4, the planner's ruling —
///   `REGENERATE_ALL_STRICT_CHAT_LIST`). v4's `chats.findByUserId` is the base
///   `findByFilter`, which logs `Error finding entities by filter
///   {collection: 'chats', error}` and answers `[]`; the fan-out then queues
///   no per-chat wipe and counts EVERY chat that holds memories as an orphan —
///   a `MEMORY_REGENERATE_CHAT` wipe on the standard profile for each (the
///   dangerous routing lost), so a transient chats-table failure retires and
///   re-extracts every chat's memories. v5 logs v4's line and fails the job
///   (`Err`), so nothing is wiped. A v4 filing candidate (not filed).
/// - **the in-flight job snapshot — CONVERGED**: each `findByUserId(status)`
///   is v4's fallback `Error finding background jobs by user ID {userId}` over
///   the inner `findByFilter` (whose line is the one that fires outside the
///   strict scope) → `[]`; only the dedup short-circuit is lost (the enqueue
///   dedups anyway).
/// - **the distinct memory chatIds — CONVERGED**:
///   [`memories_read::find_distinct_chat_ids`]'s fallback `[]` skips the
///   orphan sweep.
pub async fn handle_memory_regenerate_all(
    db: &Db,
    job_id: &str,
    user_id: &str,
    payload: &MemoryRegenerateAllPayload,
) -> Result<(), String> {
    let uid = user_id.to_string();
    let user_chats = db
        .read_main(move |c| chats_read::find_by_user_id(c, &uid))
        .inspect_err(|e| memories_read::log_find_by_filter_line("chats", e))
        .map_err(|e| error_text(&e))?;
    let valid_chat_ids: HashSet<String> = user_chats
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();

    // Snapshot in-flight regenerate-chat jobs once so the per-chat enqueues
    // can short-circuit their dedup scans.
    let uid = user_id.to_string();
    let in_flight = db
        .read_main(move |c| {
            let repo = BackgroundJobsRepository::new(c);
            let mut jobs = background_jobs_by_user_or_empty(&uid, || {
                repo.find_by_user_id(&uid, Some("PENDING"))
            })?;
            jobs.extend(background_jobs_by_user_or_empty(&uid, || {
                repo.find_by_user_id(&uid, Some("PROCESSING"))
            })?);
            Ok(jobs)
        })
        .map_err(|e| error_text(&e))?;
    let mut in_flight_chat_ids: HashSet<String> = HashSet::new();
    for job in in_flight
        .iter()
        .filter(|j| j.job_type == "MEMORY_REGENERATE_CHAT")
    {
        if let Ok(p) = serde_json::from_str::<Value>(&job.payload) {
            if let Some(chat_id) = p.get("chatId").and_then(Value::as_str) {
                if !chat_id.is_empty() {
                    in_flight_chat_ids.insert(chat_id.to_string());
                }
            }
        }
    }

    let (mut chats_queued, mut dangerous_routed, mut chats_skipped_duplicate) =
        (0usize, 0usize, 0usize);
    for chat in &user_chats {
        let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
        if in_flight_chat_ids.contains(chat_id) {
            chats_skipped_duplicate += 1;
            continue;
        }
        // Off-duty chats forfeit the dangerous profile — the operator has
        // explicitly opted them out of any uncensored routing for the chat.
        let effectively_dangerous = should_use_uncensored_route(Some(chat));
        let profile_id = if effectively_dangerous {
            &payload.dangerous_profile_id
        } else {
            &payload.standard_profile_id
        };
        if effectively_dangerous && profile_id != &payload.standard_profile_id {
            dangerous_routed += 1;
        }
        match enqueue_memory_regenerate_chat(db, user_id, chat_id, profile_id).await {
            Ok(_) => chats_queued += 1,
            Err(error) => tracing::error!(
                chatId = %chat_id,
                error = %error_text(&error),
                "[MemoryRegenerateAll] Failed to enqueue per-chat wipe"
            ),
        }
    }

    let distinct_chat_ids = db
        .read_main(memories_read::find_distinct_chat_ids)
        .map_err(|e| error_text(&e))?;
    let orphan_chat_ids: Vec<String> = distinct_chat_ids
        .into_iter()
        .filter(|id| !valid_chat_ids.contains(id) && !in_flight_chat_ids.contains(id))
        .collect();

    let mut orphans_queued = 0usize;
    for orphan_chat_id in &orphan_chat_ids {
        match enqueue_memory_regenerate_chat(
            db,
            user_id,
            orphan_chat_id,
            &payload.standard_profile_id,
        )
        .await
        {
            Ok(_) => orphans_queued += 1,
            Err(error) => tracing::error!(
                orphanChatId = %orphan_chat_id,
                error = %error_text(&error),
                "[MemoryRegenerateAll] Failed to enqueue orphan wipe"
            ),
        }
    }

    tracing::info!(
        jobId = %job_id,
        chatsQueued = chats_queued,
        chatsSkippedDuplicate = chats_skipped_duplicate,
        dangerousRouted = dangerous_routed,
        orphansQueued = orphans_queued,
        standardProfileId = %payload.standard_profile_id,
        dangerousProfileId = %payload.dangerous_profile_id,
        "[MemoryRegenerateAll] Fan-out complete"
    );
    Ok(())
}

/// v4 `backgroundJobs.findByUserId(userId, status)` as the fan-out sees it
/// (`background-jobs.repository.ts:120-138`, P4.168 B5): the inner
/// `findByFilter` fallback (`Error finding entities by filter {collection:
/// 'background_jobs', error}` → `[]`) is the line that fires outside the strict
/// scope; inside it that line gains `strictFailures=true` and the method's own
/// `Error finding background jobs by user ID {collection, userId, error,
/// strictFailures}` fires before the `Err` propagates.
fn background_jobs_by_user_or_empty<T>(
    user_id: &str,
    read: impl FnOnce() -> Result<Vec<T>, crate::db::DbError>,
) -> Result<Vec<T>, crate::db::DbError> {
    crate::db::fallback::find_by_filter_strict_aware("background_jobs", read).inspect_err(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "background_jobs",
            userId = %user_id,
            error = %error_text(error),
            strictFailures = true,
            "Error finding background jobs by user ID"
        );
    })
}
