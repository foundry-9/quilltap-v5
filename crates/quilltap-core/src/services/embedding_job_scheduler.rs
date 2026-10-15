//! The embedding job scheduler — P4.D292 (R-Q, L-8), a port of v4
//! `lib/embedding/embedding-job-scheduler.ts` at the `83334b9a5` pin: the
//! debounced vocabulary refit check for BUILTIN profiles ([`schedule_refit`])
//! and the embedding-status cleanup when an embeddable entity is deleted
//! ([`handle_entity_deletion`]).
//!
//! "Multiple rapid calls will be collapsed into a single check; a forced call
//! keeps the collapsed check forced. … When the timer fires, the refit (with
//! its full reindex) is enqueued only if the vocabulary needs one
//! (`assessBuiltinRefit`: unfitted, fitted before the text-shape change, or
//! drifted), or when forced — a single memory edit is embedded against the
//! current vocabulary and is not by itself a reason to re-embed everything."
//!
//! ## The timer is a deadline table; the host owns the cadence
//!
//! v4 keeps a `Map<key, Timeout>` (re-armed on every call — `clearTimeout` +
//! a fresh `setTimeout(5000)`) and a `Set` of forced keys. Timers do not live
//! in the core (the W4.8 rule), so the port keeps the same state as data: a
//! process-global table of pending entries, each with its deadline (`now_ms +
//! 5000`, re-armed on every call) and its forced flag (OR-ed on every call,
//! as v4's `forcedRefits.add` is never undone before the fire). The host's
//! driver sleeps until [`next_deadline`] (woken by [`schedule_notify`] on every
//! schedule) and calls [`fire_due_refits`] with its wall clock; a unit test
//! drives the same two calls with an injected clock. Without a driver (CLI
//! direct mode, a unit test) nothing fires — v4's `setTimeout` dies with its
//! process the same way (recorded, the order's Tier 3 #20).
//!
//! v4 puts both maps on `processSingleton` (bug 235: Next.js bundle copies) —
//! NO-PORT: one Rust process, the static IS process-wide.

use std::sync::Mutex;

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::builtin_refit_policy::{enqueue_refit_if_due, RefitDecisionReason};

/// v4 `REFIT_DEBOUNCE_MS` — "Default debounce delay for refit scheduling (5
/// seconds)".
pub const REFIT_DEBOUNCE_MS: i64 = 5000;

/// One pending refit check (v4: a live timer + maybe a forced-set entry).
#[derive(Debug, Clone)]
struct PendingRefit {
    /// v4's debounce key `${userId}:${profile.id}`.
    key: String,
    user_id: String,
    profile_id: String,
    /// The profile's owner (v4 `profile.userId` — what the refit job is
    /// enqueued under).
    profile_user_id: String,
    forced: bool,
    deadline_ms: i64,
    /// Arming order — v4's timer ids; two timers due at the same instant fire
    /// in the order they were set.
    seq: u64,
}

struct PendingTable {
    entries: Vec<PendingRefit>,
    next_seq: u64,
}

static PENDING: Mutex<PendingTable> = Mutex::new(PendingTable {
    entries: Vec::new(),
    next_seq: 0,
});

static SCHEDULE_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// The host driver's wake: notified on every [`schedule_refit`] that armed
/// (or re-armed) a check, so a sleeping driver re-reads [`next_deadline`].
pub fn schedule_notify() -> &'static tokio::sync::Notify {
    &SCHEDULE_NOTIFY
}

/// The earliest pending deadline, or `None` when nothing is scheduled.
pub fn next_deadline() -> Option<i64> {
    PENDING
        .lock()
        .unwrap()
        .entries
        .iter()
        .map(|e| e.deadline_ms)
        .min()
}

/// How many refit checks are pending (v4's live-timer count).
pub fn pending_count() -> usize {
    PENDING.lock().unwrap().entries.len()
}

/// Drop every pending check — test isolation only (the table is
/// process-global, as v4's maps are).
#[doc(hidden)]
pub fn clear_pending_for_tests() {
    PENDING.lock().unwrap().entries.clear();
}

