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
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;
use crate::services::activity_kinds::{ActivityCounts, ACTIVITY_KINDS};
use crate::services::activity_registry::{activity_counts, activity_start_totals};

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

/// v4 `ensureProcessorRunning()` as a handler-visible entry point — the
/// `EMBEDDING_REINDEX_ALL` handler calls it directly after its batch insert
/// (v4 `embedding-reindex.ts:258`), since `create_batch` bypasses the enqueue
/// helpers that would otherwise fire it.
pub fn wake_processor() {
    ensure_processor_running();
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
    enqueue_job_at(db, user_id, job_type, payload, max_attempts, None).await
}

/// P4.D271 (R-G): [`enqueue_job`] with v4's `options.scheduledAt` —
/// `scheduledAt: options?.scheduledAt?.toISOString() ?? now`
/// (`queue-service.ts:490`). `None` stamps now.
pub async fn enqueue_job_at(
    db: &Db,
    user_id: &str,
    job_type: &str,
    payload: Value,
    max_attempts: f64,
    scheduled_at: Option<String>,
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
        scheduled_at: scheduled_at.unwrap_or_else(|| now.clone()),
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
    // Tell every open client the queue moved (v4 `queue-service.ts:455`,
    // `f3892158d`). The bus coalesces, so a batch of enqueues arrives as one
    // hint rather than one per job. ⚠ v4 has ONE `enqueueJob`; v5 split it into
    // this and `enqueue_job_with_priority`, so v4's single publish site is TWO
    // here — both are the same v4 line.
    publish_realtime(RealtimeTopic::Jobs, None);
    // v4 `enqueueJob` calls `ensureProcessorRunning()` after every create — wake
    // the (already-running) runner so it pumps immediately (see [`set_wake_hook`]).
    ensure_processor_running();
    Ok(id)
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

/// v4 `enqueueRegenerateConversationSummaries` (queue-service.ts:742, P4.43) —
/// the summaries re-mirror backfill, deduped on an in-flight
/// `REGENERATE_CONVERSATION_SUMMARIES` for the user (only one regeneration per
/// user at a time). Returns `(jobId, isNew)`. `maxAttempts: 1` — the work is
/// idempotent, but a retry would re-walk every chat. The dedup-check failure is
/// non-fatal (v4 warns and enqueues anyway).
pub async fn enqueue_regenerate_conversation_summaries(
    db: &Db,
    user_id: &str,
) -> Result<(String, bool), DbError> {
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    if let Ok(jobs) = in_flight {
        if let Some(existing) = jobs
            .iter()
            .find(|j| j.job_type == "REGENERATE_CONVERSATION_SUMMARIES")
        {
            return Ok((existing.id.clone(), false));
        }
    }
    let id = enqueue_job(
        db,
        user_id,
        "REGENERATE_CONVERSATION_SUMMARIES",
        Value::Object(serde_json::Map::new()),
        1.0,
    )
    .await?;
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

// === P4.D288 (C1-3): the batch embedding enqueue ===

/// v4 `enqueueEmbeddingGenerateMany(userId, payloads)` (`queue-service.ts:
/// 1132-1180`): ONE read of the active `EMBEDDING_GENERATE` jobs
/// ([`BackgroundJobsRepository::find_active_by_type`](crate::db::background_jobs::BackgroundJobsRepository::find_active_by_type)),
/// every payload whose `entityId` already has a job — or repeats one earlier in
/// the batch — skipped, then ONE batch insert (each at its entity type's
/// priority, `maxAttempts` 3, `scheduledAt` the shared `now`). Nothing fresh →
/// DEBUG `Embedding batch: every entity already has a job {requested}` and
/// `[]`; else INFO `Embedding generate batch enqueued {requested, queued,
/// alreadyQueued}` and — only when something queued — the `jobs` realtime
/// publish + the processor wake. Answers the created ids (its length is the
/// number actually queued). The two lines' bytes are P4.D292's pin (§R.5).
pub async fn enqueue_embedding_generate_many(
    db: &Db,
    user_id: &str,
    payloads: &[Value],
) -> Result<Vec<String>, DbError> {
    if payloads.is_empty() {
        return Ok(Vec::new());
    }
    let active = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn)
            .find_active_by_type("EMBEDDING_GENERATE")
    })?;
    let busy: std::collections::HashSet<String> = active
        .iter()
        .filter_map(|job| {
            serde_json::from_str::<Value>(&job.payload)
                .ok()?
                .get("entityId")?
                .as_str()
                .map(str::to_string)
        })
        .collect();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let fresh: Vec<&Value> = payloads
        .iter()
        .filter(|p| {
            // v4 `p.entityId` — a non-string id is neither busy nor seen-able
            // as a string; it is kept (and its `undefined` dedupes against
            // itself, as `Set.has(undefined)` does).
            let key = p
                .get("entityId")
                .map(|v| {
                    v.as_str()
                        .map_or_else(|| format!("\u{0}{v}"), str::to_string)
                })
                .unwrap_or_else(|| "\u{0}undefined".to_string());
            if busy.contains(&key) || seen.contains(&key) {
                return false;
            }
            seen.insert(key);
            true
        })
        .collect();
    if fresh.is_empty() {
        tracing::debug!(
            requested = payloads.len(),
            "Embedding batch: every entity already has a job"
        );
        return Ok(Vec::new());
    }
    let now = now_iso();
    let jobs: Vec<BjCreate> = fresh
        .iter()
        .map(|payload| BjCreate {
            user_id: user_id.to_string(),
            job_type: "EMBEDDING_GENERATE".to_string(),
            status: Some("PENDING".to_string()),
            payload: (*payload).clone(),
            priority: embedding_entity_priority(
                payload
                    .get("entityType")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            attempts: 0.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: now.clone(),
            started_at: None,
            completed_at: None,
        })
        .collect();
    let ids: Vec<String> = (0..jobs.len())
        .map(|_| uuid::Uuid::new_v4().to_string())
        .collect();
    let job_ids = db
        .write(move |writers| {
            writers
                .main()
                .background_jobs()
                .create_batch(&jobs, &ids, &now)
        })
        .await?;
    tracing::info!(
        requested = payloads.len(),
        queued = job_ids.len(),
        alreadyQueued = payloads.len() - fresh.len(),
        "Embedding generate batch enqueued"
    );
    if !job_ids.is_empty() {
        publish_realtime(RealtimeTopic::Jobs, None);
        ensure_processor_running();
    }
    Ok(job_ids)
}

// === end P4.D288 ===

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
    // The second half of v4's one `enqueueJob` publish site — see [`enqueue_job`].
    publish_realtime(RealtimeTopic::Jobs, None);
    ensure_processor_running();
    Ok(id)
}

// === P4.D288 (C1-6) — the job vocabulary the `83334b9a5` round shares (v4
// `104e986c7` / `ac6348c5f` / `64743ee13`: `lib/schemas/job.types.ts:20-43`,
// `lib/background-jobs/queue-service.ts:55-121,329-345`,
// `lib/memory/maintenance/types.ts:15-40`). The ENQUEUES are each owner's
// fenced region; only the types live here (append-only) ===

/// v4 `MEMORY_MAINTENANCE` — the ONE memory maintenance job, in the slot the
/// retired `MEMORY_HOUSEKEEPING` / `MEMORY_CONSOLIDATION` held. Registered by
/// P4.D297; until then a claimed row fails with the runner's "recognized but
/// not yet available" sentence (O-12).
pub const MEMORY_MAINTENANCE: &str = "MEMORY_MAINTENANCE";

/// v4 `MAINTENANCE_PHASES` (`maintenance/types.ts:15`) — the phases, in the
/// order they always run; each skippable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaintenancePhase {
    FoldNearDuplicates,
    Consolidate,
    Demote,
    Retire,
    Mirror,
}

impl MaintenancePhase {
    /// v4's order.
    pub const ALL: [Self; 5] = [
        Self::FoldNearDuplicates,
        Self::Consolidate,
        Self::Demote,
        Self::Retire,
        Self::Mirror,
    ];

    /// The kebab wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FoldNearDuplicates => "fold-near-duplicates",
            Self::Consolidate => "consolidate",
            Self::Demote => "demote",
            Self::Retire => "retire",
            Self::Mirror => "mirror",
        }
    }

    /// The enum's exact parse.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

/// v4 `MaintenanceReason` (`maintenance/types.ts:31`) — why a run was asked
/// for (the ledger and the logs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaintenanceReason {
    Scheduled,
    Watermark,
    Manual,
}

