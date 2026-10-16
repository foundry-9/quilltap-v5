//! `admit_candidate` — how a memory enters the Commonplace Book (memory
//! refactor §3.6) — v4 `lib/memory/admission/admit.ts` at the `83334b9a5` pin
//! (`8af764514` Phase C; bugs 209 / 220 from `c2795733f`; the TF-IDF scale
//! from `4ae0c4ca6`; bug 236's `empty` reject from `93c5af6ad`).
//!
//! The candidate becomes its row, passes the safety nets
//! ([`super::prepare`]), and is embedded — the expensive part, one retry
//! after 500 ms. A candidate that cannot be embedded is rejected; nothing is
//! written (invariant 4). Then the parent half: top-K search over both tiers,
//! hydrate, [`decide_admission`], write, publish `memories`.
//!
//! v4's why for the three guards, carried:
//!
//!  - **One admission per character at a time.** "Search → decide → write
//!    spans several awaits; two jobs admitting for one character would each
//!    miss the other's new row and both insert. A per-character lock
//!    serialises them." ([`with_admission_lock`], LR-3.)
//!  - **A retried job does not count twice.** "Admission commits at once,
//!    outside the job's write batch, so a job that fails after admitting and
//!    is retried would otherwise reinforce the same observations again." Each
//!    admission made inside a job carries a key (`jobId : content :
//!    occurrence-in-attempt`, minted here from the caller's
//!    [`AdmissionKeyScope`] — LR-1); the outcome is remembered per key and a
//!    repeat is answered from memory. v5's runner has no write batch at all
//!    and retries a failed job (`max_attempts` 3), so the ledger is ported
//!    whole.
//!  - **A target that vanishes is not a lost observation.** "If the row the
//!    decision folded into is retired before the write lands, admission
//!    decides once more against the index as it now stands, and inserts if
//!    the target vanishes again."
//!
//! v5 runs the parent half in place (no job child, no host-RPC — LR-4).

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Value};
use sha1::{Digest, Sha1};

use crate::admission::{
    admission_inserted, admission_thresholds_for_provider, decide_admission, fold_into,
    AdmissionAction, AdmissionCandidateView, AdmissionOutcome, AdmissionRow, AdmissionThresholds,
    AdmissionTrace, FoldSource, NeighbourAssessment, RejectReason, ADMISSION_THRESHOLDS,
    ADMISSION_TOP_K, FOLD_CAPS,
};
use crate::db::memories::TierExtra;
use crate::db::runtime::Db;
use crate::db::vector_store::Tier;
use crate::db::{js_number_to_json, DbError};
use crate::jsstr::js_trim;
use crate::memory_candidate::{to_admission_row, MemoryCandidate, ResolvedCandidateTime};
use crate::memory_embedding_text::{
    embedding_text_for, lexical_embedding_text_for, EmbeddingTextSource,
};
use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;
use crate::services::activity_kinds::ActivityKind;
use crate::services::activity_registry::track_activity;
use crate::services::memory_index;
use crate::services::memory_time_zone::refresh_memory_time_zone;

use super::apply::{apply_fold, insert_row, raise_importance, without_embedding};
use super::dry_run_book::{book_neighbours, record_book_insert, DryRunBook};
use super::links::link_both_ways;
use super::neighbours::{gather_neighbours, NeighbourProbe};
use super::prepare::{apply_episodic_fallback_anchors, apply_name_presence_check};
use super::rate_ledger::record_admission_insert;
use super::{now_ms, or_null};

/// v4's child-logger binding (`logger.child({ module: 'memory:admission' })`).
const MODULE: &str = "memory:admission";

/// Milliseconds before the one embedding retry (v4
/// `EMBEDDING_RETRY_DELAY_MS`). PORTED (LR-2): the old gate's recorded
/// omission retires with the gate. A provider rate limit can see it; no
/// differential can.
pub const EMBEDDING_RETRY_DELAY_MS: u64 = 500;

/// How long a keyed admission's outcome is remembered (covers a job's
/// retries) — v4 `ADMISSION_LEDGER_TTL_MS`.
pub const ADMISSION_LEDGER_TTL_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;
/// Most keyed outcomes kept; the oldest go first — v4 `ADMISSION_LEDGER_MAX`.
pub const ADMISSION_LEDGER_MAX: usize = 5000;

