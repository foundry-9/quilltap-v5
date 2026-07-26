//! The background-job enqueue helpers + queue admin/read surface (v4
//! `lib/background-jobs/queue-service.ts`).
//!
//! ## The `ensureProcessorRunning()` closure (W4.8)
//!
//! v4 auto-starts the in-process job processor on every `enqueueJob`. In v5 the
//! runner is already running (the host driver owns its cadence — see
//! [`crate::services::job_runner`]); an enqueue only needs to **wake** it so it
//! pumps immediately rather than waiting for the poll interval. Instead of
//! coupling `enqueue_job` to a `JobRunner` handle, the host registers a
//! process-global wake hook once at startup ([`set_wake_hook`]); every enqueue
//! calls it (a no-op until registered — faithful to a runner that hasn't started).
//! This is the exact analogue of v4's `ensureProcessorRunning()` singleton call
//! inside `enqueueJob`.

use std::sync::OnceLock;

use serde_json::Value;

use crate::clock::now_iso;
use crate::db::background_jobs::{BjCreate, CreateOptions, QueueStats};
use crate::db::runtime::Db;
use crate::db::DbError;

// ============================================================================
// The wake hook (v4 `ensureProcessorRunning`)
// ============================================================================

type WakeHook = Box<dyn Fn() + Send + Sync>;

/// The process-global wake hook, registered once by the host at startup. `None`
/// until then — an enqueue before the runner exists simply leaves the row PENDING
/// (the runner's first pump claims it), matching v4's `QUILLTAP_JOB_CHILD` no-op.
static WAKE_HOOK: OnceLock<WakeHook> = OnceLock::new();

/// Register the runner wake hook (v4's implicit `ensureProcessorRunning` wiring).
/// Idempotent: only the first registration wins (a `OnceLock`), so repeated host
/// starts don't stack hooks. Typically the host passes a closure that calls
/// [`crate::services::job_runner::JobRunner::wake`].
pub fn set_wake_hook(hook: impl Fn() + Send + Sync + 'static) {
    let _ = WAKE_HOOK.set(Box::new(hook));
}

/// Fire the wake hook if one is registered (v4 `ensureProcessorRunning()` inside
/// `enqueueJob`). No-op when unset. `pub(crate)` so writer-closure enqueues
/// (the mount-index embedding scheduler, P4.6y) can wake the pump after their
/// batch lands.
pub(crate) fn ensure_processor_running() {
    if let Some(hook) = WAKE_HOOK.get() {
        hook();
    }
}

/// v4 `enqueueJob`: mint and persist a PENDING background job. Returns the job
/// id.
pub async fn enqueue_job(
    db: &Db,
    user_id: &str,
    job_type: &str,
    payload: Value,
    max_attempts: f64,
) -> Result<String, DbError> {
    let now = now_iso();
    let id = uuid::Uuid::new_v4().to_string();
    let create = BjCreate {
        user_id: user_id.to_string(),
        job_type: job_type.to_string(),
        status: Some("PENDING".to_string()),
        payload,
        priority: 0.0,
        attempts: 0.0,
        max_attempts,
        last_error: None,
        scheduled_at: now.clone(),
        started_at: None,
        completed_at: None,
    };
    let opts = CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    db.write(move |writers| writers.main().background_jobs().create(&create, &opts))
        .await?;
    // v4 `enqueueJob` calls `ensureProcessorRunning()` after every create — wake
    // the (already-running) runner so it pumps immediately (see [`set_wake_hook`]).
    ensure_processor_running();
    Ok(id)
}

/// v4 `enqueueMemoryHousekeeping`: de-dupe against in-flight
/// `MEMORY_HOUSEKEEPING` jobs for the same (userId, characterId), then enqueue
/// with `maxAttempts: 1` (housekeeping is retry-hostile — the daily scheduler
/// re-enqueues anyway). Returns the existing or new job id.
pub async fn enqueue_memory_housekeeping(
    db: &Db,
    user_id: &str,
    payload: Value,
) -> Result<String, DbError> {
    // De-dupe against in-flight jobs. v4 wraps the check in a try/catch and
    // falls through to enqueue on failure — double work beats none.
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    if let Ok(jobs) = in_flight {
        let our_char = payload.get("characterId").and_then(Value::as_str);
        let existing = jobs.iter().find(|j| {
            if j.job_type != "MEMORY_HOUSEKEEPING" {
                return false;
            }
            let job_char: Option<Value> = serde_json::from_str::<Value>(&j.payload)
                .ok()
                .and_then(|p| p.get("characterId").cloned());
            job_char.as_ref().and_then(Value::as_str) == our_char
        });
        if let Some(existing) = existing {
            return Ok(existing.id.clone());
        }
    }

    enqueue_job(db, user_id, "MEMORY_HOUSEKEEPING", payload, 1.0).await
}