impl MaintenanceReason {
    pub const ALL: [Self; 3] = [Self::Scheduled, Self::Watermark, Self::Manual];

    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Watermark => "watermark",
            Self::Manual => "manual",
        }
    }

    /// The exact parse.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

/// v4 `MemoryExtractionPass` (`queue-service.ts:113`) — the grain a
/// `MEMORY_EXTRACTION` job runs at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryExtractionPass {
    Turn,
    OtherFold,
    EpisodeFold,
}

impl MemoryExtractionPass {
    pub const ALL: [Self; 3] = [Self::Turn, Self::OtherFold, Self::EpisodeFold];

    /// The wire string (also the cursor pass's — `turn` / `other-fold` /
    /// `episode-fold`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::OtherFold => "other-fold",
            Self::EpisodeFold => "episode-fold",
        }
    }

    /// The exact parse.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

/// v4 `memoryExtractionPassOf(payload)` (`queue-service.ts:116-121`) — a
/// payload's grain, reading the pre-Phase-D flags of rows still in the queue:
/// `payload.pass` when TRUTHY (JS truthiness), else `episode-fold` for a truthy
/// `foldEpisodeCatchup`, else `other-fold` for a truthy `foldOtherCatchup`,
/// else `turn`.
///
/// R-X: v4 returns a truthy `pass` it does not know AS IS (its type is erased
/// at run time) — `Err` carries that raw value, exactly as v4 holds it (a
/// string, or any other truthy JSON value); the extraction lane's `PassOf`
/// wraps it (it lands on the episode branch, as v4's does).
pub fn pass_of(payload: &Value) -> Result<MemoryExtractionPass, Value> {
    let get = |k: &str| payload.get(k);
    if crate::api::system_qtap::js_truthy(get("pass")) {
        let raw = get("pass").expect("truthy");
        return raw
            .as_str()
            .and_then(MemoryExtractionPass::parse)
            .ok_or_else(|| raw.clone());
    }
    if crate::api::system_qtap::js_truthy(get("foldEpisodeCatchup")) {
        return Ok(MemoryExtractionPass::EpisodeFold);
    }
    if crate::api::system_qtap::js_truthy(get("foldOtherCatchup")) {
        return Ok(MemoryExtractionPass::OtherFold);
    }
    Ok(MemoryExtractionPass::Turn)
}

// === end P4.D288 ===

// === P4.D297: memory maintenance enqueue ===
// v4 `lib/background-jobs/queue-service.ts:325-343, :635-785` (`104e986c7`, at
// the `83334b9a5` pin). The payload is a preserve-order `serde_json::Map`
// throughout — its BYTES are a comparand (the widened payload keeps the
// prior's key positions and appends new keys, as v4's object spreads do):
// API `{"reason":"manual"[,"phases"][,"settings"],"characterId","exemptPhases"}`,
// scheduler `{"reason":"scheduled","characterId","exemptPhases":[]}`, watermark
// `{"characterId","reason":"watermark","phases","exemptPhases":[]}`. v4's
// `MemoryMaintenancePayload` has no `dryRun` — a dry run never becomes a job.

/// v4 `unionPhases(a, b)`: "union two phase lists; `undefined` (every phase)
/// absorbs anything" — first-seen order, deduped. A non-array is `undefined`.
fn union_phases(a: Option<&Value>, b: Option<&Value>) -> Option<Vec<Value>> {
    let (Some(Value::Array(a)), Some(Value::Array(b))) = (a, b) else {
        return None;
    };
    let mut out: Vec<Value> = Vec::new();
    for p in a.iter().chain(b.iter()) {
        if !out.contains(p) {
            out.push(p.clone());
        }
    }
    Some(out)
}

/// v4 `exemptPhasesOf(payload)`: "the phases a request asks for by hand: they
/// run whatever the enabled switches and per-phase toggles say. A manual
/// request exempts what it names (every phase when it names none); an
/// automatic one exempts nothing." — `exemptPhases` when JS-truthy (an array,
/// even empty), else `[]` unless `reason === 'manual'`, else `phases ?? ALL`.
pub fn exempt_phases_of(payload: &serde_json::Map<String, Value>) -> Value {
    match payload.get("exemptPhases") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => {}
        Some(Value::String(s)) if s.is_empty() => {}
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => {}
        Some(v) => return v.clone(),
    }
    if payload.get("reason").and_then(Value::as_str) != Some("manual") {
        return Value::Array(Vec::new());
    }
    match payload.get("phases") {
        None | Some(Value::Null) => Value::Array(
            MaintenancePhase::ALL
                .into_iter()
                .map(|p| Value::String(p.as_str().to_string()))
                .collect(),
        ),
        Some(v) => v.clone(),
    }
}

fn phase_values(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        _ => Vec::new(),
    }
}

/// v4 `widenPayload(prior, payload)`: "fold `payload` into a waiting job's
/// payload. Each phase keeps its own exemption." `{...prior, [phases: union],
/// reason: payload.reason === 'manual' ? 'manual' : prior.reason,
/// exemptPhases: [...new Set([...exemptPhasesOf(prior),
/// ...exemptPhasesOf(payload)])], [settings: {...prior.settings,
/// ...payload.settings}]}`, then `delete phases` when the union is
/// `undefined`. "For the ledger and the logs only: what may skip the switches
/// is `exemptPhases`."
pub fn widen_maintenance_payload(
    prior: &serde_json::Map<String, Value>,
    payload: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let phases = union_phases(prior.get("phases"), payload.get("phases"));
    let mut exempt: Vec<Value> = Vec::new();
    for p in phase_values(&exempt_phases_of(prior))
        .into_iter()
        .chain(phase_values(&exempt_phases_of(payload)))
    {
        if !exempt.contains(&p) {
            exempt.push(p);
        }
    }
    let mut widened = prior.clone();
    if let Some(phases) = &phases {
        widened.insert("phases".into(), Value::Array(phases.clone()));
    }
    let reason = if payload.get("reason").and_then(Value::as_str) == Some("manual") {
        Some(Value::String("manual".to_string()))
    } else {
        prior.get("reason").cloned()
    };
    match reason {
        Some(r) => {
            widened.insert("reason".into(), r);
        }
        // `reason: undefined` — `JSON.stringify` drops the key.
        None => {
            widened.shift_remove("reason");
        }
    }
    widened.insert("exemptPhases".into(), Value::Array(exempt));
    if let Some(settings) = payload.get("settings").filter(|s| js_truthy_value(s)) {
        let mut merged = match prior.get("settings") {
            Some(Value::Object(o)) => o.clone(),
            _ => serde_json::Map::new(),
        };
        if let Value::Object(o) = settings {
            for (k, v) in o {
                merged.insert(k.clone(), v.clone());
            }
        }
        widened.insert("settings".into(), Value::Object(merged));
    }
    if phases.is_none() {
        widened.shift_remove("phases");
    }
    widened
}