/// One job attempt's idempotency scope (LR-1) — v4's per-attempt
/// `jobScopeStore` (`child-repositories-proxy.ts`), carried rather than
/// ambient: the job handler creates one per RUN (one attempt — so a retry's
/// keys equal the first attempt's) and passes it on every admission of that
/// run; `None` outside a job. Interior-mutable, so admissions share it by
/// reference.
#[derive(Debug)]
pub struct AdmissionKeyScope {
    job_id: String,
    occurrences: Mutex<HashMap<String, u64>>,
}

impl AdmissionKeyScope {
    /// A fresh scope for one attempt of job `job_id`.
    pub fn new(job_id: impl Into<String>) -> Self {
        Self {
            job_id: job_id.into(),
            occurrences: Mutex::new(HashMap::new()),
        }
    }

    /// The job this scope belongs to.
    pub fn job_id(&self) -> &str {
        &self.job_id
    }

    /// The key for one admission in this attempt — v4 `nextAdmissionKey(
    /// contentKey)`: `jobId:contentKey:n`, where `n` counts earlier
    /// admissions of the same content in this attempt.
    pub fn next_key(&self, content_key: &str) -> String {
        let mut occurrences = self.occurrences.lock().unwrap_or_else(|p| p.into_inner());
        let n = occurrences.get(content_key).copied().unwrap_or(0);
        occurrences.insert(content_key.to_string(), n + 1);
        format!("{}:{content_key}:{n}", self.job_id)
    }
}

/// v4 `AdmitOptions` + the two things v5 carries instead of reading ambient
/// state: the job's key scope (LR-1) and the host's memory-calendar fallback
/// zone (LR-10).
pub struct AdmitOptions<'a> {
    pub user_id: &'a str,
    /// Decide and return without writing (the extraction dry run). "The
    /// candidate is still embedded and its neighbours searched, so the outcome
    /// names the action live admission would take; nothing is inserted,
    /// folded, linked or counted, and the outcome's `memory` is the row it
    /// would fold into (null for an insert)."
    pub dry_run: bool,
    /// A rebuild of history: the insert is not counted towards the extraction
    /// rate limit.
    pub backfill: bool,
    /// With `dry_run`: the dry run's scratch book — rows the regenerate would
    /// retire are searched past, and this run's would-be inserts are
    /// neighbours too.
    pub dry_run_book: Option<&'a mut DryRunBook>,
    /// The running job attempt's key scope; `None` outside a job (the API, a
    /// tool call). A dry run carries no key.
    pub key_scope: Option<&'a AdmissionKeyScope>,
    /// The host's memory-calendar fallback zone (`refresh_memory_time_zone`).
    pub host_zone_fallback: &'a str,
}

/// A stable key for what a row says and whom it is about (not where it came
/// from in time) — v4 `admissionContentKey`: SHA-1 hex of `JSON.stringify([
/// characterId, aboutCharacterId ?? null, kind ?? 'semantic', source ??
/// 'MANUAL', summary, content, sourceMessageId ?? null, occurredAt ?? null])`.
///
/// LR-7: the array holds only strings and `null`, where `serde_json`'s
/// escaping is `JSON.stringify`'s byte for byte (control characters as
/// lowercase `\u00xx` or the short escapes, everything else — U+2028 and
/// non-BMP included — raw); pinned by tier-1 rows in `admission_tier3`.
pub fn admission_content_key(row: &AdmissionRow) -> String {
    let array = json!([
        row.character_id,
        row.about_character_id,
        row.kind.as_deref().unwrap_or("semantic"),
        row.source.as_deref().unwrap_or("MANUAL"),
        row.summary,
        row.content,
        row.source_message_id,
        row.occurred_at,
    ]);
    let text = serde_json::to_string(&array).unwrap_or_default();
    hex::encode(Sha1::digest(text.as_bytes()))
}

// ---------------------------------------------------------------------------
// The per-character lock (LR-3) and the idempotency ledger (LR-1)
// ---------------------------------------------------------------------------

struct LockEntry {
    lock: Arc<tokio::sync::Mutex<()>>,
    holders: usize,
}

fn character_locks() -> &'static Mutex<HashMap<String, LockEntry>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, LockEntry>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Leaves the lock (and drops the map entry when its last holder goes — v4's
/// `if (characterLocks.get(id) === tail) delete`) on drop, so a failed or
/// cancelled admission still releases it.
struct LockRelease {
    character_id: String,
}