/// v4 `enqueueMemoryRegenerateAll` (P4.6s) — the single fan-out job, deduped on
/// an in-flight `MEMORY_REGENERATE_ALL` for the user (a double-click yields the
/// existing job). Returns `(jobId, isNew)`. `maxAttempts` 1.
pub async fn enqueue_memory_regenerate_all(
    db: &Db,
    user_id: &str,
    payload: Value,
) -> Result<(String, bool), DbError> {
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    if let Ok(jobs) = in_flight {
        if let Some(existing) = jobs.iter().find(|j| j.job_type == "MEMORY_REGENERATE_ALL") {
            return Ok((existing.id.clone(), false));
        }
    }
    let id = enqueue_job(db, user_id, "MEMORY_REGENERATE_ALL", payload, 1.0).await?;
    Ok((id, true))
}

/// v4 `EMBEDDING_ENTITY_PRIORITIES` — real-time entity kinds (MEMORY,
/// CONVERSATION_CHUNK) get higher priority than the batch kinds (HELP_DOC,
/// MOUNT_CHUNK) and any unknown type.
fn embedding_entity_priority(entity_type: &str) -> f64 {
    match entity_type {
        "MEMORY" | "CONVERSATION_CHUNK" => 10.0,
        _ => 0.0,
    }
}

/// v4 `enqueueEmbeddingGenerate` — per-entity-deduped EMBEDDING_GENERATE enqueue.
/// If a PENDING/PROCESSING EMBEDDING_GENERATE job already exists for the payload's
/// `entityId`, returns `(existingId, false)` and enqueues nothing; otherwise
/// enqueues at the entity's priority (`embedding_entity_priority`, maxAttempts 3)
/// and returns `(newId, true)`. The dedup is the hard guarantee against stacked
/// jobs; callers count `isNew`.
pub async fn enqueue_embedding_generate(
    db: &Db,
    user_id: &str,
    payload: Value,
) -> Result<(String, bool), DbError> {
    let entity_id = payload
        .get("entityId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let entity_id_for_read = entity_id.clone();
    let existing = db.read_main(move |conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn)
            .find_pending_for_entity(&entity_id_for_read)
    })?;
    if let Some(job) = existing.iter().find(|j| j.job_type == "EMBEDDING_GENERATE") {
        return Ok((job.id.clone(), false));
    }
    let priority = embedding_entity_priority(
        payload
            .get("entityType")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let id = enqueue_job_with_priority(db, user_id, "EMBEDDING_GENERATE", payload, priority, 3.0)
        .await?;
    Ok((id, true))
}

/// v4 `enqueueJob` variant carrying an explicit `priority` (v4's
/// `options.priority`). Mints and persists a PENDING background job. The
/// zero-priority [`enqueue_job`] above is the common case; the danger /
/// scene-state / render enqueues run at priority `-1` (below interactive tasks).
pub async fn enqueue_job_with_priority(
    db: &Db,
    user_id: &str,
    job_type: &str,
    payload: Value,
    priority: f64,
    max_attempts: f64,
) -> Result<String, DbError> {
    let now = now_iso();
    let id = uuid::Uuid::new_v4().to_string();
    let create = BjCreate {
        user_id: user_id.to_string(),
        job_type: job_type.to_string(),
        status: Some("PENDING".to_string()),
        payload,
        priority,
        attempts: 0.0,
        max_attempts,
        last_error: None,
        scheduled_at: now.clone(),
        started_at: None,
        completed_at: None,
    };
    let opts = CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    db.write(move |writers| writers.main().background_jobs().create(&create, &opts))
        .await?;
    ensure_processor_running();
    Ok(id)
}