fn js_truthy_value(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// One of the user's PENDING maintenance jobs, as the enqueue tracks it.
#[derive(Clone, Debug)]
pub struct PendingMaintenance {
    pub id: String,
    pub payload: serde_json::Map<String, Value>,
}

fn character_of(payload: &serde_json::Map<String, Value>) -> Option<&Value> {
    payload.get("characterId")
}

/// v4 `findPendingMaintenance(userId, characterId?)`: "the user's PENDING
/// maintenance jobs, or null when they cannot be read". v4's `findByUserId`
/// is a fallback `safeQuery` over `findByFilter` — a failed read logs `Error
/// finding entities by filter` and answers `[]` — so the helper's own catch
/// (WARN `[Maintenance] Failed to check for pending jobs during enqueue
/// {characterId, error}`, then `null`) is unreachable through v4's real code:
/// pinned NEGATIVE, its v5 home the fallback below. "A duplicate run is
/// wasteful, not harmful."
fn find_pending_maintenance(db: &Db, user_id: &str) -> Option<Vec<PendingMaintenance>> {
    let uid = user_id.to_string();
    let jobs = db
        .read_main(|conn| {
            Ok(crate::db::fallback::find_by_filter_or_empty(
                "background_jobs",
                || {
                    crate::db::background_jobs::BackgroundJobsRepository::new(conn)
                        .find_by_user_id(&uid, Some("PENDING"))
                },
            ))
        })
        .unwrap_or_default();
    Some(
        jobs.into_iter()
            .filter(|j| j.job_type == MEMORY_MAINTENANCE)
            .map(|j| PendingMaintenance {
                id: j.id,
                payload: match serde_json::from_str::<Value>(&j.payload) {
                    Ok(Value::Object(m)) => m,
                    _ => serde_json::Map::new(),
                },
            })
            .collect(),
    )
}

/// v4 `enqueueMaintenanceAgainst(userId, payload, pending, options)`:
/// "enqueue (or widen) one character's maintenance job against an
/// already-read list of the user's PENDING maintenance jobs, which it keeps up
/// to date." A PENDING job for the same `characterId` absorbs the request
/// (its payload widened in place; DEBUG `[Maintenance] Pending job widened to
/// the requested phases {characterId, existingJobId, phases: widened.phases ??
/// 'all', exemptPhases, reason}`, root logger); else a new job with
/// `maxAttempts` 1 (`options.scheduledAt` passed through).
async fn enqueue_maintenance_against(
    db: &Db,
    user_id: &str,
    payload: serde_json::Map<String, Value>,
    pending: &mut Option<Vec<PendingMaintenance>>,
    scheduled_at: Option<String>,
) -> Result<String, DbError> {
    let mut normalized = payload.clone();
    normalized.insert("exemptPhases".into(), exempt_phases_of(&payload));
    let character = character_of(&payload).cloned();
    if let Some(list) = pending.as_mut() {
        if let Some(existing) = list
            .iter_mut()
            .find(|j| character_of(&j.payload) == character.as_ref())
        {
            let widened = widen_maintenance_payload(&existing.payload, &normalized);
            let (id, stored) = (existing.id.clone(), Value::Object(widened.clone()));
            db.write(move |w| {
                w.main().background_jobs().update(
                    &id,
                    &crate::db::background_jobs::BjUpdate {
                        payload: Some(stored),
                        updated_at: now_iso(),
                        ..Default::default()
                    },
                )
            })
            .await?;
            existing.payload = widened.clone();
            let character_text = character
                .as_ref()
                .map(|c| c.as_str().map_or_else(|| c.to_string(), str::to_string))
                .unwrap_or_default();
            let exempt_json = widened
                .get("exemptPhases")
                .map(Value::to_string)
                .unwrap_or_else(|| "null".to_string());
            let reason_text = match widened.get("reason") {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => "undefined".to_string(),
            };
            match widened.get("phases") {
                Some(phases) => tracing::debug!(
                    characterId = %character_text,
                    existingJobId = %existing.id,
                    phasesJson = %phases,
                    exemptPhasesJson = %exempt_json,
                    reason = %reason_text,
                    "[Maintenance] Pending job widened to the requested phases"
                ),
                None => tracing::debug!(
                    characterId = %character_text,
                    existingJobId = %existing.id,
                    phases = "all",
                    exemptPhasesJson = %exempt_json,
                    reason = %reason_text,
                    "[Maintenance] Pending job widened to the requested phases"
                ),
            }
            return Ok(existing.id.clone());
        }
    }
    let id = enqueue_job_at(
        db,
        user_id,
        MEMORY_MAINTENANCE,
        Value::Object(normalized.clone()),
        1.0,
        scheduled_at,
    )
    .await?;
    if let Some(list) = pending.as_mut() {
        list.push(PendingMaintenance {
            id: id.clone(),
            payload: normalized,
        });
    }
    Ok(id)
}

/// v4 `enqueueMemoryMaintenance(userId, payload, options?)`: "enqueue memory
/// maintenance for one character. Dedupe key is the character, whatever the
/// reason (bug 221): a PENDING maintenance job for the same character absorbs
/// the request — it widens to the phases asked for and adopts the requested
/// overrides — so the daily sweep, a watermark run and a manual run collapse
/// into one job. … A PROCESSING job never absorbs a request: it has already
/// read the corpus and may be past the phase asked for. A new PENDING job is
/// created instead, and the handler defers it while the running one
/// finishes, so two runs never work on one character at once. Retry-hostile:
/// … attempts default to 1."
pub async fn enqueue_memory_maintenance(
    db: &Db,
    user_id: &str,
    payload: serde_json::Map<String, Value>,
    scheduled_at: Option<String>,
) -> Result<String, DbError> {
    let mut pending = find_pending_maintenance(db, user_id);
    enqueue_maintenance_against(db, user_id, payload, &mut pending, scheduled_at).await
}

/// v4 `enqueueMemoryMaintenanceMany(userId, characterIds, payload, options?)`:
/// "one maintenance job per listed character, each deduped per character,
/// with one read of the user's pending jobs for the whole batch. Returns every
/// job id, new or widened, without duplicates." Each payload is `{...payload,
/// characterId}` (the id appended unless the base already carries it).
pub async fn enqueue_memory_maintenance_many(
    db: &Db,
    user_id: &str,
    character_ids: &[String],
    payload: serde_json::Map<String, Value>,
    scheduled_at: Option<String>,
) -> Result<Vec<String>, DbError> {
    let mut pending = find_pending_maintenance(db, user_id);
    let mut ids: Vec<String> = Vec::new();
    for character_id in character_ids {
        let mut one = payload.clone();
        one.insert("characterId".into(), Value::String(character_id.clone()));
        let id = enqueue_maintenance_against(db, user_id, one, &mut pending, scheduled_at.clone())
            .await?;
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// v4 `enqueueMemoryMaintenanceForUser(userId, payload, options?)`: "one
/// maintenance job per live character the user owns (each deduped per
/// character). Returns every job id, new or widened." DEBUG `[Maintenance]
/// User-wide run fanned out per character {userId, reason, characters,
/// jobs}`.
pub async fn enqueue_memory_maintenance_for_user(
    db: &Db,
    user_id: &str,
    payload: serde_json::Map<String, Value>,
) -> Result<Vec<String>, DbError> {
    let uid = user_id.to_string();
    let characters = db.read_main(|main| {
        db.read_mount_index(|mount| crate::db::characters_read::find_by_user_id(main, mount, &uid))
    })?;
    let live: Vec<String> = characters
        .iter()
        .filter(|c| !crate::api::characters::is_archived(c))
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let reason = payload
        .get("reason")
        .map(|r| r.as_str().map_or_else(|| r.to_string(), str::to_string))
        .unwrap_or_else(|| "undefined".to_string());
    let ids = enqueue_memory_maintenance_many(db, user_id, &live, payload, None).await?;
    tracing::debug!(
        userId = %user_id,
        reason = %reason,
        characters = live.len(),
        jobs = ids.len(),
        "[Maintenance] User-wide run fanned out per character"
    );
    Ok(ids)
}
// === end P4.D297 ===

// === P4.D296: memory extraction enqueue ===

/// One `MEMORY_EXTRACTION` enqueue — v4 `MemoryExtractionPayload` as its five
/// writers build it (`ac6348c5f` + bug 245's `extractAfterMessageId`; pin
/// `83334b9a5`). Every writer's object literal shares ONE key order, so
/// [`MemoryExtractionEnqueue::to_payload`] writes each caller's stored bytes:
/// `{chatId, turnOpenerMessageId, extractionAnchorMessageId[,
/// extractAfterMessageId], connectionProfileId[, pass][, cursor][, ignoreIdle:
/// true][, backfill: true]}` — the opener and anchor always materialized
/// (`null` included), the rest only when set:
///
///  - the turn-end trigger `{chatId, turnOpenerMessageId,
///    extractionAnchorMessageId, connectionProfileId}`;
///  - the sweep `{…, extractionAnchorMessageId: <anchor>, connectionProfileId,
///    pass}`;
///  - a catch-up's continuation `{…, extractionAnchorMessageId: null,
///    connectionProfileId, pass, cursor[, ignoreIdle][, backfill]}`;
///  - the regenerate's turn `{…[, extractAfterMessageId], connectionProfileId,
///    backfill}` and catch-up `{…, extractionAnchorMessageId:
///    "regenerate:<jobId>", connectionProfileId, pass, ignoreIdle, backfill}`.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryExtractionEnqueue<'a> {
    pub chat_id: &'a str,
    pub turn_opener_message_id: Option<&'a str>,
    pub extraction_anchor_message_id: Option<&'a str>,
    /// Bug 245 — written only when set (v4 `...(x ? { x } : {})`).
    pub extract_after_message_id: Option<&'a str>,
    pub connection_profile_id: &'a str,
    /// The grain — RAW (L-4): a catch-up continues with the payload's own
    /// string.
    pub pass: Option<&'a str>,
    pub cursor: Option<&'a str>,
    pub ignore_idle: bool,
    pub backfill: bool,
    /// v4 `options.skipDedupCheck` — the regenerate's.
    pub skip_dedup_check: bool,
}

impl MemoryExtractionEnqueue<'_> {
    /// The stored payload, in v4's key order.
    pub fn to_payload(&self) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("chatId".into(), Value::from(self.chat_id));
        m.insert(
            "turnOpenerMessageId".into(),
            self.turn_opener_message_id.map_or(Value::Null, Value::from),
        );
        m.insert(
            "extractionAnchorMessageId".into(),
            self.extraction_anchor_message_id
                .map_or(Value::Null, Value::from),
        );
        if let Some(after) = self.extract_after_message_id.filter(|a| !a.is_empty()) {
            m.insert("extractAfterMessageId".into(), Value::from(after));
        }
        m.insert(
            "connectionProfileId".into(),
            Value::from(self.connection_profile_id),
        );
        if let Some(pass) = self.pass {
            m.insert("pass".into(), Value::from(pass));
        }
        if let Some(cursor) = self.cursor {
            m.insert("cursor".into(), Value::from(cursor));
        }
        if self.ignore_idle {
            m.insert("ignoreIdle".into(), Value::Bool(true));
        }
        if self.backfill {
            m.insert("backfill".into(), Value::Bool(true));
        }
        Value::Object(m)
    }
}