impl Drop for LockRelease {
    fn drop(&mut self) {
        let mut map = character_locks().lock().unwrap_or_else(|p| p.into_inner());
        if let Some(entry) = map.get_mut(&self.character_id) {
            entry.holders = entry.holders.saturating_sub(1);
            if entry.holders == 0 {
                map.remove(&self.character_id);
            }
        }
    }
}

/// Run `f` after every earlier admission for this character has finished —
/// v4 `withAdmissionLock` (`withCharacterLock`): the ONE lock, shared with
/// maintenance's near-duplicate fold. FIFO (tokio's mutex queues waiters in
/// order, as v4's promise chain does).
pub async fn with_admission_lock<F: Future>(character_id: &str, f: F) -> F::Output {
    let lock = {
        let mut map = character_locks().lock().unwrap_or_else(|p| p.into_inner());
        let entry = map
            .entry(character_id.to_string())
            .or_insert_with(|| LockEntry {
                lock: Arc::new(tokio::sync::Mutex::new(())),
                holders: 0,
            });
        entry.holders += 1;
        entry.lock.clone()
    };
    let _release = LockRelease {
        character_id: character_id.to_string(),
    };
    let _guard = lock.lock().await;
    f.await
}

struct LedgerEntry {
    outcome: AdmissionOutcome,
    at: f64,
}

/// Outcomes of keyed admissions, oldest first (v4's insertion-ordered Map).
fn admission_ledger() -> &'static Mutex<VecDeque<(String, LedgerEntry)>> {
    static LEDGER: OnceLock<Mutex<VecDeque<(String, LedgerEntry)>>> = OnceLock::new();
    LEDGER.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// v4 `recallAdmission(key, now)`: an entry older than the TTL is dropped and
/// answers nothing.
fn recall_admission(key: &str, now: f64) -> Option<AdmissionOutcome> {
    let mut ledger = admission_ledger().lock().unwrap_or_else(|p| p.into_inner());
    let index = ledger.iter().position(|(k, _)| k == key)?;
    if now - ledger[index].1.at > ADMISSION_LEDGER_TTL_MS {
        ledger.remove(index);
        return None;
    }
    Some(ledger[index].1.outcome.clone())
}

/// v4 `recordAdmission(key, outcome, now)`: `Map.set` (an existing key keeps
/// its place), then the evict loop from the oldest until the ledger is within
/// both bounds.
fn record_admission(key: String, outcome: AdmissionOutcome, now: f64) {
    let mut ledger = admission_ledger().lock().unwrap_or_else(|p| p.into_inner());
    let entry = LedgerEntry { outcome, at: now };
    match ledger.iter_mut().find(|(k, _)| *k == key) {
        Some(slot) => slot.1 = entry,
        None => ledger.push_back((key, entry)),
    }
    while let Some((_, oldest)) = ledger.front() {
        if ledger.len() <= ADMISSION_LEDGER_MAX && now - oldest.at <= ADMISSION_LEDGER_TTL_MS {
            break;
        }
        ledger.pop_front();
    }
}

/// Test hook (v4 `resetAdmissionStateForTests`): clears the lock map and the
/// ledger.
pub fn reset_admission_state_for_tests() {
    character_locks()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clear();
    admission_ledger()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clear();
}

// ---------------------------------------------------------------------------
// The entry point
// ---------------------------------------------------------------------------

/// v4 `rejected(reason, notes, attempts)` — the trace carries the NEURAL set
/// always.
fn rejected(reason: RejectReason, notes: Vec<String>, attempts: u32) -> AdmissionOutcome {
    AdmissionOutcome {
        action: AdmissionAction::Reject { reason },
        memory: None,
        novel_details: Vec::new(),
        linked_ids: Vec::new(),
        trace: AdmissionTrace {
            thresholds: ADMISSION_THRESHOLDS,
            assessments: Vec::new(),
            embedding_attempts: attempts,
            orphans_ignored: 0,
            searched_k: 0,
            notes,
        },
    }
}

/// The embedded-text view of an admission row.
fn row_text_source(row: &AdmissionRow) -> EmbeddingTextSource<'_> {
    EmbeddingTextSource {
        summary: &row.summary,
        content: &row.content,
        occurred_at: row.occurred_at.as_deref(),
        narrative_time: row.narrative_time.as_deref(),
        entities: row.entities.as_ref().into(),
    }
}

/// What [`embed_row`] produced.
struct Embedded {
    embedding: Option<Vec<f32>>,
    provider: Option<String>,
    attempts: u32,
}