/// v4 `enqueueMemoryExtraction` (`lib/background-jobs/queue-service.ts`): enqueue
/// a `MEMORY_EXTRACTION` job for a closed turn, deduping on the
/// `(chatId, turnOpenerMessageId, extractionAnchorMessageId)` triple. If a
/// PENDING or PROCESSING `MEMORY_EXTRACTION` job with a matching triple already
/// exists (across the user's jobs), this is a no-op returning the existing id —
/// so the first character to finalize in a multi-character turn creates the job
/// and later finalizes fold into it. A `null` opener/anchor dedupe on
/// `(chatId, null, null)`. A dedupe-lookup failure falls through and enqueues
/// anyway (v4 warns). `max_attempts` is v4's default 3 (the trigger passes no
/// options). The `skipDedupCheck` option is not modeled (the finalizer never
/// sets it).
///
/// `turn_opener_message_id` / `extraction_anchor_message_id` are v4's payload
/// fields; both `null` when the turn has no user opener and no anchor is set. The
/// payload written is `{ chatId, turnOpenerMessageId, extractionAnchorMessageId,
/// connectionProfileId }` (nulls materialized, matching v4's object literal so
/// the stored payload bytes match).
pub async fn enqueue_memory_extraction(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    turn_opener_message_id: Option<&str>,
    extraction_anchor_message_id: Option<&str>,
    connection_profile_id: &str,
) -> Result<String, DbError> {
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    if let Ok(jobs) = in_flight {
        // v4 treats a missing/undefined anchor as `null` before comparing.
        let incoming_anchor = extraction_anchor_message_id;
        let existing = jobs.iter().find(|j| {
            if j.job_type != "MEMORY_EXTRACTION" {
                return false;
            }
            let payload: Option<Value> = serde_json::from_str::<Value>(&j.payload).ok();
            let Some(p) = payload else {
                return false;
            };
            // `p.get(k)` returns None for an absent key; a stored JSON `null`
            // reads as `Some(Value::Null)`. Both collapse to Rust `None` via
            // `and_then(as_str)`, matching v4's `?? null` normalization.
            let job_chat = p.get("chatId").and_then(Value::as_str);
            let job_opener = p.get("turnOpenerMessageId").and_then(Value::as_str);
            let job_anchor = p.get("extractionAnchorMessageId").and_then(Value::as_str);
            job_chat == Some(chat_id)
                && job_opener == turn_opener_message_id
                && job_anchor == incoming_anchor
        });
        if let Some(existing) = existing {
            return Ok(existing.id.clone());
        }
    }

    // v4's payload object literal materializes every key (nulls included).
    let payload = serde_json::json!({
        "chatId": chat_id,
        "turnOpenerMessageId": turn_opener_message_id,
        "extractionAnchorMessageId": extraction_anchor_message_id,
        "connectionProfileId": connection_profile_id,
    });
    enqueue_job(db, user_id, "MEMORY_EXTRACTION", payload, 3.0).await
}

/// v4 `enqueueCarinaMemoryExtraction` (`lib/background-jobs/queue-service.ts`):
/// enqueue a `CARINA_MEMORY_EXTRACTION` job for a posted Carina reference answer,
/// deduping against in-flight jobs (PENDING/PROCESSING) with a matching
/// `carinaMessageId` (across the user's jobs → no-op returning the existing id).
/// A dedupe-lookup failure falls through and enqueues anyway (v4 warns).
/// `max_attempts` is v4's default 3 (the caller passes no options; `skipDedupCheck`
/// is not modeled — `runCarinaQuery` never sets it).
///
/// The payload is `{ chatId, carinaMessageId, answererId, connectionProfileId }`
/// (v4's object literal — all keys materialized so the stored bytes match).
pub async fn enqueue_carina_memory_extraction(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    carina_message_id: &str,
    answerer_id: &str,
    connection_profile_id: &str,
) -> Result<String, DbError> {
    // Deduped by carinaMessageId across the user's in-flight jobs (the message id
    // is globally unique, so chatId is not part of the key). A lookup failure
    // falls through to enqueue.
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    if let Ok(jobs) = in_flight {
        let existing = jobs.iter().find(|j| {
            if j.job_type != "CARINA_MEMORY_EXTRACTION" {
                return false;
            }
            let payload: Option<Value> = serde_json::from_str::<Value>(&j.payload).ok();
            payload
                .as_ref()
                .and_then(|p| p.get("carinaMessageId"))
                .and_then(Value::as_str)
                == Some(carina_message_id)
        });
        if let Some(existing) = existing {
            return Ok(existing.id.clone());
        }
    }

    let payload = serde_json::json!({
        "chatId": chat_id,
        "carinaMessageId": carina_message_id,
        "answererId": answerer_id,
        "connectionProfileId": connection_profile_id,
    });
    enqueue_job(db, user_id, "CARINA_MEMORY_EXTRACTION", payload, 3.0).await
}

/// v4 `enqueueChatDangerClassification` (`lib/background-jobs/queue-service.ts`):
/// enqueue a `CHAT_DANGER_CLASSIFICATION` job at priority `-1`, deduping via
/// `findPendingForChat` (any PENDING/PROCESSING classification job for the same
/// chat → no-op returning the existing id). `max_attempts` is v4's default 3.
///
/// The payload is `{ chatId, connectionProfileId }` (v4's object literal).
pub async fn enqueue_chat_danger_classification(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    connection_profile_id: &str,
) -> Result<String, DbError> {
    let cid = chat_id.to_string();
    let pending = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        repo.find_pending_for_chat(&cid)
    })?;
    let existing = pending
        .iter()
        .find(|j| j.job_type == "CHAT_DANGER_CLASSIFICATION");
    if let Some(existing) = existing {
        return Ok(existing.id.clone());
    }

    let payload = serde_json::json!({
        "chatId": chat_id,
        "connectionProfileId": connection_profile_id,
    });
    enqueue_job_with_priority(
        db,
        user_id,
        "CHAT_DANGER_CLASSIFICATION",
        payload,
        -1.0,
        3.0,
    )
    .await
}