/// `x ?? null` over a payload key (absent and `null` alike).
fn or_null(v: Option<&Value>) -> Value {
    match v {
        None => Value::Null,
        Some(v) => v.clone(),
    }
}

/// v4 `enqueueMemoryExtraction`'s dedupe match (`queue-service.ts:567-574`):
/// `chatId ===`, `turnOpenerMessageId ===` (STRICT — a stored ABSENT key
/// never equals an incoming `null`; L-6, where pre-round v5 collapsed both),
/// `(anchor ?? null) ===`, `memoryExtractionPassOf ===` (raw values compared
/// as v4 compares them), `(cursor ?? null) ===`.
pub fn memory_extraction_dedupe_matches(existing: &Value, incoming: &Value) -> bool {
    existing.get("chatId").is_some()
        && existing.get("chatId") == incoming.get("chatId")
        && match (
            existing.get("turnOpenerMessageId"),
            incoming.get("turnOpenerMessageId"),
        ) {
            (Some(a), Some(b)) => a == b,
            // `undefined === undefined` — never reached by a v4 writer.
            (None, None) => true,
            _ => false,
        }
        && or_null(existing.get("extractionAnchorMessageId"))
            == or_null(incoming.get("extractionAnchorMessageId"))
        && pass_of(existing) == pass_of(incoming)
        && or_null(existing.get("cursor")) == or_null(incoming.get("cursor"))
}

/// v4 `enqueueMemoryExtraction(userId, payload, options?)`: with
/// `skip_dedup_check` straight to `enqueueJob`; else a PENDING / PROCESSING
/// `MEMORY_EXTRACTION` of the user matching
/// [`memory_extraction_dedupe_matches`] is returned instead. A lookup failure
/// WARNs `[MemoryExtraction] Failed to check for existing jobs during
/// enqueue` and enqueues anyway. `maxAttempts` is v4's default 3.
pub async fn enqueue_memory_extraction_with(
    db: &Db,
    user_id: &str,
    job: &MemoryExtractionEnqueue<'_>,
) -> Result<String, DbError> {
    let payload = job.to_payload();
    if !job.skip_dedup_check {
        let uid = user_id.to_string();
        let in_flight = db.read_main(|conn| {
            let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
            let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
            jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
            Ok(jobs)
        });
        match in_flight {
            Ok(jobs) => {
                let existing = jobs.iter().find(|j| {
                    j.job_type == "MEMORY_EXTRACTION"
                        && serde_json::from_str::<Value>(&j.payload)
                            .is_ok_and(|p| memory_extraction_dedupe_matches(&p, &payload))
                });
                if let Some(existing) = existing {
                    return Ok(existing.id.clone());
                }
            }
            Err(error) => tracing::warn!(
                error = %crate::db::fallback::error_text(&error),
                "[MemoryExtraction] Failed to check for existing jobs during enqueue"
            ),
        }
    }
    enqueue_job(db, user_id, "MEMORY_EXTRACTION", payload, 3.0).await
}

/// The turn-end trigger's enqueue (`{chatId, turnOpenerMessageId,
/// extractionAnchorMessageId, connectionProfileId}`, deduped) — the
/// finalizer's `trigger_turn_memory_extraction` call.
pub async fn enqueue_memory_extraction(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    turn_opener_message_id: Option<&str>,
    extraction_anchor_message_id: Option<&str>,
    connection_profile_id: &str,
) -> Result<String, DbError> {
    enqueue_memory_extraction_with(
        db,
        user_id,
        &MemoryExtractionEnqueue {
            chat_id,
            turn_opener_message_id,
            extraction_anchor_message_id,
            connection_profile_id,
            ..Default::default()
        },
    )
    .await
}

/// One entry of v4's `MemoryExtractionBatchEntry` union in its object arm
/// (`{turnOpenerMessageId, extractionAnchorMessageId?, extractAfterMessageId?}`
/// — the bare-string / null legacy arms have no caller).
#[derive(Debug, Clone, Default)]
pub struct MemoryExtractionBatchEntry {
    pub turn_opener_message_id: Option<String>,
    pub extraction_anchor_message_id: Option<String>,
    /// Bug 245 — an autonomous room's previous anchor.
    pub extract_after_message_id: Option<String>,
}

/// v4 `enqueueMemoryExtractionBatch` — ONE batch insert of `MEMORY_EXTRACTION`
/// jobs, one per entry; returns the minted ids in input order; an empty input
/// inserts nothing. No dedupe (a second "queue memories" stacks a second set;
/// the action clears prior jobs first). Every batch rebuilds history, so each
/// payload carries `backfill: true` (`{chatId, turnOpenerMessageId,
/// extractionAnchorMessageId[, extractAfterMessageId], connectionProfileId,
/// backfill}`). INFO `Memory extraction batch enqueued {chatId, jobCount}`.
pub async fn enqueue_memory_extraction_batch(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    connection_profile_id: &str,
    entries: &[MemoryExtractionBatchEntry],
    priority: f64,
) -> Result<Vec<String>, DbError> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let now = now_iso();
    let jobs: Vec<BjCreate> = entries
        .iter()
        .map(|e| BjCreate {
            user_id: user_id.to_string(),
            job_type: "MEMORY_EXTRACTION".to_string(),
            status: Some("PENDING".to_string()),
            payload: MemoryExtractionEnqueue {
                chat_id,
                turn_opener_message_id: e.turn_opener_message_id.as_deref(),
                extraction_anchor_message_id: e.extraction_anchor_message_id.as_deref(),
                extract_after_message_id: e.extract_after_message_id.as_deref(),
                connection_profile_id,
                backfill: true,
                ..Default::default()
            }
            .to_payload(),
            priority,
            attempts: 0.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: now.clone(),
            started_at: None,
            completed_at: None,
        })
        .collect();
    let ids: Vec<String> = (0..jobs.len())
        .map(|_| uuid::Uuid::new_v4().to_string())
        .collect();
    let (jobs_for_write, ids_for_write, now_for_write) = (jobs, ids.clone(), now);
    db.write(move |ws| {
        crate::db::background_jobs::BackgroundJobsRepository::new(ws.main().connection())
            .create_batch(&jobs_for_write, &ids_for_write, &now_for_write)
            .map(|_| ())
    })
    .await?;
    tracing::info!(
        chatId = %chat_id,
        jobCount = ids.len(),
        "Memory extraction batch enqueued"
    );
    // v4 fires the processor only when something was actually enqueued — and
    // publishes under the same guard. The empty case returned early above.
    publish_realtime(RealtimeTopic::Jobs, None);
    ensure_processor_running();
    Ok(ids)
}

/// v4 `enqueueMemoryRegenerateChat` (`queue-service.ts:847-877`, P4.D296 R-V)
/// — deduped by chat against the user's pending / processing
/// `MEMORY_REGENERATE_CHAT` jobs (a failed look-up WARNs and enqueues anyway),
/// `maxAttempts` 1.
pub async fn enqueue_memory_regenerate_chat(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    connection_profile_id: &str,
) -> Result<String, DbError> {
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    match in_flight {
        Ok(jobs) => {
            let existing = jobs.iter().find(|j| {
                j.job_type == "MEMORY_REGENERATE_CHAT"
                    && serde_json::from_str::<Value>(&j.payload)
                        .ok()
                        .and_then(|p| p.get("chatId").and_then(Value::as_str).map(str::to_string))
                        .as_deref()
                        == Some(chat_id)
            });
            if let Some(existing) = existing {
                return Ok(existing.id.clone());
            }
        }
        Err(error) => tracing::warn!(
            error = %crate::db::fallback::error_text(&error),
            "[MemoryRegenerateChat] Failed to check for existing jobs during enqueue"
        ),
    }
    enqueue_job(
        db,
        user_id,
        "MEMORY_REGENERATE_CHAT",
        serde_json::json!({ "chatId": chat_id, "connectionProfileId": connection_profile_id }),
        1.0,
    )
    .await
}