/// Embed the row as it will be stored (anchor line included), one retry
/// after [`EMBEDDING_RETRY_DELAY_MS`] — v4 `embedRow`. Notes `embedding
/// attempt 1 failed (<msg>)` / `embedding failed after retry (<msg>)`; the
/// second failure logs WARN `Memory not admitted: embedding failed after
/// retry {module, characterId, userId, error}`.
async fn embed_row<P: EmbeddingProvider + Sync>(
    provider: &P,
    row: &AdmissionRow,
    time_zone: &str,
    user_id: &str,
    notes: &mut Vec<String>,
) -> Embedded {
    let source = row_text_source(row);
    let text = embedding_text_for(&source, time_zone);
    let call =
        EmbeddingCallOptions::background().with_lexical_text(lexical_embedding_text_for(&source));
    match provider
        .generate_embedding_for_user(&text, user_id, None, call.clone())
        .await
    {
        Ok(r) => {
            return Embedded {
                embedding: Some(r.embedding),
                provider: Some(r.provider),
                attempts: 1,
            }
        }
        Err(first) => notes.push(format!("embedding attempt 1 failed ({})", first.message)),
    }
    tokio::time::sleep(std::time::Duration::from_millis(EMBEDDING_RETRY_DELAY_MS)).await;
    match provider
        .generate_embedding_for_user(&text, user_id, None, call)
        .await
    {
        Ok(r) => Embedded {
            embedding: Some(r.embedding),
            provider: Some(r.provider),
            attempts: 2,
        },
        Err(second) => {
            notes.push(format!("embedding failed after retry ({})", second.message));
            tracing::warn!(
                module = MODULE,
                characterId = %row.character_id,
                userId = %user_id,
                error = %second.message,
                "Memory not admitted: embedding failed after retry"
            );
            Embedded {
                embedding: None,
                provider: None,
                attempts: 2,
            }
        }
    }
}

/// Admit one candidate (v4 `admitCandidate`). The only way an AUTO row is
/// created (invariant 4); the Memories API's hand-written rows come through
/// here too.
pub async fn admit_candidate<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    candidate: &MemoryCandidate,
    resolved: &ResolvedCandidateTime,
    opts: AdmitOptions<'_>,
) -> Result<AdmissionOutcome, DbError> {
    admit_row(db, provider, to_admission_row(candidate, resolved), opts).await
}

/// [`admit_candidate`] for a row already built from a candidate (v4
/// `admitRow`). "Memories formed outside a job (a tool call, the API) have no
/// job row to count them, so admission registers itself. Re-entrant by kind."
pub async fn admit_row<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    row: AdmissionRow,
    opts: AdmitOptions<'_>,
) -> Result<AdmissionOutcome, DbError> {
    track_activity(
        ActivityKind::Memory,
        Box::pin(admit_row_inner(db, provider, row, opts)),
    )
    .await
}

/// What the parent half needs (v4 `AdmitPreparedRequest`).
struct Prepared {
    row: AdmissionRow,
    embedding: Vec<f32>,
    /// The provider that made `embedding` (`BUILTIN` is TF-IDF), so the
    /// decision is made on that vector's cosine scale.
    embedding_provider: Option<String>,
    user_id: String,
    embedding_attempts: u32,
    notes: Vec<String>,
    backfill: bool,
}

async fn admit_row_inner<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    row: AdmissionRow,
    opts: AdmitOptions<'_>,
) -> Result<AdmissionOutcome, DbError> {
    let mut notes: Vec<String> = Vec::new();
    if js_trim(&row.content).is_empty() || js_trim(&row.summary).is_empty() {
        tracing::debug!(
            module = MODULE,
            characterId = %row.character_id,
            "Memory not admitted: empty text"
        );
        return Ok(rejected(RejectReason::Empty, notes, 0));
    }

    let time_zone = refresh_memory_time_zone(db, opts.host_zone_fallback, now_ms());
    let prepared = apply_name_presence_check(db, row, &mut notes);
    let prepared = apply_episodic_fallback_anchors(prepared, &time_zone);

    let embedded = embed_row(provider, &prepared, &time_zone, opts.user_id, &mut notes).await;
    let Some(embedding) = embedded.embedding else {
        return Ok(rejected(
            RejectReason::EmbeddingFailed,
            notes,
            embedded.attempts,
        ));
    };

    // LR-1: "A dry run writes nothing, so it carries no idempotency key."
    let key = match (opts.dry_run, opts.key_scope) {
        (false, Some(scope)) => Some(scope.next_key(&admission_content_key(&prepared))),
        _ => None,
    };
    let request = Prepared {
        row: prepared,
        embedding,
        embedding_provider: embedded.provider,
        user_id: opts.user_id.to_string(),
        embedding_attempts: embedded.attempts,
        notes,
        backfill: opts.backfill,
    };
    if opts.dry_run {
        decide_only(
            db,
            provider,
            &request,
            opts.dry_run_book,
            opts.host_zone_fallback,
        )
        .await
    } else {
        admit_prepared(db, provider, request, key, opts.host_zone_fallback).await
    }
}