/// v4 `enqueueTitleUpdate` (`lib/background-jobs/queue-service.ts`): enqueue a
/// `TITLE_UPDATE` job, de-duping on `chatId`. If a PENDING or PROCESSING
/// `TITLE_UPDATE` job already exists for the same `chatId`, this is a no-op that
/// returns the existing job id (multiple finalizer firings at the same
/// interchange checkpoint fold into one pending job). `maxAttempts` defaults to 3
/// (v4 `options?.maxAttempts ?? 3`; the differential passes no options). The
/// `skipDedupCheck` option is not modeled — the context-summary caller never sets
/// it. A dedupe-lookup failure falls through and enqueues anyway (v4 warns).
pub async fn enqueue_title_update(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    payload: Value,
) -> Result<String, DbError> {
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    if let Ok(jobs) = in_flight {
        let existing = jobs.iter().find(|j| {
            if j.job_type != "TITLE_UPDATE" {
                return false;
            }
            let job_chat: Option<Value> = serde_json::from_str::<Value>(&j.payload)
                .ok()
                .and_then(|p| p.get("chatId").cloned());
            job_chat.as_ref().and_then(Value::as_str) == Some(chat_id)
        });
        if let Some(existing) = existing {
            return Ok(existing.id.clone());
        }
    }

    enqueue_job(db, user_id, "TITLE_UPDATE", payload, 3.0).await
}

/// v4 `enqueueCharacterAvatarGeneration` (`lib/background-jobs/queue-service.ts`):
/// enqueue a `CHARACTER_AVATAR_GENERATION` job, deduping via `findPendingForChat`
/// (any PENDING/PROCESSING avatar job for the SAME chat + character → no-op
/// returning the existing id). `max_attempts` is v4's default 3.
///
/// The payload is `{ chatId, characterId, imageProfileId, [equippedSlotsOverride] }`
/// (v4's `CharacterAvatarGenerationPayload` object literal — the override key is
/// present only when set, matching v4's `...(equippedSlotsOverride ? {…} : {})`).
/// Returns `(jobId, is_new)`.
pub async fn enqueue_character_avatar_generation(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    image_profile_id: &str,
    equipped_slots_override: Option<Value>,
) -> Result<(String, bool), DbError> {
    let cid = chat_id.to_string();
    let pending = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid)
    })?;
    let existing = pending.iter().find(|j| {
        if j.job_type != "CHARACTER_AVATAR_GENERATION" {
            return false;
        }
        serde_json::from_str::<Value>(&j.payload)
            .ok()
            .and_then(|p| {
                p.get("characterId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .as_deref()
            == Some(character_id)
    });
    if let Some(existing) = existing {
        return Ok((existing.id.clone(), false));
    }

    let mut payload = serde_json::Map::new();
    payload.insert("chatId".into(), Value::String(chat_id.to_string()));
    payload.insert(
        "characterId".into(),
        Value::String(character_id.to_string()),
    );
    payload.insert(
        "imageProfileId".into(),
        Value::String(image_profile_id.to_string()),
    );
    if let Some(over) = equipped_slots_override {
        payload.insert("equippedSlotsOverride".into(), over);
    }

    let job_id = enqueue_job(
        db,
        user_id,
        "CHARACTER_AVATAR_GENERATION",
        Value::Object(payload),
        3.0,
    )
    .await?;
    Ok((job_id, true))
}

/// v4 `enqueueStoryBackgroundGeneration` (`lib/background-jobs/queue-service.ts`):
/// enqueue a `STORY_BACKGROUND_GENERATION` job, deduping via `findPendingForChat`
/// **chat-level only** (any PENDING/PROCESSING story-background job for the same
/// chat → no-op returning the existing id; NOT per-character, unlike the avatar
/// enqueue). Priority `-1` (below interactive), `max_attempts` the default 3.
///
/// The payload is `{ chatId, imageProfileId, characterIds, [sceneContext],
/// projectId }` (v4's `StoryBackgroundGenerationPayload`). Returns
/// `(jobId, is_new)`.
///
/// **`projectId` is ALWAYS written, `null` when the chat has no project.** The
/// type marks it optional, but BOTH v4 call sites — `handleRegenerateBackground`
/// and `queueStoryBackgroundIfEnabled` — build the payload literal with
/// `projectId: chat.projectId ?? null`, so the key is never absent in practice.
/// `project_id: None` here means exactly that explicit `null` (P4.6ao: the port
/// previously OMITTED the key, which `cost_background_routes_equivalence`'s job-row
/// diff caught). `sceneContext` keeps the insert-if-present shape — it too is
/// passed by both call sites, so a `None` would mean a caller that genuinely
/// omitted it.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_story_background_generation(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    image_profile_id: &str,
    character_ids: &[String],
    scene_context: Option<&str>,
    project_id: Option<String>,
) -> Result<(String, bool), DbError> {
    let cid = chat_id.to_string();
    let pending = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid)
    })?;
    // Chat-level dedupe: any pending/processing story-background job for this chat.
    if let Some(existing) = pending
        .iter()
        .find(|j| j.job_type == "STORY_BACKGROUND_GENERATION")
    {
        return Ok((existing.id.clone(), false));
    }

    let mut payload = serde_json::Map::new();
    payload.insert("chatId".into(), Value::String(chat_id.to_string()));
    payload.insert(
        "imageProfileId".into(),
        Value::String(image_profile_id.to_string()),
    );
    payload.insert(
        "characterIds".into(),
        Value::Array(
            character_ids
                .iter()
                .map(|c| Value::String(c.clone()))
                .collect(),
        ),
    );
    if let Some(scene) = scene_context {
        payload.insert("sceneContext".into(), Value::String(scene.to_string()));
    }
    // `projectId: chat.projectId ?? null` — always present; see the fn docs.
    payload.insert(
        "projectId".into(),
        match project_id {
            Some(pid) => Value::String(pid),
            None => Value::Null,
        },
    );

    let job_id = enqueue_job_with_priority(
        db,
        user_id,
        "STORY_BACKGROUND_GENERATION",
        Value::Object(payload),
        -1.0,
        3.0,
    )
    .await?;
    Ok((job_id, true))
}