// === end P4.D296 ===

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
    force: bool,
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
        // P4.D238 (the `caller_context` rider): v4's two INFO lines, absent
        // until now.
        tracing::info!(
            context = "background-jobs.queue",
            chat_id = %chat_id,
            character_id = %character_id,
            existing_job_id = %existing.id,
            "[CharacterAvatar] Reusing existing pending job"
        );
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
    // v4 `...(force ? { force: true } : {})` — the key is written ONLY when the
    // reroll is asked for, so an automatic trigger's job row carries no `force`
    // key at all and the payload bytes match v4's byte for byte.
    if force {
        payload.insert("force".into(), Value::Bool(true));
    }

    let job_id = enqueue_job(
        db,
        user_id,
        "CHARACTER_AVATAR_GENERATION",
        Value::Object(payload),
        3.0,
    )
    .await?;
    tracing::info!(
        context = "background-jobs.queue",
        chat_id = %chat_id,
        character_id = %character_id,
        job_id = %job_id,
        "[CharacterAvatar] Avatar generation job enqueued"
    );
    Ok((job_id, true))
}

/// v4 `WardrobeItemImageGenerationPayload` (`queue-service.ts:236-247`,
/// `b3f937076`; P4.D255) — a wardrobe item picture queued by a wardrobe tool.
/// Tools only create and edit character-owned items, so the container is
/// always the owning character's wardrobe (measured at the pin: the payload
/// carries no scope / container key — `tool-image-generation.ts:100-104`).
/// Serialized in v4's key order: `chatId`, `characterId`, `itemId`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeItemImageJobPayload {
    /// The chat the tool ran in — scopes the pending-job dedupe.
    pub chat_id: String,
    /// The character whose wardrobe holds the item (the recipient, for a gift).
    pub character_id: String,
    /// The wardrobe item to draw.
    pub item_id: String,
}

/// v4 `enqueueWardrobeItemImageGeneration(userId, payload)`
/// (`queue-service.ts:1246-1286`, `b3f937076`; P4.D255): collapses against a
/// still-PENDING `WARDROBE_ITEM_IMAGE_GENERATION` job for the same item in the
/// same chat — the handler reads the item when it runs, so one queued job
/// already covers any later edit; a PROCESSING job has already built its
/// prompt, so a fresh one is enqueued (`find_pending_for_chat` answers both
/// statuses, so PENDING is filtered here). One try (`maxAttempts: 1` — a
/// refusal or a provider failure would only be paid for again). v4's two INFO
/// lines. Answers `(jobId, isNew)`.
pub async fn enqueue_wardrobe_item_image_generation(
    db: &Db,
    user_id: &str,
    payload: &WardrobeItemImageJobPayload,
) -> Result<(String, bool), DbError> {
    let chat_id = payload.chat_id.clone();
    let pending = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn)
            .find_pending_for_chat(&chat_id)
    })?;
    let existing = pending.iter().find(|j| {
        j.job_type == "WARDROBE_ITEM_IMAGE_GENERATION"
            && j.status == "PENDING"
            && serde_json::from_str::<Value>(&j.payload)
                .ok()
                .and_then(|p| p.get("itemId").and_then(Value::as_str).map(str::to_string))
                .as_deref()
                == Some(payload.item_id.as_str())
    });
    if let Some(existing) = existing {
        tracing::info!(
            context = "background-jobs.queue",
            chatId = %payload.chat_id,
            itemId = %payload.item_id,
            existingJobId = %existing.id,
            "[WardrobeItemImage] Reusing existing pending job"
        );
        return Ok((existing.id.clone(), false));
    }
    let value = serde_json::to_value(payload)
        .map_err(|e| DbError::Internal(format!("wardrobe item image payload: {e}")))?;
    let job_id = enqueue_job(db, user_id, "WARDROBE_ITEM_IMAGE_GENERATION", value, 1.0).await?;
    tracing::info!(
        context = "background-jobs.queue",
        chatId = %payload.chat_id,
        characterId = %payload.character_id,
        itemId = %payload.item_id,
        jobId = %job_id,
        "[WardrobeItemImage] Wardrobe item image job enqueued"
    );
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
    enqueue_story_background_generation_with(
        db,
        user_id,
        chat_id,
        image_profile_id,
        character_ids,
        scene_context,
        project_id,
        false,
    )
    .await
}

/// [`enqueue_story_background_generation`] with v4's `forceUncensored` payload
/// key (`ce2f1dabf`, #77): spread LAST, after `projectId`, and ONLY when true
/// (`...(forceUncensored ? { forceUncensored: true } : {})`) — so a plain
/// regenerate's payload is byte-unchanged.
///
/// ⚠ **The dedupe drops the force silently** (v4's, ported as-is; a candidate
/// upstream note): a forced request that finds an in-flight PLAIN job for the
/// chat reuses it (`is_new = false`) and that job paints moderated.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_story_background_generation_with(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    image_profile_id: &str,
    character_ids: &[String],
    scene_context: Option<&str>,
    project_id: Option<String>,
    force_uncensored: bool,
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
    if force_uncensored {
        payload.insert("forceUncensored".into(), Value::Bool(true));
    }

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
/// `EMBEDDING_REFIT` handler fires this after a successful vocabulary refit.
///
/// The payload is v4's object literal, and — as with the render enqueue — the
/// KEY SET depends on the caller: v4's refit passes `{ profileId }` alone
/// (`embedding-refit.ts:145`), so `scope: None` OMITS the key rather than
/// writing `"all"`, and the handler's own `?? 'all'` default supplies it. A
/// `Some(scope)` caller (the unported embedding-profiles management surface,
/// `p4.9h`) writes both keys.
pub async fn enqueue_embedding_reindex_all(
    db: &Db,
    user_id: &str,
    profile_id: &str,
    scope: Option<&str>,
) -> Result<String, DbError> {
    let payload = match scope {
        Some(s) => serde_json::json!({ "profileId": profile_id, "scope": s }),
        None => serde_json::json!({ "profileId": profile_id }),
    };
    enqueue_job_with_priority(db, user_id, "EMBEDDING_REINDEX_ALL", payload, -1.0, 3.0).await
}

/// v4 `enqueueEmbeddingRefit` — enqueue an `EMBEDDING_REFIT` (TF-IDF vocabulary
/// rebuild) at `priority = -1`, `maxAttempts` = the default 3. `trigger_reindex`
/// controls whether the refit handler fans out reindex jobs after fitting; the
/// embedding-profiles PUT matrix (BUILTIN branch) and the `?action=refit` route
/// both pass `true`. Payload `{profileId, triggerReindex}`.
pub async fn enqueue_embedding_refit(
    db: &Db,
    user_id: &str,
    profile_id: &str,
    trigger_reindex: bool,
) -> Result<String, DbError> {
    let payload = serde_json::json!({
        "profileId": profile_id,
        "triggerReindex": trigger_reindex,
    });
    enqueue_job_with_priority(db, user_id, "EMBEDDING_REFIT", payload, -1.0, 3.0).await
}

/// v4 `enqueueEmbeddingReapplyProfile` — enqueue an `EMBEDDING_REAPPLY_PROFILE`
/// (Matryoshka slice + renormalize, a pure-local rewrite, no provider call) at
/// `priority = -1`, `maxAttempts` = the default 3. The PUT matrix's narrow-only
/// arm and the `?action=reapply` route fire it. Payload `{profileId}`.
pub async fn enqueue_embedding_reapply_profile(
    db: &Db,
    user_id: &str,
    profile_id: &str,
) -> Result<String, DbError> {
    let payload = serde_json::json!({ "profileId": profile_id });
    enqueue_job_with_priority(db, user_id, "EMBEDDING_REAPPLY_PROFILE", payload, -1.0, 3.0).await
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
/// sweep (image collapse, cache collapse) computes its cutoff
/// from this one value so they always agree on "stale". A read error resolves to
/// the fallback (v4's `try/catch → default`).
pub fn resolve_stale_chat_days(db: &Db) -> i64 {
    db.read_main(crate::db::instance_settings::get_data_retention_settings)
        .unwrap_or(STALE_CHAT_RETENTION_DAYS)
}

/// [`resolve_stale_chat_days`] against one borrowed connection — the boot
/// reconcile's form (it runs on the writer's main connection and cannot reach
/// the read pool from inside a write closure).
pub fn resolve_stale_chat_days_conn(conn: &rusqlite::Connection) -> i64 {
    crate::db::instance_settings::get_data_retention_settings(conn)
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

/// v4 `getActivitySnapshot(userId?)` (`664cfca84`) — active (PENDING+PROCESSING)
/// job counts grouped by activity kind, MERGED with the non-job work currently
/// registered in [`crate::services::activity_registry`].
///
/// This is what the toolbar chips read: a chip stays lit for the whole span of
/// the work it names, whether that work is a queued job or something running
/// inline in a request. `started` is the registry's monotonic per-kind blip
/// totals since process boot — a delta base for the client, never an absolute.
pub async fn get_activity_snapshot(
    db: &Db,
    user_id: Option<&str>,
) -> Result<ActivitySnapshot, DbError> {
    let uid = user_id.map(str::to_string);
    let job_counts = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn)
            .get_active_counts_by_kind(uid.as_deref())
    })?;
    Ok(merge_activity_snapshot(job_counts))
}