/// The candidate's text, for neighbours the index cannot compare (bug 237) —
/// v4 `neighbourProbe(row, userId)`.
fn probe_text(row: &AdmissionRow) -> String {
    format!("{}\n{}", row.summary, row.content)
}

fn candidate_view(row: &AdmissionRow) -> AdmissionCandidateView<'_> {
    AdmissionCandidateView {
        holder_id: &row.character_id,
        about_id: row.about_character_id.as_deref(),
        kind: row.kind.as_deref(),
        source: row.source.as_deref(),
        occurred_at: row.occurred_at.as_deref(),
    }
}

fn id_of(memory: &Value) -> &str {
    memory.get("id").and_then(Value::as_str).unwrap_or_default()
}

/// The trace minus its notes (v4's trace holds the SAME `notes` array the
/// apply step keeps pushing to, so the notes are joined at the end).
struct TraceHead {
    thresholds: AdmissionThresholds,
    assessments: Vec<NeighbourAssessment>,
    embedding_attempts: u32,
    orphans_ignored: u32,
    searched_k: u32,
}

/// The parent half: search, decide, write — v4 `admitPrepared`, under the
/// per-character lock. NO-PORT (LR-4): v4's job-child refusal and its
/// `patchChildIndex`.
async fn admit_prepared<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    request: Prepared,
    idempotency_key: Option<String>,
    host_zone_fallback: &str,
) -> Result<AdmissionOutcome, DbError> {
    let character_id = request.row.character_id.clone();
    let outcome = with_admission_lock(&character_id, async {
        let now = now_ms();
        if let Some(key) = &idempotency_key {
            if let Some(mut earlier) = recall_admission(key, now) {
                tracing::debug!(
                    module = MODULE,
                    characterId = %character_id,
                    action = earlier.action.kind().as_str(),
                    memoryId = %or_null(earlier.memory.as_ref().map(id_of)),
                    "Admission already made by an earlier attempt of this job; nothing written"
                );
                earlier
                    .trace
                    .notes
                    .push("already admitted by an earlier attempt of this job".to_string());
                return Ok(earlier);
            }
        }

        let row = &request.row;
        let mut notes = request.notes.clone();
        let mut head: Option<TraceHead> = None;
        let mut applied: Option<Applied> = None;
        let mut neighbour_count = 0usize;
        let mut best_cosine: Option<f64> = None;
        let thresholds = admission_thresholds_for_provider(request.embedding_provider.as_deref());
        let text = probe_text(row);
        let probe = NeighbourProbe {
            text: &text,
            user_id: &request.user_id,
        };

        for attempt in 0..2 {
            if applied.is_some() {
                break;
            }
            let gathered = gather_neighbours(
                db,
                provider,
                &character_id,
                &request.embedding,
                None,
                Some(&probe),
                host_zone_fallback,
            )
            .await?;
            let decision = decide_admission(&candidate_view(row), &gathered.neighbours, thresholds);
            head = Some(TraceHead {
                thresholds: *thresholds,
                assessments: decision.assessments.clone(),
                embedding_attempts: request.embedding_attempts,
                orphans_ignored: gathered.orphan_ids.len() as u32,
                searched_k: gathered.searched_k as u32,
            });
            neighbour_count = gathered.neighbours.len();
            best_cosine = gathered.neighbours.first().map(|n| n.cosine);
            let by_id: HashMap<String, Value> = gathered
                .neighbours
                .iter()
                .map(|n| (id_of(&n.memory).to_string(), n.memory.clone()))
                .collect();

            let kind = decision.action.kind();
            let result = apply_action(
                db,
                provider,
                decision.action,
                row,
                &request.embedding,
                &by_id,
                &request.user_id,
                &mut notes,
                host_zone_fallback,
            )
            .await?;
            match result {
                Some(done) => applied = Some(done),
                None if attempt == 0 => {
                    notes.push("target retired before the write; deciding again".to_string());
                    tracing::debug!(
                        module = MODULE,
                        characterId = %character_id,
                        action = kind.as_str(),
                        "Admission target vanished; deciding again"
                    );
                }
                None => {
                    notes.push("target retired again; inserted instead".to_string());
                    tracing::warn!(
                        module = MODULE,
                        characterId = %character_id,
                        "Admission target vanished twice; inserting the candidate"
                    );
                    applied = apply_action(
                        db,
                        provider,
                        AdmissionAction::Insert,
                        row,
                        &request.embedding,
                        &HashMap::new(),
                        &request.user_id,
                        &mut notes,
                        host_zone_fallback,
                    )
                    .await?;
                }
            }
        }

        let applied = applied.ok_or_else(|| {
            DbError::Internal("Admission applied no action (an insert never vanishes)".into())
        })?;
        let head = head.ok_or_else(|| DbError::Internal("Admission made no decision".into()))?;
        let outcome = AdmissionOutcome {
            action: applied.action,
            memory: applied.memory,
            novel_details: applied.novel_details,
            linked_ids: applied.linked_ids,
            trace: AdmissionTrace {
                thresholds: head.thresholds,
                assessments: head.assessments,
                embedding_attempts: head.embedding_attempts,
                orphans_ignored: head.orphans_ignored,
                searched_k: head.searched_k,
                notes,
            },
        };

        // R-AF: the hint goes out on the caller thread once the last write
        // has returned.
        if outcome.memory.is_some() {
            publish_realtime(RealtimeTopic::Memories, None);
        }
        if admission_inserted(&outcome.action)
            && row.source.as_deref() == Some("AUTO")
            && !request.backfill
        {
            record_admission_insert(&character_id, now_ms());
        }
        let keyed = idempotency_key.is_some();
        if let Some(key) = idempotency_key {
            record_admission(key, outcome.clone(), now);
        }

        let scale = if *thresholds == ADMISSION_THRESHOLDS {
            "neural"
        } else {
            "tfidf"
        };
        tracing::debug!(
            module = MODULE,
            characterId = %character_id,
            action = outcome.action.kind().as_str(),
            memoryId = %or_null(outcome.memory.as_ref().map(id_of)),
            neighbours = neighbour_count,
            bestCosine = %best_cosine.map_or("null".to_string(), |c| c.to_string()),
            novelDetails = outcome.novel_details.len(),
            linked = outcome.linked_ids.len(),
            keyed = keyed,
            thresholdScale = scale,
            "Memory admitted"
        );
        Ok::<AdmissionOutcome, DbError>(outcome)
    })
    .await?;
    if admission_inserted(&outcome.action) {
        // The maintenance watermarks (the cap and consolidation's backlog)
        // count the row just written; the ledger they consult lives here in
        // the parent. v4 `admit.ts:405-409` fires it `void` after `Memory
        // admitted`, outside the lock, with a catch DEBUG `Maintenance
        // watermark check skipped after admission` for a failed dynamic
        // import; `maybe_enqueue_maintenance` never fails (its own WARN covers
        // the check), so that arm has no v5 twin. Landed at the `83334b9a5`
        // unification (§R.10(h), §S.5 (1)).
        let _enqueued = crate::services::memory_maintenance::triggers::maybe_enqueue_maintenance(
            db,
            std::slice::from_ref(&character_id),
            now_ms(),
        )
        .await;
    }
    Ok(outcome)
}