/// v4 `enqueueWardrobeOutfitAnnouncement` (`lib/background-jobs/queue-service.ts`):
/// enqueue a debounced `WARDROBE_OUTFIT_ANNOUNCEMENT` job (`scheduledAt = now`,
/// `priority = -1`, `maxAttempts` = the default 3). Collapses against a still-
/// **PENDING** job for the same chat + character (v4 deliberately does NOT collapse
/// against a PROCESSING job — it has already snapshotted the equipped state and
/// can't absorb fresh changes, so those edits get a fresh debounced job). Returns
/// `(jobId, is_new)`.
///
/// The payload is `{ chatId, characterId }` (v4's `WardrobeOutfitAnnouncementPayload`
/// object literal).
pub async fn enqueue_wardrobe_outfit_announcement(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
) -> Result<(String, bool), DbError> {
    let cid = chat_id.to_string();
    let pending = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid)
    })?;
    // v4: `job.type === 'WARDROBE_OUTFIT_ANNOUNCEMENT' && job.status === 'PENDING'
    //      && payload.characterId === characterId`. `find_pending_for_chat` returns
    // PENDING+PROCESSING, so the explicit PENDING check matters.
    let existing = pending.iter().find(|j| {
        if j.job_type != "WARDROBE_OUTFIT_ANNOUNCEMENT" || j.status != "PENDING" {
            return false;
        }
        serde_json::from_str::<Value>(&j.payload)
            .ok()
            .and_then(|p| {
                p.get("characterId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .as_deref()
            == Some(character_id)
    });
    if let Some(existing) = existing {
        return Ok((existing.id.clone(), false));
    }

    let payload = serde_json::json!({ "chatId": chat_id, "characterId": character_id });
    let job_id = enqueue_job_with_priority(
        db,
        user_id,
        "WARDROBE_OUTFIT_ANNOUNCEMENT",
        payload,
        -1.0,
        3.0,
    )
    .await?;
    Ok((job_id, true))
}

/// v4 `enqueueAutonomousRoomTurn` (`lib/background-jobs/queue-service.ts`):
/// enqueue an `AUTONOMOUS_ROOM_TURN` job (4.6 Private Character Rooms).
///
/// **No dedupe** — multiple in-flight turn jobs for the same chat is exactly the
/// failure mode the handler's stale-run guard (`payload.runId` vs
/// `chats.currentRunId`) catches. `maxAttempts: 1` because the per-room procedure
/// classifies fatal vs non-fatal itself and decides whether to re-enqueue; the
/// runner's automatic retry would muddle that lifecycle (the failure path is the
/// `reconcile_failed_autonomous_turn` reconciliation instead). Default priority
/// (0), immediate `scheduledAt` — no v4 caller ever passes options.
///
/// The payload is v4's `AutonomousRoomTurnPayload` object literal
/// `{ chatId, runId }` (both keys materialized so the stored bytes match).
pub async fn enqueue_autonomous_room_turn(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    run_id: &str,
) -> Result<String, DbError> {
    let payload = serde_json::json!({ "chatId": chat_id, "runId": run_id });
    enqueue_job(db, user_id, "AUTONOMOUS_ROOM_TURN", payload, 1.0).await
}

/// v4 `enqueueAutonomousRoomScheduleTick` (`lib/background-jobs/queue-service.ts`):
/// enqueue the autonomous-room scheduler tick. Dedup-aware: if an unprocessed
/// (**PENDING**-only — v4 queries `findByUserId(userId, 'PENDING')`, deliberately
/// NOT PROCESSING) tick already exists for this user, return its id instead of
/// stacking duplicate scans. Priority `-1` (below interactive), `maxAttempts: 1`,
/// empty `{}` payload. The dedupe lookup is NOT error-swallowed (v4 has no
/// try/catch here — a read failure propagates).
pub async fn enqueue_autonomous_room_schedule_tick(
    db: &Db,
    user_id: &str,
) -> Result<String, DbError> {
    let uid = user_id.to_string();
    let pending = db.read_main(move |conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn)
            .find_by_user_id(&uid, Some("PENDING"))
    })?;
    if let Some(existing) = pending
        .iter()
        .find(|j| j.job_type == "AUTONOMOUS_ROOM_SCHEDULE_TICK")
    {
        return Ok(existing.id.clone());
    }
    enqueue_job_with_priority(
        db,
        user_id,
        "AUTONOMOUS_ROOM_SCHEDULE_TICK",
        serde_json::json!({}),
        -1.0,
        1.0,
    )
    .await
}