/// v4 `scheduleRefit(userId, profileId?, { force })` (`embedding-job-
/// scheduler.ts:51-122`): resolve the profile — `findById(profileId)` when
/// given, else the USER's `findDefault(userId)` (not the instance default
/// `checkBuiltinRefit` reads) — and return silently when there is none or its
/// provider is not `BUILTIN` (v4's literal: provider ROUTING, not the
/// lexical-scale predicate). Otherwise arm (or re-arm) the check for
/// `"<userId>:<profileId>"` at `now_ms + 5000`, OR `force` into its flag, and
/// log DEBUG `[EmbeddingScheduler] Refit check scheduled (debounced) {context:
/// 'scheduleRefit', userId, profileId, forced}` (`forced` = the flag after the
/// OR — v4's `forcedRefits.has(key)`). A profile read failure is the caller's
/// `Err` (v4's rejected promise — each caller WARNs).
pub async fn schedule_refit(
    db: &Db,
    user_id: &str,
    profile_id: Option<&str>,
    force: bool,
    now_ms: i64,
) -> Result<(), DbError> {
    let (uid, pid) = (user_id.to_string(), profile_id.map(str::to_string));
    let profile = db.read_main(move |conn| match pid {
        Some(pid) => crate::db::embedding_profiles::find_by_id(conn, &pid),
        None => crate::db::embedding_profiles::find_default(conn, &uid),
    })?;
    let Some(profile) = profile else {
        return Ok(());
    };
    if profile.provider != "BUILTIN" {
        return Ok(());
    }
    let key = format!("{user_id}:{}", profile.id);
    let forced = {
        let mut table = PENDING.lock().unwrap();
        let seq = table.next_seq;
        table.next_seq += 1;
        let deadline_ms = now_ms + REFIT_DEBOUNCE_MS;
        match table.entries.iter_mut().find(|e| e.key == key) {
            Some(entry) => {
                entry.forced |= force;
                entry.deadline_ms = deadline_ms;
                entry.seq = seq;
                entry.profile_user_id = profile.user_id.clone();
                entry.forced
            }
            None => {
                table.entries.push(PendingRefit {
                    key,
                    user_id: user_id.to_string(),
                    profile_id: profile.id.clone(),
                    profile_user_id: profile.user_id.clone(),
                    forced: force,
                    deadline_ms,
                    seq,
                });
                force
            }
        }
    };
    tracing::debug!(
        target: "quilltap::embedding",
        context = "scheduleRefit",
        userId = %user_id,
        profileId = %profile.id,
        forced = forced,
        "[EmbeddingScheduler] Refit check scheduled (debounced)"
    );
    SCHEDULE_NOTIFY.notify_one();
    Ok(())
}

/// Pop the earliest check due at `now_ms` (deadline, then arming order).
fn pop_due(now_ms: i64) -> Option<PendingRefit> {
    let mut table = PENDING.lock().unwrap();
    let index = table
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.deadline_ms <= now_ms)
        .min_by_key(|(_, e)| (e.deadline_ms, e.seq))
        .map(|(i, _)| i)?;
    Some(table.entries.remove(index))
}

/// Fire every check due at `now_ms`, one at a time (v4's timer callback):
/// `enqueueRefitIfDue(profile, { force: forced, source: 'scheduleRefit' })`,
/// then INFO `[EmbeddingScheduler] Refit not needed; the vocabulary is current
/// {context, userId, profileId}` when there is no reason, INFO
/// `[EmbeddingScheduler] Refit job scheduled (debounced) {context, userId,
/// profileId, reason}` when it enqueued (`reason` may be `forced`), nothing
/// for a due-but-held decision ("the policy already did"), and ERROR
/// `[EmbeddingScheduler] Failed to schedule refit job {context, userId,
/// profileId, error}` on a failure. Answers how many checks fired.
pub async fn fire_due_refits(db: &Db, now_ms: i64) -> usize {
    let mut fired = 0usize;
    while let Some(entry) = pop_due(now_ms) {
        fired += 1;
        match enqueue_refit_if_due(
            db,
            &entry.profile_id,
            &entry.profile_user_id,
            entry.forced,
            "scheduleRefit",
        )
        .await
        {
            Ok(decision) => match decision.reason {
                None => tracing::info!(
                    target: "quilltap::embedding",
                    context = "scheduleRefit",
                    userId = %entry.user_id,
                    profileId = %entry.profile_id,
                    "[EmbeddingScheduler] Refit not needed; the vocabulary is current"
                ),
                Some(reason) if decision.enqueued => tracing::info!(
                    target: "quilltap::embedding",
                    context = "scheduleRefit",
                    userId = %entry.user_id,
                    profileId = %entry.profile_id,
                    reason = RefitDecisionReason::as_str(reason),
                    "[EmbeddingScheduler] Refit job scheduled (debounced)"
                ),
                Some(_) => {}
            },
            Err(e) => tracing::error!(
                target: "quilltap::embedding",
                context = "scheduleRefit",
                userId = %entry.user_id,
                profileId = %entry.profile_id,
                error = %crate::db::fallback::error_text(&e),
                "[EmbeddingScheduler] Failed to schedule refit job"
            ),
        }
    }
    fired
}

/// v4 `handleEntityDeletion(entityType, entityId)` (`:130-137`) — "Handle
/// memory deletion - clean up embedding status": `embeddingStatus.
/// deleteByEntity`, a RETHROW `safeQuery` (ERROR `Error deleting embedding
/// status by entity {collection, entityType, entityId, error}`, then the
/// caller's `Err`). Immediate — not a timing seam. Answers the rows deleted.
pub async fn handle_entity_deletion(
    db: &Db,
    entity_type: &str,
    entity_id: &str,
) -> Result<usize, DbError> {
    let (t, id) = (entity_type.to_string(), entity_id.to_string());
    let outcome = db
        .write(move |writers| writers.main().embedding_status().delete_by_entity(&t, &id))
        .await;
    outcome.inspect_err(|e| {
        crate::db::fallback::log_embedding_status_delete_failure(entity_type, entity_id, e)
    })
}