/// The dry run's parent half: search and decide as live admission would, and
/// write nothing — v4 `decideOnly`. The outcome's `memory` is the row the
/// action names (the fold target, without its vector), or `None` for an
/// insert. DEBUG `Admission decided (dry run; nothing written) {module,
/// characterId, action, targetId, neighbours}`.
async fn decide_only<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    request: &Prepared,
    book: Option<&mut DryRunBook>,
    host_zone_fallback: &str,
) -> Result<AdmissionOutcome, DbError> {
    let row = &request.row;
    let thresholds = admission_thresholds_for_provider(request.embedding_provider.as_deref());
    let text = probe_text(row);
    let probe = NeighbourProbe {
        text: &text,
        user_id: &request.user_id,
    };
    let mut gathered = gather_neighbours(
        db,
        provider,
        &row.character_id,
        &request.embedding,
        book.as_deref().map(|b| &b.retiring),
        Some(&probe),
        host_zone_fallback,
    )
    .await?;
    if let Some(book) = book.as_deref() {
        let mut merged = std::mem::take(&mut gathered.neighbours);
        merged.extend(book_neighbours(book, &row.character_id, &request.embedding));
        js_array_sort_by(&mut merged, |a, b| b.cosine - a.cosine);
        merged.truncate(ADMISSION_TOP_K);
        gathered.neighbours = merged;
    }
    let decision = decide_admission(&candidate_view(row), &gathered.neighbours, thresholds);
    let action = decision.action;
    let target = action.into_id().and_then(|into| {
        gathered
            .neighbours
            .iter()
            .find(|n| id_of(&n.memory) == into)
            .map(|n| n.memory.clone())
    });
    if let Some(book) = book {
        if matches!(
            action,
            AdmissionAction::Insert | AdmissionAction::InsertLinked { .. }
        ) {
            record_book_insert(book, row, &request.embedding);
        }
    }
    tracing::debug!(
        module = MODULE,
        characterId = %row.character_id,
        action = action.kind().as_str(),
        targetId = %or_null(target.as_ref().map(id_of)),
        neighbours = gathered.neighbours.len(),
        "Admission decided (dry run; nothing written)"
    );
    let linked_ids = match &action {
        AdmissionAction::InsertLinked { link_to, .. } => link_to.clone(),
        _ => Vec::new(),
    };
    let mut notes = request.notes.clone();
    notes.push("dry run: decided, nothing written".to_string());
    Ok(AdmissionOutcome {
        action,
        memory: target.map(without_embedding),
        novel_details: Vec::new(),
        linked_ids,
        trace: AdmissionTrace {
            thresholds: *thresholds,
            assessments: decision.assessments,
            embedding_attempts: request.embedding_attempts,
            orphans_ignored: gathered.orphan_ids.len() as u32,
            searched_k: gathered.searched_k as u32,
            notes,
        },
    })
}