/// v4 `enqueueEmbeddingReindexAll`: enqueue an `EMBEDDING_REINDEX_ALL` job at
/// `priority = -1` (below interactive), `maxAttempts` = the default 3. The
/// `EMBEDDING_REFIT` handler fires this after a successful vocabulary refit. The
/// payload is v4's `{ profileId }` (the `scope` field defaults to `'all'` and is
/// omitted, matching the refit call site's object literal). The REINDEX handler
/// itself is not ported (it stays on the runner's loud fallback).
pub async fn enqueue_embedding_reindex_all(
    db: &Db,
    user_id: &str,
    profile_id: &str,
) -> Result<String, DbError> {
    let payload = serde_json::json!({ "profileId": profile_id });
    enqueue_job_with_priority(db, user_id, "EMBEDDING_REINDEX_ALL", payload, -1.0, 3.0).await
}

// ============================================================================
// Retention windows (v4 `lib/background-jobs/maintenance/retention-constants.ts`)
// ============================================================================

/// v4 `COMPLETED_JOB_RETENTION_DAYS` — reap COMPLETED jobs this many days after
/// `completedAt`.
pub const COMPLETED_JOB_RETENTION_DAYS: i64 = 7;

/// v4 `DEAD_JOB_RETENTION_DAYS` — reap DEAD (retry-exhausted) jobs this many days
/// after `completedAt` (longer than COMPLETED: a DEAD row is the only forensic
/// trail).
pub const DEAD_JOB_RETENTION_DAYS: i64 = 30;

/// v4 `STALE_CHAT_RETENTION_DAYS` — a chat is "stale" after this many days without
/// played activity; only then are its superseded generated assets collapsed.
pub const STALE_CHAT_RETENTION_DAYS: i64 = 30;

/// v4 `CLOSED_TERMINAL_RETENTION_DAYS`.
pub const CLOSED_TERMINAL_RETENTION_DAYS: i64 = 30;

/// v4 `DAY_MS`.
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// v4 `retentionCutoff(days, now)` — the absolute cutoff instant (ISO 8601) `days`
/// before `now_ms`.
pub fn retention_cutoff_iso(days: i64, now_ms: i64) -> String {
    crate::clock::iso_from_unix_ms(now_ms - days * DAY_MS)
}

/// v4 `resolveStaleChatDays()` — the effective stale-chat window: the
/// user-configured `dataRetention.staleChatDays` instance setting, falling back
/// to [`STALE_CHAT_RETENTION_DAYS`] when unset or unreadable. Every stale-gated
/// sweep (image collapse, cache collapse, chunk cold-tier) computes its cutoff
/// from this one value so they always agree on "stale". A read error resolves to
/// the fallback (v4's `try/catch → default`).
pub fn resolve_stale_chat_days(db: &Db) -> i64 {
    db.read_main(crate::db::instance_settings::get_data_retention_settings)
        .unwrap_or(STALE_CHAT_RETENTION_DAYS)
}

// ============================================================================
// Read / admin surface (v4 queue-service read helpers)
// ============================================================================

/// v4 `getJobStatus(jobId)` — the full job row, or `None`.
pub async fn get_job_status(
    db: &Db,
    job_id: &str,
) -> Result<Option<crate::db::background_jobs::BackgroundJob>, DbError> {
    let id = job_id.to_string();
    db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).find_by_id(&id)
    })
}

/// v4 `getQueueStats(userId?)` — per-status counts, optionally user-scoped.
pub async fn get_queue_stats(db: &Db, user_id: Option<&str>) -> Result<QueueStats, DbError> {
    let uid = user_id.map(str::to_string);
    db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).get_stats(uid.as_deref())
    })
}

