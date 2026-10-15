//! The data tick's fold catch-up sweep — port of v4
//! `lib/background-jobs/maintenance/fold-catchup.ts` (`ac6348c5f`, replacing
//! `fold-other-catchup.ts`; bug 232's in-flight chain guard; pin
//! `83334b9a5`).
//!
//! v4's why, carried: for one fold pass, select the idle chats whose cursor
//! is behind ([`find_fold_catchup_candidates`]) and enqueue one catch-up job
//! each — but never a second chain for a chat whose catch-up of the same pass
//! is still queued or running (a capped backlog continues itself; a sweep
//! starting another would double the reads). v4 logs these lines on a child
//! logger bound `module: 'maintenance.fold-catchup'`; the field rides every
//! line here.

use std::collections::HashSet;

use serde_json::Value;

use crate::db::fallback::error_text;
use crate::db::memory_extraction_cursors::ExtractionCursorMap;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::extraction::catchup::{
    find_fold_catchup_candidates, FoldCatchupOptions, FOLD_CATCHUP_MAX_CHATS,
};
use crate::services::queue_service::{
    enqueue_memory_extraction_with, pass_of, MemoryExtractionEnqueue,
};

const MODULE: &str = "maintenance.fold-catchup";

/// v4 `FoldCatchupSweepSummary`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FoldCatchupSweepSummary {
    /// Chats a catch-up job was enqueued for.
    pub enqueued: usize,
    /// Eligible chats beyond the per-sweep cap, left for the next sweep.
    pub deferred: usize,
}

/// v4 `chatsWithCatchupInFlight(pass, userIds)` — chats (by user) with a
/// catch-up of `pass` queued or running.
fn chats_with_catchup_in_flight(
    db: &Db,
    pass: &str,
    user_ids: &[String],
) -> Result<HashSet<String>, DbError> {
    let mut chat_ids = HashSet::new();
    for user_id in user_ids {
        let uid = user_id.clone();
        let jobs = db.read_main(move |conn| {
            let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
            let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
            jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
            Ok(jobs)
        })?;
        for job in jobs.iter().filter(|j| j.job_type == "MEMORY_EXTRACTION") {
            let Ok(payload) = serde_json::from_str::<Value>(&job.payload) else {
                continue;
            };
            let same = match pass_of(&payload) {
                Ok(p) => p.as_str() == pass,
                Err(raw) => raw.as_str() == Some(pass),
            };
            if same {
                if let Some(chat_id) = payload.get("chatId").and_then(Value::as_str) {
                    chat_ids.insert(chat_id.to_string());
                }
            }
        }
    }
    Ok(chat_ids)
}

/// v4 `enqueueFoldCatchups(pass, options)`. The selection's own failure
/// propagates (the tick's `runSweep` records it); an in-flight read failure
/// WARNs and relies on the enqueue dedupe; a failed enqueue WARNs and moves
/// on.
pub async fn enqueue_fold_catchups(
    db: &Db,
    pass: &str,
    now: Option<f64>,
    limit: Option<usize>,
    chats: Option<&[Value]>,
    cursors: Option<&[(String, ExtractionCursorMap)]>,
) -> Result<FoldCatchupSweepSummary, DbError> {
    let limit = limit.unwrap_or(FOLD_CATCHUP_MAX_CHATS);
    let selection = find_fold_catchup_candidates(
        db,
        pass,
        FoldCatchupOptions {
            now,
            limit: Some(limit),
            chats,
            cursors,
        },
    )?;

    let mut in_flight: HashSet<String> = HashSet::new();
    if !selection.candidates.is_empty() {
        let mut user_ids: Vec<String> = Vec::new();
        for c in &selection.candidates {
            if !user_ids.contains(&c.user_id) {
                user_ids.push(c.user_id.clone());
            }
        }
        match chats_with_catchup_in_flight(db, pass, &user_ids) {
            Ok(set) => in_flight = set,
            // Without the job list the enqueue dedupe still stops an
            // identical job.
            Err(error) => tracing::warn!(
                module = MODULE,
                pass = %pass,
                error = %error_text(&error),
                "Could not read in-flight jobs; relying on the enqueue dedupe"
            ),
        }
    }

    let mut enqueued = 0usize;
    let mut already_running = 0usize;
    for candidate in &selection.candidates {
        if in_flight.contains(&candidate.chat_id) {
            already_running += 1;
            tracing::debug!(
                module = MODULE,
                pass = %pass,
                chatId = %candidate.chat_id,
                "Catch-up already queued or running for chat; not starting another"
            );
            continue;
        }
        match enqueue_memory_extraction_with(
            db,
            &candidate.user_id,
            &MemoryExtractionEnqueue {
                chat_id: &candidate.chat_id,
                turn_opener_message_id: None,
                extraction_anchor_message_id: Some(&candidate.anchor_message_id),
                connection_profile_id: &candidate.connection_profile_id,
                pass: Some(pass),
                ..Default::default()
            },
        )
        .await
        {
            Ok(_) => {
                enqueued += 1;
                tracing::debug!(
                    module = MODULE,
                    pass = %pass,
                    chatId = %candidate.chat_id,
                    anchor = %candidate.anchor_message_id,
                    "Enqueued fold catch-up"
                );
            }
            Err(error) => tracing::warn!(
                module = MODULE,
                pass = %pass,
                chatId = %candidate.chat_id,
                error = %error_text(&error),
                "Failed to enqueue fold catch-up"
            ),
        }
    }

    if selection.deferred > 0 {
        tracing::info!(
            module = MODULE,
            pass = %pass,
            enqueued,
            alreadyRunning = already_running,
            deferred = selection.deferred,
            limit,
            "Fold catch-up capped; remaining chats wait for the next sweep"
        );
    } else {
        tracing::debug!(
            module = MODULE,
            pass = %pass,
            enqueued,
            alreadyRunning = already_running,
            "Fold catch-up sweep done"
        );
    }
    Ok(FoldCatchupSweepSummary {
        enqueued,
        deferred: selection.deferred,
    })
}
