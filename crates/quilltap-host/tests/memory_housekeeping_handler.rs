//! P4.D266 — the `MEMORY_HOUSEKEEPING` job handler (`spine.rs`
//! `MemoryHousekeepingHandler`, v4 `lib/background-jobs/handlers/
//! memory-housekeeping.ts` at `01a83539d`) and the retire path's line per
//! path (P4.D288).
//!
//! - **The outcome cache now measures `demoted + deleted`** (v4 `d58548051`):
//!   a demote-only sweep is EFFECTIVE, so the watermark backoff no longer
//!   misfires after a sweep that moved ten rows to the cold tier and deleted
//!   none.
//! - **v4's four handler lines** — the enumerate-failure ERROR, the per-character
//!   `Completed sweep for character` INFO, the per-character `Sweep failed for
//!   character` ERROR (the sweep continues), and the closing `Job complete`
//!   INFO — with their keys in v4's order, and silence legs.
//! - **The retire path (P4.D288, C1-7):** the retention delete and dedup's
//!   fold both run through `memory_retire` — `Retired memories` with v4's
//!   reason (the F8 `countSource` label went with v4's
//!   `deleteMemoriesWithUnlinkBatch`, `af48f6cc2`).
//!
//! The batch-delete DEBUG is emitted INSIDE a `Db::write` closure, on the
//! writer THREAD, which neither of core's thread-scoped capture rigs can see
//! (memory note `capture-rig-writer-thread`). This binary therefore owns its
//! global subscriber outright: one PROCESS-wide capturing layer that records
//! every thread's events while a test holds the capture lock, so the
//! writer-thread line is captured too. The tests serialize on that lock.

use std::sync::{Mutex, Once};

use quilltap_core::db::background_jobs::BackgroundJob;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::housekeeping_outcome_cache::should_skip_watermark_sweep;
use quilltap_core::services::job_runner::{JobHandler, JobOutcome};
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_host::spine::MemoryHousekeepingHandler;
use serde_json::{json, Value};
use tempfile::TempDir;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";
const OLD: &str = "2020-01-01T00:00:00.000Z";

// ---------------------------------------------------------------------------
// The process-global capture (every thread, while armed).
// ---------------------------------------------------------------------------

static LINES: Mutex<Option<Vec<String>>> = Mutex::new(None);
/// Async-aware: held across the handler's awaits.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct GlobalLayer;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for GlobalLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut guard = LINES.lock().unwrap_or_else(|p| p.into_inner());
        let Some(buf) = guard.as_mut() else {
            return;
        };
        let meta = event.metadata();
        let mut visitor = quilltap_core::test_support::FieldVisitor(format!(
            "{} {}",
            meta.level(),
            meta.target()
        ));
        event.record(&mut visitor);
        buf.push(visitor.0);
    }
}

fn install() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        use tracing_subscriber::layer::SubscriberExt;
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(GlobalLayer))
            .expect("this binary owns its global subscriber");
    });
}

/// Serialize the test and arm the global buffer.
async fn arm() -> tokio::sync::MutexGuard<'static, ()> {
    install();
    let serial = SERIAL.lock().await;
    *LINES.lock().unwrap() = Some(Vec::new());
    serial
}

fn disarm() -> Vec<String> {
    LINES.lock().unwrap().take().unwrap_or_default()
}

fn only_line(lines: &[String], needle: &str) -> String {
    let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
    assert_eq!(hits.len(), 1, "expected exactly one {needle:?}: {lines:#?}");
    hits[0].clone()
}

// ---------------------------------------------------------------------------
// A real provisioned instance + planted memories.
// ---------------------------------------------------------------------------

fn instance() -> (TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    provision_fresh_instance(dir.path(), PEPPER).expect("provision a fresh instance");
    let db = Db::open(
        DbPaths {
            main: dir.path().join("quilltap.db"),
            mount_index: Some(dir.path().join("quilltap-mount-index.db")),
            llm_logs: Some(dir.path().join("quilltap-llm-logs.db")),
        },
        PEPPER,
    )
    .expect("open the instance");
    (dir, db)
}

/// A main-only `Db` (no mount-index partition) — enumerating characters fails.
fn main_only() -> (TempDir, Db) {
    let (dir, _db) = instance();
    drop(_db);
    let db = Db::open_main(dir.path().join("quilltap.db"), PEPPER).unwrap();
    (dir, db)
}

struct Plant {
    id: String,
    importance: f64,
    tier: &'static str,
    superseded_by: Option<&'static str>,
}