/// v4 `getActiveCountsByType(userId?)` — active (PENDING+PROCESSING) counts grouped
/// by type. Returned as a `(type, count)` list (v4 returns a `Record`; the caller
/// builds the map).
pub async fn get_active_counts_by_type(
    db: &Db,
    user_id: Option<&str>,
) -> Result<Vec<(String, i64)>, DbError> {
    let uid = user_id.map(str::to_string);
    db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn)
            .get_active_counts_by_type(uid.as_deref())
    })
}

/// v4 `cancelJob(jobId)` — cancel a PENDING|FAILED job (→ DEAD). Returns whether a
/// row was cancelled.
pub async fn cancel_job(db: &Db, job_id: &str) -> Result<bool, DbError> {
    let id = job_id.to_string();
    db.write(move |writers| {
        crate::db::background_jobs::BackgroundJobsRepository::new(writers.main().connection())
            .cancel(&id)
    })
    .await
}

/// v4 `getPendingJobsForChat(chatId)` — PENDING+PROCESSING jobs whose payload
/// `chatId` matches.
pub async fn get_pending_jobs_for_chat(
    db: &Db,
    chat_id: &str,
) -> Result<Vec<crate::db::background_jobs::BackgroundJob>, DbError> {
    let cid = chat_id.to_string();
    db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid)
    })
}

/// v4 `cleanupOldJobs(daysOld)` (deprecated single-window reaper): delete
/// COMPLETED|DEAD jobs older than `days_old` days by `completedAt`. Returns the
/// count deleted. Prefer [`cleanup_finished_jobs`] (per-status windows).
pub async fn cleanup_old_jobs(db: &Db, days_old: i64, now_ms: i64) -> Result<usize, DbError> {
    let cutoff = retention_cutoff_iso(days_old, now_ms);
    db.write(move |writers| {
        crate::db::background_jobs::BackgroundJobsRepository::new(writers.main().connection())
            .cleanup_old_jobs(&cutoff)
    })
    .await
}

/// v4 `cleanupFinishedJobs()` — the finished-job reaper: COMPLETED before the short
/// window, DEAD before the longer one (both keyed on `completedAt`). Called by the
/// daily maintenance tick. Returns `(completed, dead)` counts.
pub async fn cleanup_finished_jobs(db: &Db, now_ms: i64) -> Result<(usize, usize), DbError> {
    let completed_cutoff = retention_cutoff_iso(COMPLETED_JOB_RETENTION_DAYS, now_ms);
    let dead_cutoff = retention_cutoff_iso(DEAD_JOB_RETENTION_DAYS, now_ms);
    db.write(move |writers| {
        crate::db::background_jobs::BackgroundJobsRepository::new(writers.main().connection())
            .cleanup_old_jobs_by_status(&completed_cutoff, &dead_cutoff)
    })
    .await
}

// ============================================================================
// Scheduler sweep bodies (v4 `scheduled-*.ts` — the portable enqueuer loops)
// ============================================================================