/// The merge half of [`get_activity_snapshot`], separated so the jobs verb can
/// read the job counts inside its own single read transaction and still fold
/// them through exactly this code (v4 does two reads; the merge is the part that
/// must not diverge).
pub fn merge_activity_snapshot(job_counts: ActivityCounts) -> ActivitySnapshot {
    let inline = activity_counts();
    let mut active = ActivityCounts::default();
    for kind in ACTIVITY_KINDS {
        active.set(kind, job_counts.get(kind) + inline.get(kind));
    }
    ActivitySnapshot {
        active,
        started: activity_start_totals(),
    }
}

/// v4's `{ active, started }` activity snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActivitySnapshot {
    /// Job rows mapped through the kind table PLUS in-flight registry counts.
    pub active: ActivityCounts,
    /// Monotonic per-kind totals of ended spans that outlived the blip threshold.
    pub started: ActivityCounts,
}

/// v4 `cancelJob(jobId)` — cancel a PENDING|FAILED job (→ DEAD). Returns whether a
/// row was cancelled.
pub async fn cancel_job(db: &Db, job_id: &str) -> Result<bool, DbError> {
    let id = job_id.to_string();
    let cancelled = db
        .write(move |writers| {
            crate::db::background_jobs::BackgroundJobsRepository::new(writers.main().connection())
                .cancel(&id)
        })
        .await?;
    // Only when the cancel actually took (v4 `queue-service.ts:1154`,
    // `f3892158d`: `if (cancelled) publishRealtime('jobs')`).
    if cancelled {
        publish_realtime(RealtimeTopic::Jobs, None);
    }
    Ok(cancelled)
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

/// v4 `runScheduledCleanup()` — for every user whose
/// `llmLoggingSettings.enabled` is true AND `retentionDays > 0`, enqueue one
/// `LLM_LOG_CLEANUP` job (`{ userId, retentionDays }`). Returns `(usersProcessed,
/// jobsEnqueued)`. The host driver calls this on the daily cadence (v4 runs it
/// immediately on startup — no grace).
///
/// ⚠️ **The bag defaults are load-bearing** (P4.24). v4's `findAll` Zod-parses
/// every row, so a `chat_settings` row whose `llmLoggingSettings` cell is SQL
/// NULL arrives as `LLMLoggingSettingsSchema`'s default —
/// `{ enabled: true, retentionDays: 30 }` — and IS swept. This function used to
/// read a NULL cell as "not configured" and `continue`, which silently dropped
/// such a user from the daily sweep forever; the first differential ever pointed
/// at this enqueuer caught it. Both this and the handler resolve the bag through
/// [`llm_logging_settings_defaults`] so they cannot drift apart again.
pub async fn run_scheduled_cleanup(db: &Db) -> Result<(usize, usize), DbError> {
    let settings = db.read_main(crate::db::chat_settings::find_all_scheduler_settings)?;
    let mut users_processed = 0usize;
    let mut jobs_enqueued = 0usize;
    for s in &settings {
        let (enabled, retention_days) =
            crate::services::llm_log_cleanup_job::llm_logging_settings_defaults(
                s.llm_logging_settings.as_ref(),
            );
        if !enabled || retention_days <= 0.0 {
            continue;
        }
        let payload = cleanup_payload(&s.user_id, retention_days);
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

/// v4's `LLM_LOG_CLEANUP` payload: `{ userId, retentionDays }`, with
/// `retentionDays` materialized from the settings bag.
///
/// `retention_days` arrives as `f64` because the settings bag is untyped JSON,
/// but v4 carries a JS number all the way to `JSON.stringify`, where a
/// whole-valued number renders `7` — never `7.0`. Serializing the `f64`
/// directly wrote `"retentionDays":7.0` into a payload column both apps read
/// (found on the Friday copy 2026-07-31, dogfood finding #41), so the value
/// goes through [`js_number_to_json`] like every other persisted JS number.
fn cleanup_payload(user_id: &str, retention_days: f64) -> Value {
    serde_json::json!({
        "userId": user_id,
        "retentionDays": crate::db::js_number_to_json(retention_days),
    })
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
/// The payload is v4's `ConversationRenderPayload` object literal, and the KEY
/// SET depends on the caller: the manual `render-conversation` entrance passes
/// `{ chatId, fullReembed: true }`, while the startup reconcile passes
/// `{ chatId }` alone. `full_reembed: None` reproduces the latter — the key is
/// OMITTED, not written as `false`, so the stored payload bytes match v4's on
/// both paths (the differential dumps `background_jobs.payload`). Returns
/// `(job_id, is_new)`, v4's `ConversationRenderEnqueueResult`.
pub async fn enqueue_conversation_render(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    full_reembed: Option<bool>,
) -> Result<(String, bool), DbError> {
    let cid = chat_id.to_string();
    let pending = db.read_main(|conn| {
        crate::db::background_jobs::BackgroundJobsRepository::new(conn).find_pending_for_chat(&cid)
    })?;
    if let Some(existing) = pending.iter().find(|j| j.job_type == "CONVERSATION_RENDER") {
        return Ok((existing.id.clone(), false));
    }

    let payload = conversation_render_payload(chat_id, full_reembed);
    let id =
        enqueue_job_with_priority(db, user_id, "CONVERSATION_RENDER", payload, -1.0, 3.0).await?;
    Ok((id, true))
}

/// The `CONVERSATION_RENDER` payload literal (see
/// [`enqueue_conversation_render`] for why the key set varies by caller).
fn conversation_render_payload(chat_id: &str, full_reembed: Option<bool>) -> Value {
    match full_reembed {
        Some(v) => serde_json::json!({ "chatId": chat_id, "fullReembed": v }),
        None => serde_json::json!({ "chatId": chat_id }),
    }
}

/// [`enqueue_conversation_render`] on a borrowed writer connection — the shape a
/// BOOT pass needs (the startup reconcile runs inside one `db.write_blocking`
/// closure, so it cannot await the async form). Same dedupe, same priority, same
/// payload rules; mints the id and timestamps exactly as
/// [`enqueue_job_with_priority`] does.
pub fn enqueue_conversation_render_blocking(
    main: &rusqlite::Connection,
    user_id: &str,
    chat_id: &str,
    full_reembed: Option<bool>,
) -> Result<(String, bool), DbError> {
    let repo = crate::db::background_jobs::BackgroundJobsRepository::new(main);
    let pending = repo.find_pending_for_chat(chat_id)?;
    if let Some(existing) = pending.iter().find(|j| j.job_type == "CONVERSATION_RENDER") {
        return Ok((existing.id.clone(), false));
    }

    let now = now_iso();
    let id = uuid::Uuid::new_v4().to_string();
    let create = BjCreate {
        user_id: user_id.to_string(),
        job_type: "CONVERSATION_RENDER".to_string(),
        status: Some("PENDING".to_string()),
        payload: conversation_render_payload(chat_id, full_reembed),
        priority: -1.0,
        attempts: 0.0,
        max_attempts: 3.0,
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
    repo.create(&create, &opts)?;
    // A third v5 site for v4's one `enqueueJob` publish: this render enqueue
    // mints its row inside the caller's transaction rather than going through
    // `enqueue_job`, so it would otherwise be the one enqueue no client hears
    // about.
    publish_realtime(RealtimeTopic::Jobs, None);
    ensure_processor_running();
    Ok((id, true))
}

#[cfg(test)]
mod scheduled_cleanup_tests {
    use super::cleanup_payload;

    /// The bytes v4 writes: a whole retention renders without a fractional part.
    #[test]
    fn whole_retention_serializes_as_an_integer() {
        let payload = cleanup_payload("ffffffff-ffff-ffff-ffff-ffffffffffff", 7.0);
        assert_eq!(
            payload.to_string(),
            r#"{"userId":"ffffffff-ffff-ffff-ffff-ffffffffffff","retentionDays":7}"#
        );
    }

    /// A fractional retention keeps its fraction — `js_number_to_json` narrows
    /// only when the value is exactly integral, matching `JSON.stringify`.
    #[test]
    fn fractional_retention_keeps_its_fraction() {
        let payload = cleanup_payload("u", 0.5);
        assert_eq!(payload["retentionDays"].as_f64(), Some(0.5));
        assert!(payload.to_string().contains("0.5"));
    }
}

#[cfg(test)]
mod activity_snapshot_tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::services::activity_kinds::ActivityKind;
    use crate::services::activity_registry::{begin_activity, ActivityTestGuard};

    /// A temp `Db` carrying just the `background_jobs` columns these reads touch.
    /// The scratch dir rides out with the handle — bind it for the test's life.
    async fn test_db(tag: &str) -> (Db, tempfile::TempDir) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("qt-activity-snapshot-{tag}-"))
            .tempdir()
            .expect("tempdir");
        let db = Db::open(
            DbPaths {
                main: dir.path().join("main.db"),
                mount_index: None,
                llm_logs: None,
            },
            "dGVzdC1wZXBwZXItZm9yLWFjdGl2aXR5LXNuYXBzaG90",
        )
        .expect("open test db");
        db.write(|ws| {
            ws.main()
                .connection()
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS background_jobs (
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, status TEXT,
                       payload TEXT, priority REAL, attempts REAL, maxAttempts REAL,
                       error TEXT, scheduledAt TEXT, startedAt TEXT, completedAt TEXT,
                       createdAt TEXT, updatedAt TEXT);",
                )
                .map_err(Into::into)
        })
        .await
        .expect("create tables");
        (db, dir)
    }

    async fn seed_job(db: &Db, id: &str, user: &str, job_type: &str, status: &str) {
        let (id, user, job_type, status) = (
            id.to_string(),
            user.to_string(),
            job_type.to_string(),
            status.to_string(),
        );
        db.write(move |ws| {
            ws.main()
                .connection()
                .execute(
                    "INSERT INTO background_jobs \
                     (id, userId, type, status, payload, priority, attempts, maxAttempts, \
                      scheduledAt, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, '{}', 5, 0, 3, \
                             '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z', \
                             '2020-01-01T00:00:00.000Z')",
                    rusqlite::params![id, user, job_type, status],
                )
                .map_err(Into::into)
                .map(|_| ())
        })
        .await
        .expect("seed job");
    }

    /// The repo leg alone: PENDING+PROCESSING rows folded through the kind table,
    /// with unclaimed types (v4's explicit `null`s) contributing nothing and
    /// terminal statuses excluded.
    #[tokio::test]
    async fn active_counts_by_kind_folds_types_and_skips_the_uncounted() {
        let _g = ActivityTestGuard::new();
        let (db, _scratch) = test_db("bykind").await;
        seed_job(&db, "j1", "u1", "MEMORY_EXTRACTION", "PENDING").await;
        seed_job(&db, "j2", "u1", "MEMORY_MAINTENANCE", "PROCESSING").await;
        seed_job(&db, "j3", "u1", "EMBEDDING_GENERATE", "PENDING").await;
        // Deliberately uncounted (v4's `null`) — must not land on any chip.
        seed_job(&db, "j4", "u1", "AUTONOMOUS_ROOM_TURN", "PENDING").await;
        seed_job(&db, "j5", "u1", "LLM_LOG_CLEANUP", "PROCESSING").await;
        // Terminal — not active.
        seed_job(&db, "j6", "u1", "MEMORY_EXTRACTION", "COMPLETED").await;
        // Another user — excluded when scoped.
        seed_job(&db, "j7", "u2", "MEMORY_EXTRACTION", "PENDING").await;

        let snap = get_activity_snapshot(&db, Some("u1")).await.unwrap();
        assert_eq!(snap.active.memory, 2);
        assert_eq!(snap.active.embedding, 1);
        assert_eq!(snap.active.summary, 0);
        assert_eq!(snap.active.danger, 0);
        assert_eq!(snap.active.image, 0);

        // Unscoped picks up the other user's row too.
        let all = get_activity_snapshot(&db, None).await.unwrap();
        assert_eq!(all.active.memory, 3);
    }

    /// The MERGE is what no DB state can see: an inline span adds to the job
    /// rows' count for the same kind, and the monotonic totals ride along.
    #[tokio::test]
    async fn the_snapshot_merges_inline_registry_counts_with_job_rows() {
        let _g = ActivityTestGuard::new();
        let (db, _scratch) = test_db("merge").await;
        seed_job(&db, "j1", "u1", "EMBEDDING_GENERATE", "PENDING").await;

        let before = get_activity_snapshot(&db, Some("u1")).await.unwrap();
        assert_eq!(before.active.embedding, 1);
        assert_eq!(before.active.image, 0);

        let embedding_span = begin_activity(ActivityKind::Embedding);
        let image_span = begin_activity(ActivityKind::Image);
        let during = get_activity_snapshot(&db, Some("u1")).await.unwrap();
        assert_eq!(during.active.embedding, 2, "job row + inline span");
        assert_eq!(
            during.active.image, 1,
            "inline-only work still lights a chip"
        );

        embedding_span.end();
        image_span.end();
        let after = get_activity_snapshot(&db, Some("u1")).await.unwrap();
        assert_eq!(after.active.embedding, 1, "the job row survives the span");
        assert_eq!(after.active.image, 0);
    }

    /// `started` is the registry's totals verbatim — a delta base, never derived
    /// from job rows.
    #[tokio::test(start_paused = true)]
    async fn the_snapshot_started_totals_come_from_the_registry() {
        let _g = ActivityTestGuard::new();
        let (db, _scratch) = test_db("started").await;
        seed_job(&db, "j1", "u1", "TITLE_UPDATE", "PENDING").await;

        assert_eq!(
            get_activity_snapshot(&db, Some("u1"))
                .await
                .unwrap()
                .started,
            ActivityCounts::default(),
            "a fresh process answers zeros however many job rows exist"
        );

        crate::services::activity_registry::track_activity(ActivityKind::Danger, async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        })
        .await;

        let snap = get_activity_snapshot(&db, Some("u1")).await.unwrap();
        assert_eq!(snap.started.danger, 1);
        assert_eq!(
            snap.active.summary, 1,
            "the TITLE_UPDATE row is summary work"
        );
    }
}