/// One applied decision (v4 `Applied`, minus the child-cache entries —
/// LR-4).
struct Applied {
    action: AdmissionAction,
    memory: Option<Value>,
    novel_details: Vec<String>,
    linked_ids: Vec<String>,
}

/// `{ ...target, ...patch }` on a row's JSON — each key at its `MemorySchema`
/// position (`apply::with_key`: v4's parsed row carries a NULL column's key
/// `undefined`, so its spread lands `lastReinforcedAt` in place where a `Map`
/// would append it — the `83334b9a5` unification's `create_reinforce_200`).
fn merged(mut memory: Value, fields: &[(&str, Value)]) -> Value {
    for (k, v) in fields {
        memory = super::apply::with_key(memory, k, v.clone());
    }
    memory
}

/// Write one decision — v4 `applyAction`. `Ok(None)` is v4's `'vanished'`:
/// the row it folds into is gone before the write lands (retired meanwhile);
/// the caller decides again.
#[allow(clippy::too_many_arguments)]
async fn apply_action<P: EmbeddingProvider + Sync>(
    db: &Db,
    provider: &P,
    action: AdmissionAction,
    row: &AdmissionRow,
    embedding: &[f32],
    neighbours: &HashMap<String, Value>,
    user_id: &str,
    notes: &mut Vec<String>,
    host_zone_fallback: &str,
) -> Result<Option<Applied>, DbError> {
    let into = match &action {
        AdmissionAction::Reject { .. } => {
            return Ok(Some(Applied {
                action,
                memory: None,
                novel_details: Vec::new(),
                linked_ids: Vec::new(),
            }))
        }
        AdmissionAction::Insert => {
            let memory = insert_row(db, row, embedding).await?;
            return Ok(Some(Applied {
                action,
                memory: Some(without_embedding(memory)),
                novel_details: Vec::new(),
                linked_ids: Vec::new(),
            }));
        }
        AdmissionAction::InsertLinked { link_to, .. } => {
            let memory = insert_row(db, row, embedding).await?;
            let linked_ids = link_both_ways(db, &row.character_id, id_of(&memory), link_to).await?;
            // The post-link row: callers that union `relatedMemoryIds` must
            // start from the links written here (bug 26).
            let memory = merged(memory, &[("relatedMemoryIds", json!(linked_ids))]);
            return Ok(Some(Applied {
                action,
                memory: Some(without_embedding(memory)),
                novel_details: Vec::new(),
                linked_ids,
            }));
        }
        AdmissionAction::Absorb { into, .. }
        | AdmissionAction::Reinforce { into }
        | AdmissionAction::PromoteAndReinforce { into, .. } => into.clone(),
    };

    // The decider only names neighbours it was shown.
    let Some(mut target) = neighbours.get(&into).cloned() else {
        return Err(DbError::Internal(format!(
            "Admission target {into} is not among the gathered neighbours"
        )));
    };
    let target_character = target
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if matches!(action, AdmissionAction::PromoteAndReinforce { .. }) {
        // The world asked for a cap-demoted row again: back to the hot tier.
        // Retired meanwhile, the tier move touches nothing.
        let (cid, ids) = (target_character.clone(), vec![into.clone()]);
        let moved = db
            .write(move |w| {
                memory_index::set_tier(
                    w.main().connection(),
                    &cid,
                    &ids,
                    Tier::Hot,
                    TierExtra {
                        superseded_by_id: Some(None),
                        ..Default::default()
                    },
                )
            })
            .await?;
        if moved == 0 {
            return Ok(None);
        }
        target = merged(
            target,
            &[("tier", json!("hot")), ("supersededById", Value::Null)],
        );
        notes.push(format!("promoted cold row {into} to hot"));
    }

    let with_footnotes = matches!(action, AdmissionAction::Reinforce { .. })
        || matches!(
            action,
            AdmissionAction::PromoteAndReinforce {
                with_footnotes: true,
                ..
            }
        );
    let mut novel_details = Vec::new();
    let memory = if with_footnotes {
        // The one combine: the candidate is a source of one observation.
        let plan = fold_into(
            &target,
            &[FoldSource {
                content: row.content.clone(),
                occurred_at: row.occurred_at.clone(),
                narrative_time: row.narrative_time.clone(),
                entities: row.entities.clone(),
                reinforcement_count: Some(1.0),
                last_reinforced_at: Some(crate::clock::now_iso()),
                ..Default::default()
            }],
            &FOLD_CAPS,
        );
        let memory = apply_fold(
            db,
            Some(provider),
            &target,
            &plan,
            Some(user_id),
            host_zone_fallback,
        )
        .await?;
        if memory.is_some() {
            novel_details = plan.footnotes;
        }
        memory
    } else {
        // Count only: a near-duplicate's text and vector stay as they are.
        let (cid, mid, at) = (
            target_character.clone(),
            into.clone(),
            crate::clock::now_iso(),
        );
        let counted = db
            .write(move |w| {
                w.main()
                    .memories()
                    .increment_reinforcement(&cid, &mid, &at, 1)
            })
            .await?;
        counted.map(|c| {
            merged(
                target.clone(),
                &[
                    (
                        "reinforcementCount",
                        js_number_to_json(c.reinforcement_count),
                    ),
                    (
                        "reinforcedImportance",
                        js_number_to_json(c.reinforced_importance),
                    ),
                    ("lastReinforcedAt", Value::String(c.last_reinforced_at)),
                ],
            )
        })
    };
    let Some(mut memory) = memory else {
        tracing::debug!(
            module = MODULE,
            memoryId = %into,
            "Admission target vanished before the write"
        );
        return Ok(None);
    };
    if row.source.as_deref() == Some("MANUAL") {
        memory = raise_importance(db, memory, row.importance).await?;
    }
    Ok(Some(Applied {
        action,
        memory: Some(without_embedding(memory)),
        novel_details,
        linked_ids: Vec::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_mints_job_content_occurrence_keys() {
        let scope = AdmissionKeyScope::new("job-1");
        assert_eq!(scope.next_key("abc"), "job-1:abc:0");
        assert_eq!(scope.next_key("abc"), "job-1:abc:1");
        assert_eq!(scope.next_key("def"), "job-1:def:0");
        // A retry is a fresh scope: the same keys again.
        let retry = AdmissionKeyScope::new("job-1");
        assert_eq!(retry.next_key("abc"), "job-1:abc:0");
    }

    #[test]
    fn the_content_key_is_sha1_over_v4s_eight_element_array() {
        let row = AdmissionRow {
            character_id: "c".into(),
            content: "content".into(),
            summary: "summary".into(),
            ..Default::default()
        };
        // sha1('["c",null,"semantic","MANUAL","summary","content",null,null]')
        let text = r#"["c",null,"semantic","MANUAL","summary","content",null,null]"#;
        assert_eq!(
            admission_content_key(&row),
            hex::encode(Sha1::digest(text.as_bytes()))
        );
    }
}