fn hot_low(id: String) -> Plant {
    Plant {
        id,
        importance: 0.1,
        tier: "hot",
        superseded_by: None,
    }
}

async fn plant(db: &Db, character_id: &str, rows: Vec<Plant>) {
    let cid = character_id.to_string();
    db.write(move |w| {
        for p in &rows {
            w.main().connection().execute(
                "INSERT INTO memories (id, characterId, content, summary, keywords, tags, \
                 importance, source, reinforcementCount, relatedMemoryIds, reinforcedImportance, \
                 tier, supersededById, consolidatedFrom, entities, kind, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'c', 's', '[]', '[]', ?3, 'AUTO', 1, '[]', ?3, ?4, ?5, '[]', \
                 '[]', 'semantic', ?6, ?6)",
                rusqlite::params![p.id, cid, p.importance, p.tier, p.superseded_by, OLD],
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
}

async fn set_retention(db: &Db, days: i64) {
    db.write(move |w| {
        quilltap_core::db::instance_settings::set_memory_consolidation_settings(
            w.main().connection(),
            &json!({ "coldRetentionDays": days }),
        )?;
        Ok(())
    })
    .await
    .unwrap();
}

fn job(payload: Value) -> BackgroundJob {
    BackgroundJob {
        id: "a0000000-0000-4000-8000-0000000000d1".into(),
        user_id: USER.into(),
        job_type: "MEMORY_HOUSEKEEPING".into(),
        status: "PROCESSING".into(),
        payload: payload.to_string(),
        priority: 0.0,
        attempts: 1.0,
        max_attempts: 3.0,
        last_error: None,
        scheduled_at: OLD.into(),
        started_at: None,
        completed_at: None,
        created_at: OLD.into(),
        updated_at: OLD.into(),
    }
}

fn uuid(prefix: &str, n: usize) -> String {
    format!("{prefix}000000-0000-4000-8000-{n:012}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// v4 `d58548051` (`memory-housekeeping.ts:100-107`): the outcome cache records
/// `demoted + deleted`. Twelve pass-1 demotions and zero deletions is an
/// EFFECTIVE sweep — the watermark backoff stays disarmed (on the old
/// `deleted`-only record it would have armed for an hour).
#[tokio::test]
async fn a_demote_only_sweep_records_an_effective_outcome() {
    let _serial = arm().await;
    let (_d, db) = instance();
    let cid = uuid("a1", 1);
    plant(&db, &cid, (0..12).map(|i| hot_low(uuid("b1", i))).collect()).await;
    let outcome = MemoryHousekeepingHandler
        .handle(&db, &job(json!({ "characterId": cid, "reason": "manual" })))
        .await;
    let lines = disarm();
    assert!(matches!(outcome, JobOutcome::Completed(None)));
    assert!(
        !should_skip_watermark_sweep(&cid),
        "a demote-only sweep of 12 is effective: {lines:#?}"
    );
    let line = only_line(&lines, "[Housekeeping] Completed sweep for character");
    assert!(line.starts_with("INFO "), "{line:?}");
    assert!(
        line.contains(&format!(
            " jobId=a0000000-0000-4000-8000-0000000000d1 userId={USER} characterId={cid} \
             reason=manual dryRun=false totalBefore=12 totalAfter=0 demoted=12 deleted=0 \
             cold=0 merged=0 kept=0"
        )),
        "v4's keys in v4's order: {line:?}"
    );
    let done = only_line(&lines, "[Housekeeping] Job complete");
    assert!(
        done.contains(&format!(
            " jobId=a0000000-0000-4000-8000-0000000000d1 userId={USER} charactersSwept=1 \
             totalDemoted=12 totalDeleted=0 totalMerged=0 reason=manual"
        )),
        "{done:?}"
    );
    for silent in [
        "Sweep failed for character",
        "Failed to enumerate characters",
    ] {
        assert!(
            !lines.iter().any(|l| l.contains(silent)),
            "{silent}: {lines:#?}"
        );
    }
}

/// A reason-less payload logs `reason=unknown` (v4 `payload.reason ??
/// 'unknown'`); a dry run records no outcome and logs `dryRun=true`.
#[tokio::test]
async fn a_dry_run_records_nothing_and_logs_unknown_reason() {
    let _serial = arm().await;
    let (_d, db) = instance();
    let cid = uuid("a2", 1);
    plant(&db, &cid, (0..3).map(|i| hot_low(uuid("b2", i))).collect()).await;
    MemoryHousekeepingHandler
        .handle(&db, &job(json!({ "characterId": cid, "dryRun": true })))
        .await;
    let lines = disarm();
    let line = only_line(&lines, "[Housekeeping] Completed sweep for character");
    assert!(line.contains(" reason=unknown dryRun=true "), "{line:?}");
    assert!(
        !should_skip_watermark_sweep(&cid),
        "a dry run records no outcome at all"
    );
}

/// v4 `:44-49`: a character enumeration that fails logs the ERROR and RETURNS
/// (a completed no-op — no `Job complete` line).
#[tokio::test]
async fn a_failed_enumeration_logs_and_returns() {
    let _serial = arm().await;
    let (_d, db) = main_only();
    let outcome = MemoryHousekeepingHandler
        .handle(&db, &job(json!({ "reason": "manual" })))
        .await;
    let lines = disarm();
    assert!(matches!(outcome, JobOutcome::Completed(None)));
    let line = only_line(
        &lines,
        "[Housekeeping] Failed to enumerate characters for user",
    );
    assert!(line.starts_with("ERROR "), "{line:?}");
    assert!(
        line.contains(&format!(
            " jobId=a0000000-0000-4000-8000-0000000000d1 userId={USER} error="
        )),
        "{line:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("Job complete")),
        "{lines:#?}"
    );
}

/// v4 `:123-131` + R-J: a character whose sweep throws (an out-of-range
/// retention cutoff → `Invalid time value`) logs `Sweep failed for character`
/// and the job continues to `Job complete`.
#[tokio::test]
async fn a_failed_character_sweep_logs_and_continues() {
    let _serial = arm().await;
    let (_d, db) = instance();
    let cid = uuid("a3", 1);
    plant(&db, &cid, vec![hot_low(uuid("b3", 0))]).await;
    set_retention(&db, 200_000_000).await;
    let outcome = MemoryHousekeepingHandler
        .handle(&db, &job(json!({ "characterId": cid, "reason": "manual" })))
        .await;
    let lines = disarm();
    assert!(matches!(outcome, JobOutcome::Completed(None)));
    let line = only_line(&lines, "[Housekeeping] Sweep failed for character");
    assert!(line.starts_with("ERROR "), "{line:?}");
    assert!(
        line.contains(&format!(
            " jobId=a0000000-0000-4000-8000-0000000000d1 userId={USER} characterId={cid} \
             error=Invalid time value"
        )),
        "{line:?}"
    );
    only_line(&lines, "[Housekeeping] Job complete");
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Completed sweep for character")),
        "{lines:#?}"
    );
}

/// P4.D288 (C1-7): the retention delete on BOTH paths runs through the ONE
/// retire path — `Retired memories` with `reason=retention` (emitted on the
/// writer thread — this binary's global rig sees it). The F8 `countSource`
/// label went with v4's `deleteMemoriesWithUnlinkBatch` (`af48f6cc2`).
#[tokio::test]
async fn both_retention_paths_retire_through_the_one_path() {
    let _serial = arm().await;
    let (_d, db) = instance();
    let cid = uuid("a4", 1);
    plant(
        &db,
        &cid,
        vec![Plant {
            id: uuid("b4", 0),
            importance: 0.1,
            tier: "cold",
            superseded_by: Some("c4000000-0000-4000-8000-000000000001"),
        }],
    )
    .await;
    set_retention(&db, 90).await;
    MemoryHousekeepingHandler
        .handle(&db, &job(json!({ "characterId": cid, "reason": "manual" })))
        .await;
    let lines = disarm();
    let batch = only_line(&lines, " Retired memories ");
    assert!(
        batch.contains(" module=memory:retire reason=retention keep=null requested=1 retired=1 "),
        "the job path retires through the one path: {batch:?}"
    );

    // The Memories API path, over a second character planted the same way.
    let _ = &_serial;
    *LINES.lock().unwrap() = Some(Vec::new());
    let cid2 = uuid("a5", 1);
    plant(
        &db,
        &cid2,
        vec![Plant {
            id: uuid("b5", 0),
            importance: 0.1,
            tier: "cold",
            superseded_by: Some("c5000000-0000-4000-8000-000000000001"),
        }],
    )
    .await;
    let r = quilltap_core::services::housekeeping::run_housekeeping(
        &db,
        &cid2,
        &quilltap_core::services::housekeeping::HousekeepingOptions {
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let lines = disarm();
    assert_eq!(r.deleted, 1);
    let batch = only_line(&lines, " Retired memories ");
    assert!(
        batch.contains(" module=memory:retire reason=retention keep=null requested=1 retired=1 "),
        "the API path retires through the one path: {batch:?}"
    );
}

/// P4.D288 (C1-7): dedup retires its folded rows through the ONE retire path
/// (`reason=near-duplicate-fold`); and the all-characters driver logs v4's two INFO
/// lines (`Starting deduplication for all characters`, `Deduplication
/// complete`) with v4's keys, plus the per-character `Bulk deleted memories`.
#[tokio::test]
async fn dedup_retires_through_the_one_path_and_logs_its_info_lines() {
    let _serial = arm().await;
    let (_d, db) = instance();
    let created = quilltap_core::api::characters::character_create(
        &db,
        quilltap_core::services::provisioning::SINGLE_USER_ID,
        json!({ "name": "Dedupe" }),
    )
    .await;
    let created = serde_json::to_value(&created).unwrap();
    let cid = created["data"]["character"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("character create answered {created}"))
        .to_string();
    // Two memories on one vector — one cluster, one loser.
    let cid_w = cid.clone();
    db.write(move |w| {
        for (id, content, importance) in [
            (
                uuid("b6", 0),
                "the survivor row with the longest words.",
                0.9,
            ),
            (uuid("b6", 1), "the loser row.", 0.2),
        ] {
            w.main().memories().create(
                &quilltap_core::db::memories::MemCreate {
                    character_id: cid_w.clone(),
                    about_character_id: None,
                    chat_id: None,
                    project_id: None,
                    content: content.into(),
                    summary: "s".into(),
                    keywords: vec![],
                    tags: vec![],
                    importance,
                    embedding: Some(vec![1.0, 0.0]),
                    source: "AUTO".into(),
                    witnessed_context: None,
                    occurred_at: None,
                    narrative_time: None,
                    entities: vec![],
                    kind: "semantic".into(),
                    source_message_id: None,
                    last_accessed_at: None,
                    reinforcement_count: 1.0,
                    last_reinforced_at: None,
                    related_memory_ids: vec![],
                    reinforced_importance: importance,
                    tier: "hot".into(),
                    superseded_by_id: None,
                    superseded_at: None,
                    consolidated_from: vec![],
                    consolidated_at: None,
                },
                &quilltap_core::db::memories::CreateOptions {
                    id: id.clone(),
                    created_at: OLD.into(),
                    updated_at: OLD.into(),
                },
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    *LINES.lock().unwrap() = Some(Vec::new());
    let result = quilltap_core::services::memory_dedup::deduplicate_all_memories(
        &db,
        None::<&quilltap_core::model::embedding::CannedEmbeddingProvider>,
        quilltap_core::services::provisioning::SINGLE_USER_ID,
        0.8,
        false,
    )
    .await
    .unwrap();
    let lines = disarm();
    assert_eq!(result["totalRemoved"], 1, "{result}");
    let batch = only_line(&lines, " Retired memories ");
    assert!(
        batch.contains(
            " module=memory:retire reason=near-duplicate-fold keep=null requested=1 retired=1 "
        ),
        "dedup retires its folded rows through the one path: {batch:?}"
    );
    let start = only_line(
        &lines,
        "[MemoryDedup] Starting deduplication for all characters",
    );
    assert!(start.starts_with("INFO "), "{start:?}");
    assert!(
        start.contains(
            " context=memory-dedup.deduplicateAllMemories userId=ffffffff-ffff-ffff-ffff-ffffffffffff \
             threshold=0.8 dryRun=false"
        ),
        "{start:?}"
    );
    let done = only_line(&lines, "[MemoryDedup] Deduplication complete");
    assert!(
        done.contains(
            " context=memory-dedup.deduplicateAllMemories userId=ffffffff-ffff-ffff-ffff-ffffffffffff \
             characterCount=1 totalOriginal=2 totalRemoved=1 totalMergedDetails=0 totalFinal=1 \
             dryRun=false"
        ),
        "{done:?}"
    );
    let deleted = only_line(&lines, "[MemoryDedup] Bulk deleted memories");
    assert!(
        deleted.contains(&format!(
            " context=memory-dedup.deduplicateCharacterMemories characterId={cid} requested=1 \
             deleted=1"
        )),
        "{deleted:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Failed to deduplicate character")),
        "{lines:#?}"
    );
}