// === P4.82 ===
/// v4 `enqueueCharacterHeadShouldersBackfill` (`queue-service.ts:604-651`) —
/// enqueue a `CHARACTER_HEADSHOULDERS_BACKFILL` job for one character, deduping
/// against every in-flight (`PENDING` ∪ `PROCESSING`) job of that type for the
/// same `(userId, characterId)`. Background priority (`-1`); generation is
/// idempotent, so callers may raise `max_attempts` — the startup scan asks for
/// 3 so a cold job child retries instead of burning its one shot.
///
/// Returns `(jobId, isNew)`.
///
/// **Blocking, over `&Connection`, on purpose** — the
/// `enqueue_conversation_render_blocking` precedent. v4's ONLY caller is the
/// startup scan ([`crate::services::headshoulders_backfill_enqueue`]),
/// which in v5 runs inside the boot-repair write closure; an `async`
/// `Db::write` form would deadlock against the writer that closure already
/// holds. The order's prescribed `async` shape would have needed a second
/// implementation to serve that one caller, so the file's own idiom won and the
/// deviation is recorded (`an-orders-prescribed-shape-may-fight-the-files-idiom`).
///
/// A dedupe read that ERRORS logs and falls through to the enqueue. v4's own
/// `catch` (`queue-service.ts:635-640`, the `[HeadShouldersBackfill] Failed to
/// check for existing jobs during enqueue` warn) is DEAD CODE: both reads are
/// `safeQuery(…, [])` (`background-jobs.repository.ts:101-118`), which never
/// throws — it logs `Error finding background jobs by user ID` itself and
/// answers `[]`, so v4 dedupes against nothing and enqueues. v5 has no
/// `safeQuery` layer, so the `Err` arm here renders THAT line (the sentence
/// v4 can actually produce) and falls through the same way; the dead warn's
/// sentence is kept as [`DEDUPE_LOOKUP_WARN_UNREACHABLE`] for the record
/// (the §3 unification review; the `SELECT_FAILED_UNREACHABLE` precedent).
/// v4's dead `catch` sentence (`queue-service.ts:636`) — unreachable in v4
/// because `findByUserId` is fallback-mode `safeQuery`; recorded, never emitted.
#[allow(dead_code)]
pub const DEDUPE_LOOKUP_WARN_UNREACHABLE: &str =
    "[HeadShouldersBackfill] Failed to check for existing jobs during enqueue";