/// v4 `runScheduledHousekeeping()` — for every user whose
/// `autoHousekeepingSettings.enabled` is true, enqueue one `MEMORY_HOUSEKEEPING`
/// job (`{ reason: 'scheduled' }`). The enqueue helper dedupes against in-flight
/// jobs for the same (userId, characterId). Returns `(usersProcessed,
/// jobsEnqueued)` (equal here — one enqueue per enabled user, a per-user enqueue
/// failure swallowed and counted for neither). The host driver calls this on the
/// daily cadence.
pub async fn run_scheduled_housekeeping(db: &Db) -> Result<(usize, usize), DbError> {
    let settings = db.read_main(crate::db::chat_settings::find_all_scheduler_settings)?;
    let mut users_processed = 0usize;
    let mut jobs_enqueued = 0usize;
    for s in &settings {
        let enabled = s
            .auto_housekeeping_settings
            .as_ref()
            .and_then(|v| v.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !enabled {
            continue;
        }
        // v4 passes `{ reason: 'scheduled' }`; the payload otherwise defaults.
        let payload = serde_json::json!({ "reason": "scheduled" });
        match enqueue_memory_housekeeping(db, &s.user_id, payload).await {
            Ok(_) => {
                jobs_enqueued += 1;
                users_processed += 1;
            }
            Err(_) => { /* v4 warns + continues */ }
        }
    }
    Ok((users_processed, jobs_enqueued))
}

/// v4 `runScheduledCleanup()` — for every user whose
/// `llmLoggingSettings.enabled` is true AND `retentionDays > 0`, enqueue one
/// `LLM_LOG_CLEANUP` job (`{ userId, retentionDays }`). Returns `(usersProcessed,
/// jobsEnqueued)`. The host driver calls this on the daily cadence (v4 runs it
/// immediately on startup — no grace).
pub async fn run_scheduled_cleanup(db: &Db) -> Result<(usize, usize), DbError> {
    let settings = db.read_main(crate::db::chat_settings::find_all_scheduler_settings)?;
    let mut users_processed = 0usize;
    let mut jobs_enqueued = 0usize;
    for s in &settings {
        let Some(llm) = &s.llm_logging_settings else {
            continue;
        };
        let enabled = llm.get("enabled").and_then(Value::as_bool).unwrap_or(false);
        let retention_days = llm
            .get("retentionDays")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if !enabled || retention_days <= 0.0 {
            continue;
        }
        // v4's payload: `{ userId, retentionDays }` (retentionDays materialized).
        let payload = serde_json::json!({
            "userId": s.user_id,
            "retentionDays": retention_days,
        });
        match enqueue_job(db, &s.user_id, "LLM_LOG_CLEANUP", payload, 3.0).await {
            Ok(_) => {
                jobs_enqueued += 1;
                users_processed += 1;
            }
            Err(_) => { /* v4 warns + continues */ }
        }
    }
    Ok((users_processed, jobs_enqueued))
}

// ============================================================================
// P4.1d append-only additions (the danger-scan enqueuer's helpers)
// ============================================================================

/// v4 `enqueueContextSummary` (`lib/background-jobs/queue-service.ts:778`): a
/// plain `enqueueJob` of type `CONTEXT_SUMMARY` — NO dedupe (unlike the title /
/// classification enqueues). The payload is v4's `ContextSummaryPayload` object
/// literal in declaration order: `{ chatId, connectionProfileId,
/// forceRegenerate }` (the danger-scan caller always materializes
/// `forceRegenerate: false`). `priority` is the caller's `options.priority`
/// (the danger scan passes `-2`); `maxAttempts` is the default 3.
pub async fn enqueue_context_summary(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    connection_profile_id: &str,
    force_regenerate: bool,
    priority: f64,
) -> Result<String, DbError> {
    let payload = serde_json::json!({
        "chatId": chat_id,
        "connectionProfileId": connection_profile_id,
        "forceRegenerate": force_regenerate,
    });
    enqueue_job_with_priority(db, user_id, "CONTEXT_SUMMARY", payload, priority, 3.0).await
}

/// [`enqueue_chat_danger_classification`] with an explicit priority (v4's
/// `options.priority ?? -1` passthrough — the scheduled danger scan enqueues at
/// `-2`, below even the finalizer's `-1` classification jobs). Same dedupe:
/// any PENDING/PROCESSING `CHAT_DANGER_CLASSIFICATION` job for the chat → no-op
/// returning the existing id. The payload is `{ chatId, connectionProfileId }`.
pub async fn enqueue_chat_danger_classification_with_priority(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    connection_profile_id: &str,
    priority: f64,
) -> Result<String, DbError> {
    let cid = chat_id.to_string();
    let pending = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid)
    })?;
    if let Some(existing) = pending
        .iter()
        .find(|j| j.job_type == "CHAT_DANGER_CLASSIFICATION")
    {
        return Ok(existing.id.clone());
    }

    let payload = serde_json::json!({
        "chatId": chat_id,
        "connectionProfileId": connection_profile_id,
    });
    enqueue_job_with_priority(
        db,
        user_id,
        "CHAT_DANGER_CLASSIFICATION",
        payload,
        priority,
        3.0,
    )
    .await
}

/// v4 `enqueueConversationRender` (`lib/background-jobs/queue-service.ts:399`):
/// enqueue a `CONVERSATION_RENDER` job, deduping via `findPendingForChat` (any
/// PENDING/PROCESSING render job for the SAME chat → no-op returning the existing
/// id with `is_new = false`). Priority is v4's `options?.priority ?? -1` — "lower
/// priority than interactive tasks"; `max_attempts` is `enqueueJob`'s default 3.
///
/// The payload is `{ chatId, fullReembed }` (v4's `ConversationRenderPayload`
/// object literal — the manual `render-conversation` entrance always passes
/// `fullReembed: true`). Returns `(job_id, is_new)`, v4's
/// `ConversationRenderEnqueueResult`.
pub async fn enqueue_conversation_render(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    full_reembed: bool,
) -> Result<(String, bool), DbError> {
    let cid = chat_id.to_string();
    let pending = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid)
    })?;
    if let Some(existing) = pending.iter().find(|j| j.job_type == "CONVERSATION_RENDER") {
        return Ok((existing.id.clone(), false));
    }

    let payload = serde_json::json!({
        "chatId": chat_id,
        "fullReembed": full_reembed,
    });
    let id =
        enqueue_job_with_priority(db, user_id, "CONVERSATION_RENDER", payload, -1.0, 3.0).await?;
    Ok((id, true))
}