pub fn enqueue_character_headshoulders_backfill_blocking(
    main: &rusqlite::Connection,
    user_id: &str,
    character_id: &str,
    max_attempts: f64,
) -> Result<(String, bool), DbError> {
    let repo = crate::db::background_jobs::BackgroundJobsRepository::new(main);
    // v4 reads PENDING and PROCESSING separately and concatenates, in that
    // order — reproduced rather than collapsed into one `IN (…)`, because the
    // helper returns the FIRST match and each read is independently capped at
    // 100 newest-first.
    let in_flight = repo
        .find_by_user_id(user_id, Some("PENDING"))
        .and_then(|mut pending| {
            let processing = repo.find_by_user_id(user_id, Some("PROCESSING"))?;
            pending.extend(processing);
            Ok(pending)
        });
    match in_flight {
        Ok(jobs) => {
            let existing = jobs.iter().find(|j| {
                j.job_type == "CHARACTER_HEADSHOULDERS_BACKFILL"
                    && serde_json::from_str::<Value>(&j.payload)
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
        }
        Err(e) => {
            tracing::error!(
                user_id = %user_id,
                error = %e,
                "Error finding background jobs by user ID"
            );
            // Fall through and enqueue anyway (v4 dedupes against `[]`).
        }
    }

    let now = now_iso();
    let id = uuid::Uuid::new_v4().to_string();
    let create = BjCreate {
        user_id: user_id.to_string(),
        job_type: "CHARACTER_HEADSHOULDERS_BACKFILL".to_string(),
        status: Some("PENDING".to_string()),
        payload: serde_json::json!({ "characterId": character_id }),
        priority: -1.0,
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
    repo.create(&create, &opts)?;
    // A fourth v5 site for v4's one `enqueueJob` publish: this enqueue mints its
    // row inside the caller's transaction rather than going through
    // `enqueue_job`, so it would otherwise be an enqueue no client hears about.
    publish_realtime(RealtimeTopic::Jobs, None);
    ensure_processor_running();
    Ok((id, true))
}
// === end P4.82 ===

#[cfg(test)]
mod avatar_enqueue_tests {
    use super::*;
    use crate::db::runtime::DbPaths;

    /// The scratch dir rides out with the handle — bind it for the test's life.
    async fn test_db(tag: &str) -> (Db, tempfile::TempDir) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("qt-avatar-enqueue-{tag}-"))
            .tempdir()
            .expect("tempdir");
        let db = Db::open(
            DbPaths {
                main: dir.path().join("main.db"),
                mount_index: None,
                llm_logs: None,
            },
            "dGVzdC1wZXBwZXItZm9yLWF2YXRhci1lbnF1ZXVl",
        )
        .expect("open test db");
        db.write(|ws| {
            ws.main()
                .connection()
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS background_jobs (
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, status TEXT,
                       payload TEXT, priority REAL, attempts REAL, maxAttempts REAL,
                       lastError TEXT, scheduledAt TEXT, startedAt TEXT, completedAt TEXT,
                       createdAt TEXT, updatedAt TEXT);",
                )
                .map_err(Into::into)
        })
        .await
        .expect("create tables");
        (db, dir)
    }

    async fn payload_of(db: &Db, job_id: &str) -> Value {
        let id = job_id.to_string();
        let raw: String = db
            .read_main(move |c| {
                c.query_row(
                    "SELECT payload FROM background_jobs WHERE id = ?1",
                    rusqlite::params![id],
                    |r| r.get(0),
                )
                .map_err(Into::into)
            })
            .expect("the job row");
        serde_json::from_str(&raw).expect("payload json")
    }

    /// The `force` wire's WRITE half, end to end through the enqueue — the
    /// manual regenerate's row carries a literal `true`, the automatic
    /// trigger's row carries NO `force` key at all (v4 `...(force ? { force:
    /// true } : {})`). The tier-3 family constructs its payload in Rust, so
    /// without this a deleted `payload.insert` would leave every test green.
    #[tokio::test]
    async fn the_manual_reroll_writes_force_true_and_the_automatic_trigger_writes_no_key() {
        let (db, _scratch) = test_db("force").await;
        let (manual, _) =
            enqueue_character_avatar_generation(&db, "u1", "chat-a", "ch1", "p1", None, true)
                .await
                .expect("enqueue manual");
        let payload = payload_of(&db, &manual).await;
        assert_eq!(payload["force"], Value::Bool(true), "{payload}");

        let (auto, _) =
            enqueue_character_avatar_generation(&db, "u1", "chat-b", "ch1", "p1", None, false)
                .await
                .expect("enqueue automatic");
        let payload = payload_of(&db, &auto).await;
        assert!(
            payload.get("force").is_none(),
            "an automatic trigger must not carry the key: {payload}"
        );
    }
}

#[cfg(test)]
mod wardrobe_item_image_enqueue_tests {
    use super::*;
    use crate::db::runtime::DbPaths;

    fn test_db(dir: &tempfile::TempDir, rt: &tokio::runtime::Runtime) -> Db {
        let db = Db::open(
            DbPaths {
                main: dir.path().join("main.db"),
                mount_index: None,
                llm_logs: None,
            },
            "dGVzdC1wZXBwZXItZm9yLWF2YXRhci1lbnF1ZXVl",
        )
        .expect("open test db");
        rt.block_on(db.write(|ws| {
            ws.main()
                .connection()
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS background_jobs (
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, status TEXT,
                       payload TEXT, priority REAL, attempts REAL, maxAttempts REAL,
                       lastError TEXT, scheduledAt TEXT, startedAt TEXT, completedAt TEXT,
                       createdAt TEXT, updatedAt TEXT);",
                )
                .map_err(Into::into)
        }))
        .expect("create tables");
        db
    }

    /// v4's payload bytes and one try; a still-PENDING job for the same item
    /// in the same chat is reused (v4's line), a PROCESSING one is not.
    #[test]
    fn enqueue_dedupes_against_pending_only() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db = test_db(&dir, &rt);
        let payload = WardrobeItemImageJobPayload {
            chat_id: "chat-1".into(),
            character_id: "char-1".into(),
            item_id: "item-1".into(),
        };
        let ((first, is_new), lines) = crate::test_support::captured_with(|| {
            rt.block_on(enqueue_wardrobe_item_image_generation(&db, "u1", &payload))
                .unwrap()
        });
        assert!(is_new);
        assert!(lines.iter().any(|l| l.ends_with(&format!(
            "[WardrobeItemImage] Wardrobe item image job enqueued context=background-jobs.queue chatId=chat-1 characterId=char-1 itemId=item-1 jobId={first}"
        ))), "{lines:#?}");
        let id = first.clone();
        let (raw, attempts): (String, f64) = db
            .read_main(move |c| {
                c.query_row(
                    "SELECT payload, maxAttempts FROM background_jobs WHERE id = ?1",
                    rusqlite::params![id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(
            raw,
            r#"{"chatId":"chat-1","characterId":"char-1","itemId":"item-1"}"#
        );
        assert_eq!(attempts, 1.0);

        let ((again, is_new), lines) = crate::test_support::captured_with(|| {
            rt.block_on(enqueue_wardrobe_item_image_generation(&db, "u1", &payload))
                .unwrap()
        });
        assert_eq!((again.as_str(), is_new), (first.as_str(), false));
        assert!(lines.iter().any(|l| l.ends_with(&format!(
            "[WardrobeItemImage] Reusing existing pending job context=background-jobs.queue chatId=chat-1 itemId=item-1 existingJobId={first}"
        ))), "{lines:#?}");

        let id = first.clone();
        rt.block_on(db.write(move |ws| {
            ws.main()
                .connection()
                .execute(
                    "UPDATE background_jobs SET status = 'PROCESSING' WHERE id = ?1",
                    [id],
                )
                .map(|_| ())
                .map_err(Into::into)
        }))
        .unwrap();
        let (fresh, is_new) = rt
            .block_on(enqueue_wardrobe_item_image_generation(&db, "u1", &payload))
            .unwrap();
        assert!(is_new);
        assert_ne!(fresh, first);
    }
}
