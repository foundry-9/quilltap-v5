//! Tier-3 differential — the embedding-family remainder (P4.6BM): v4's
//! `handleConversationRender` (`lib/background-jobs/handlers/conversation-render.ts`)
//! against `quilltap_core::services::conversation_render_job`.
//!
//! Both sides copy the same committed two-DB fixture
//! (`crates/quilltap-web/tests/fixtures/embedding-remainder-{main,mount}.db`),
//! run the same corpus phases in the same order with state accumulating, and
//! then diff EIGHT tables: `chats`, `conversation_chunks`, `background_jobs`,
//! `embedding_status`, `memories`, `help_docs`, `vector_entries` and
//! `vector_indices`.
//!
//! ## P4.D25 — the reconcile's staleness gate + FAILED-status exclusion
//!
//! Phase 1 now exercises v4 `a0243abd`/`a5d6cee5`: the scan carries a
//! `NOT EXISTS … embedding_status … FAILED` clause bound to the resolved default
//! profile, and every selected row goes through the shared `isStale` gate. The
//! corpus grew FOUR chats whose activity sits INSIDE the retention window
//! (everything older was, by design, stale relative to `renderNowIso`): a fresh
//! arm-(A) chat (enqueued), a fresh arm-(A) chat with a PENDING render already
//! seeded (reused), a fresh chat whose only un-embedded chunk is FAILED for the
//! DEFAULT profile — now inflated OVER the per-chunk budget so v4 Bug 17's arm
//! (C) reclaims it (a re-render sub-chunks it); with no played messages, so it
//! exercises the gate's `chats.updatedAt` fallback — and its twin whose FAILED
//! row names the OTHER profile (arm (B), still selected). The oracle reports
//! `{incompleteChats: 5, enqueued: 3, reused: 1, skippedStale: 1}` — arm (C) is
//! the +1 over the pre-Bug-17 corpus.
//!
//! Two fail-soft arms are NOT reachable from this corpus and are covered by the
//! module's own unit tests instead: the no-profile sentinel (`''`, which matches
//! no rows) and the profile-resolution-threw arm that keeps it — reaching either
//! here would mean emptying `embedding_profiles` mid-run, which the later
//! reindex phases depend on.
//!
//! Nothing about this family is model-dependent — no LLM, no embedding provider —
//! so the oracle drives v4's real handler with **only** `ensureProcessorRunning`
//! stubbed (in-process it forks v4's job child, which would claim the very jobs
//! under measurement). The one non-determinism, the wall clock, is frozen on the
//! oracle side (`globalThis.Date`) and injected on this side, which is what lets
//! `chats.renderedMarkdown` — a string carrying a `Current time:` line — compare
//! **byte-exact**.
//!
//! ## P4.d27 — the reindex catch-up (v4 `7391404e`)
//!
//! Phase 3 now covers v4's new exclusions and its NEW phase 4. The corpus gained
//! the non-conforming half v4's outage was actually made of: memories, chunks,
//! help-doc and mount-chunk vectors written at a LEGACY width in BOTH on-disk
//! formats (headerless raw Float32 and the quantized header — they take different
//! `EMBEDDING_DIM_SQL` branches), two explicit mount points (one enabled, one
//! disabled) with five chunks between them, FAILED `embedding_status` rows for all
//! four entity types, a memory whose `characterId` has no `characters` row, and a
//! memory under a character owned by the SECOND user.
//!
//! Those last two are the fan-out proof: v4 abandoned `characters.findByUserId`
//! (which silently drops a character whose vault is unavailable, and is
//! user-scoped) for `memories.findDistinctCharacterIds`. Neither memory can be
//! reached by the old enumeration.
//!
//! The arms are asserted by name from the committed spec's `reindexShapeGuards`
//! against the ORACLE's own `background_jobs` dump, before the row diff — the diff
//! proves v5 agrees with v4, the guards prove the corpus still asks.
//!
//! ## P4.d27 — the boot dimension reconcile (phase 2b)
//!
//! v4's NEW startup self-heal runs as its own phase, six cases deep, each with a
//! one-UPDATE prelude choosing which profile is `isDefault`. It sits between the
//! renders and the reindexes, and the placement is load-bearing: run it AFTER the
//! reindex phase and its DELETE and meta-snap arms both report zero, because full
//! scope has already emptied `vector_entries` and `vector_indices`. Two arms would
//! have been silently uncovered. The last case restores the corpus's BUILTIN
//! default so the phases after it see the state they were written against.
//!
//! Every counter is asserted non-zero SOMEWHERE in the sequence (a corpus where
//! every pass skipped would agree with a port that did nothing) — and that now
//! includes `mismatched.mountChunks`.
//!
//! **`mismatched.mountChunks` is LIVE (v4 `7bcd8515`, Bug 16).** The count reads
//! `doc_mount_points` from the MOUNT-INDEX partition where it actually lives, so
//! the corpus's non-conforming chunks on an ENABLED mount are counted — the
//! target-dim case reports `mountChunks == 1` (one nonconforming chunk; a second
//! is excluded as FAILED for that profile). The old dead-code tripwire is retired
//! into a plain non-zero shape guard, and the per-case struct comparison against
//! v4 does the rest.
//!
//! ## Minted values
//!
//! The render mints a UUID per NEW chunk and per enqueued job, and a minted chunk
//! id appears again inside its job's `payload`. So rows are not comparable by id:
//! both sides are re-keyed here by a natural key ((chatId, interchangeIndex) for
//! chunks, the payload for jobs, …), every minted chunk id is REWRITTEN to
//! `<chunk:{chatId}#{index}>` wherever it occurs (including inside payload JSON),
//! and a minted row's own id becomes `<minted>`. Rows whose ids ARE pinned by the
//! corpus keep them, so "the upsert reused the existing chunk row" stays a
//! visible, asserted fact rather than something normalization hides.
//!
//! Timestamps on minted rows are placeholdered — EXCEPT on `conversation_chunks`,
//! where both sides stamp the injected clock and so must agree exactly.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout), against /tmp COPIES
//! of the COMMITTED fixture — the oracle mutates the DBs it is pointed at, so
//! never point it at the committed files (P4.D32's sweep hazard):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   mkdir -p /tmp/qt-er-oracle/cases /tmp/qt-er-oracle/fixtures /tmp/qt-er-oracle/db
//!   cp $V5W/crates/quilltap-web/tests/fixtures/embedding-remainder-main.db  /tmp/qt-er-oracle/db/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/embedding-remainder-mount.db /tmp/qt-er-oracle/db/
//!   cp $V5W/harness/oracle/cases/embedding-remainder.test.ts /tmp/qt-er-oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/embedding-remainder.json  /tmp/qt-er-oracle/fixtures/
//!   cp -R $V5W/harness/oracle/fixtures/help-sync              /tmp/qt-er-oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   TZ=UTC \
//!   QT_FIXTURE_ER_MAIN=/tmp/qt-er-oracle/db/embedding-remainder-main.db \
//!   QT_FIXTURE_ER_MOUNT=/tmp/qt-er-oracle/db/embedding-remainder-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-embedding-remainder.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots /tmp/qt-er-oracle/cases -- embedding-remainder
//! Run:
//!   TZ=UTC QT_ORACLE_EMBEDDING_REMAINDER=/tmp/oracle-embedding-remainder.ndjson \
//!     cargo test -p quilltap-harness --test embedding_remainder_equivalence
//!
//! Rebuilding the COMMITTED fixture pair is a separate, DELIBERATE act — a
//! rebuild mints fresh UUIDs and invalidates every family reading these DBs,
//! so it re-runs every consumer. When (and only when) the fixture itself must
//! change: `build-embedding-remainder-fixture.ts` writes fresh DBs to the
//! paths in `QT_FIXTURE_OUT` / `QT_FIXTURE_MOUNT_OUT`; copy those over the
//! committed `embedding-remainder-{main,mount}.db` and regenerate this
//! family's oracle plus every sibling that reads them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::conversation_render_job::{
    handle_conversation_render, ConversationRenderPayload,
};
use quilltap_core::services::conversation_render_reconcile::{
    reconcile_conversation_rendering, ReconcileResult,
};
use quilltap_core::services::embedding_dimension_reconcile::{
    reconcile_embedding_dimensions, DimensionReconcileResult, MismatchedCounts, SkippedReason,
};
use quilltap_core::services::embedding_reindex_job::{
    handle_embedding_reindex_all, EmbeddingReindexAllPayload,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderCase {
    name: String,
    chat_id: String,
    full_reembed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReindexCase {
    name: String,
    profile_id: String,
    scope: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueMemoriesCase {
    name: String,
    chat_id: String,
    user_id: String,
}

/// P4.d27 — one boot dimension-reconcile pass plus its default-profile prelude
/// (harness scaffolding: one UPDATE, run identically on both sides).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DimensionReconcileCase {
    name: String,
    set_default_profile_id: Option<String>,
    clear_default: bool,
}

/// P4.d27 — one arm of v4 `7391404e`, named so a fixture edit that stops
/// exercising it fails BY NAME instead of quietly agreeing.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShapeGuard {
    entity_type: String,
    entity_id: String,
    why: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReindexShapeGuards {
    mismatched_profile_id: String,
    must_enqueue: Vec<ShapeGuard>,
    must_not_enqueue: Vec<ShapeGuard>,
    never_enqueued: Vec<ShapeGuard>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    render_now_iso: String,
    render_cases: Vec<RenderCase>,
    reindex_cases: Vec<ReindexCase>,
    reindex_shape_guards: ReindexShapeGuards,
    queue_memories_cases: Vec<QueueMemoriesCase>,
    dimension_reconcile_cases: Vec<DimensionReconcileCase>,
}

/// v4 `lib/api/responses.ts` kind→status (the routes-family mapping).
fn status_of(kind: quilltap_core::api::ErrorKind) -> u16 {
    use quilltap_core::api::ErrorKind;
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 423,
        // The store-unavailable refusal (P4.23) — v4's deliberate
        // contextful 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// A `Response` as the (status, body) pair the oracle's rows compare against.
fn status_body(r: &quilltap_core::api::Response) -> (u16, Value) {
    if let quilltap_core::api::Response::Error(e) = r {
        return (status_of(e.kind), serde_json::json!({ "error": e.message }));
    }
    let v = serde_json::to_value(r).unwrap();
    (200, v.get("data").cloned().unwrap_or(Value::Null))
}

/// (table, main?, natural sort columns, timestamps stay EXACT on minted rows).
const TABLES: &[(&str, bool, &[&str], bool)] = &[
    ("chats", true, &["id"], true),
    (
        "conversation_chunks",
        true,
        &["chatId", "interchangeIndex"],
        // Both sides stamp the injected clock here, so a minted chunk's
        // timestamps must agree exactly — normalizing them would hide it.
        true,
    ),
    (
        "background_jobs",
        true,
        &["type", "status", "priority", "payload"],
        false,
    ),
    (
        "embedding_status",
        true,
        &["entityType", "entityId", "profileId"],
        false,
    ),
    ("memories", true, &["characterId", "summary"], false),
    ("help_docs", true, &["path"], false),
    ("vector_entries", true, &["characterId", "id"], false),
    ("vector_indices", true, &["characterId"], false),
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/embedding-remainder.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// Every UUID-shaped string anywhere in the corpus — the set of ids BOTH sides
/// pinned, so anything else in a dump is minted.
fn pinned_ids(spec: &Value) -> BTreeSet<String> {
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::String(s) => {
                if s.len() == 36 && s.as_bytes()[14] == b'4' && s.matches('-').count() == 4 {
                    out.insert(s.clone());
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    walk(spec, &mut out);
    out
}

/// `interchangeIndex` renders as a JS number (`0`, not `0.0`); the dumps already
/// carry it that way, so plain `to_string` on the JSON value matches both sides.
fn cell(row: &Value, col: &str) -> String {
    match row.get(col) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// Label every minted id by its row's natural key, so it can be rewritten
/// wherever it appears — its own row AND inside a job payload's `entityId`.
///
/// Two tables mint ids in this family: `conversation_chunks` (the render's new
/// interchanges) and `help_docs` (the reindex's disk sync, which re-creates every
/// doc from the committed help tree). Both reappear in `background_jobs.payload`,
/// so blanking the row id alone would leave the payloads incomparable.
fn minted_labels(
    tables: &BTreeMap<String, Vec<Value>>,
    pinned: &BTreeSet<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for row in tables.get("conversation_chunks").into_iter().flatten() {
        let id = cell(row, "id");
        if pinned.contains(&id) {
            continue;
        }
        out.push((
            id,
            format!(
                "<chunk:{}#{}>",
                cell(row, "chatId"),
                cell(row, "interchangeIndex")
            ),
        ));
    }
    for row in tables.get("help_docs").into_iter().flatten() {
        let id = cell(row, "id");
        if pinned.contains(&id) {
            continue;
        }
        out.push((id, format!("<helpdoc:{}>", cell(row, "path"))));
    }
    out
}

/// Rewrite minted ids, placeholder minted rows' own ids (and, where the table
/// says so, their timestamps), then sort by the natural key.
fn normalize(tables: &mut BTreeMap<String, Vec<Value>>, pinned: &BTreeSet<String>) {
    let labels = minted_labels(tables, pinned);
    for (table, _, sort_cols, exact_ts) in TABLES {
        let Some(rows) = tables.get_mut(*table) else {
            continue;
        };
        for row in rows.iter_mut() {
            let minted = !pinned.contains(&cell(row, "id"));
            let obj = row.as_object_mut().expect("row object");
            let cols: Vec<String> = obj.keys().cloned().collect();
            for col in cols {
                // Rewrite minted chunk ids wherever they occur (job payloads
                // carry them as `entityId`).
                if let Some(Value::String(s)) = obj.get(&col) {
                    let mut s = s.clone();
                    for (id, label) in &labels {
                        if s.contains(id.as_str()) {
                            s = s.replace(id.as_str(), label);
                        }
                    }
                    obj.insert(col.clone(), Value::String(s));
                }
                if !minted {
                    continue;
                }
                if col == "id" {
                    // The chunk rewrite above already relabelled its own id.
                    let relabelled = obj
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s.starts_with("<chunk:") || s.starts_with("<helpdoc:"));
                    if !relabelled {
                        obj.insert(col.clone(), Value::String("<minted>".to_string()));
                    }
                } else if col.ends_with("At")
                    && !*exact_ts
                    && obj.get(&col).map(|v| !v.is_null()).unwrap_or(false)
                {
                    obj.insert(col.clone(), Value::String("<ts>".to_string()));
                }
            }
        }
        // The natural key first, then the whole normalized row as a
        // tie-breaker. The key alone is NOT total here: a seeded DEAD job and a
        // minted one cancelled by the reindex share type/status/priority/payload
        // exactly, and an arbitrary order between them made the diff report a
        // divergence that was only a permutation.
        rows.sort_by_key(|r| {
            let key = sort_cols
                .iter()
                .map(|c| cell(r, c))
                .collect::<Vec<_>>()
                .join(" ");
            (key, r.to_string())
        });
    }
}

#[tokio::test]
async fn embedding_remainder_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_EMBEDDING_REMAINDER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_EMBEDDING_REMAINDER to the oracle NDJSON (see header).");
            return;
        }
    };

    let spec_json: Value = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec json");
    let spec: Spec = serde_json::from_value(spec_json.clone()).expect("parse spec");
    let pinned = pinned_ids(&spec_json);

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let mut oracle_reconcile: Option<ReconcileResult> = None;
    let mut oracle_renders: Vec<(String, String, Option<String>)> = Vec::new();
    let mut oracle_reindexes: Vec<(String, String, Option<String>)> = Vec::new();
    let mut oracle_queue: Vec<(String, u16, Value)> = Vec::new();
    let mut oracle_dim: Vec<(String, DimensionReconcileResult)> = Vec::new();
    let mut want_tables: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut want_columns: BTreeMap<String, Value> = BTreeMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("reconcile") => {
                oracle_reconcile = Some(ReconcileResult {
                    incomplete_chats: v["incompleteChats"].as_u64().unwrap() as usize,
                    enqueued: v["enqueued"].as_u64().unwrap() as usize,
                    reused: v["reused"].as_u64().unwrap() as usize,
                    failed: v["failed"].as_u64().unwrap() as usize,
                    skipped_stale: v["skippedStale"].as_u64().unwrap() as usize,
                })
            }
            Some("render") => oracle_renders.push((
                v["name"].as_str().unwrap().to_string(),
                v["outcome"].as_str().unwrap().to_string(),
                v.get("error").and_then(Value::as_str).map(str::to_string),
            )),
            Some("reindex") => oracle_reindexes.push((
                v["name"].as_str().unwrap().to_string(),
                v["outcome"].as_str().unwrap().to_string(),
                v.get("error").and_then(Value::as_str).map(str::to_string),
            )),
            // P4.d27 — `{kind:'dimReconcile', name, ...result}`.
            Some("dimReconcile") => {
                let m = &v["mismatched"];
                oracle_dim.push((
                    v["name"].as_str().unwrap().to_string(),
                    DimensionReconcileResult {
                        target_dimensions: v["targetDimensions"].as_u64().map(|n| n as usize),
                        skipped_reason: v["skippedReason"].as_str().map(|s| match s {
                            "no-profile" => SkippedReason::NoProfile,
                            "builtin-profile" => SkippedReason::BuiltinProfile,
                            "no-fixed-dim" => SkippedReason::NoFixedDim,
                            "db-unavailable" => SkippedReason::DbUnavailable,
                            other => panic!("unknown skippedReason {other:?}"),
                        }),
                        vector_entries_deleted: v["vectorEntriesDeleted"].as_u64().unwrap()
                            as usize,
                        vector_index_meta_fixed: v["vectorIndexMetaFixed"].as_u64().unwrap()
                            as usize,
                        stale_chunk_embeddings_cleared: v["staleChunkEmbeddingsCleared"]
                            .as_u64()
                            .unwrap()
                            as usize,
                        mismatched: MismatchedCounts {
                            memories: m["memories"].as_u64().unwrap() as usize,
                            conversation_chunks: m["conversationChunks"].as_u64().unwrap() as usize,
                            help_docs: m["helpDocs"].as_u64().unwrap() as usize,
                            mount_chunks: m["mountChunks"].as_u64().unwrap() as usize,
                        },
                        reindex_enqueued: v["reindexEnqueued"].as_bool().unwrap(),
                    },
                ))
            }
            Some("queueMemories") => oracle_queue.push((
                v["name"].as_str().unwrap().to_string(),
                v["status"].as_u64().unwrap() as u16,
                v["body"].clone(),
            )),
            Some("table") => {
                let table = v["table"].as_str().unwrap().to_string();
                want_columns.insert(table.clone(), v["columns"].clone());
                want_tables.insert(table, v["rows"].as_array().unwrap().clone());
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Corpus-shape assertions (not hand counts): the oracle ran EXACTLY this
    // corpus's render cases in order, and dumped exactly the diffed table set.
    assert_eq!(
        oracle_renders
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        spec.render_cases
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        "oracle render-case set/order diverges from the corpus — regenerate"
    );
    assert_eq!(
        oracle_reindexes
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        spec.reindex_cases
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        "oracle reindex-case set/order diverges from the corpus — regenerate"
    );
    // Both throw arms must actually have thrown, or the corpus stopped covering
    // them and a port that never throws would agree.
    assert_eq!(
        oracle_reindexes
            .iter()
            .filter(|(_, o, _)| o == "error")
            .count(),
        2,
        "the corpus stopped exercising both reindex throw arms"
    );
    assert_eq!(
        oracle_queue
            .iter()
            .map(|(n, _, _)| n.as_str())
            .collect::<Vec<_>>(),
        spec.queue_memories_cases
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        "oracle queue-memories case set/order diverges from the corpus — regenerate"
    );
    // All three 400 arms must actually have fired (both empty-batch messages and
    // the no-cheap-LLM one), or a port that never rejects would agree.
    assert_eq!(
        oracle_queue.iter().filter(|(_, s, _)| *s == 400).count(),
        3,
        "the corpus stopped exercising all three queue-memories 400 arms"
    );
    // P4.d27 — the reindex arms, asserted against the ORACLE's own job dump
    // BEFORE the row diff. The diff proves v5 agrees with v4; these prove the
    // corpus still asks the questions the drift is about.
    {
        let g = &spec.reindex_shape_guards;
        let jobs = want_tables
            .get("background_jobs")
            .expect("oracle dumped no background_jobs");
        // (entityType, entityId, profileId) triples of every enqueued embed.
        let embeds: Vec<(String, String, String)> = jobs
            .iter()
            .filter(|r| cell(r, "type") == "EMBEDDING_GENERATE")
            .filter_map(|r| {
                let p: Value = serde_json::from_str(&cell(r, "payload")).ok()?;
                Some((
                    p.get("entityType")?.as_str()?.to_string(),
                    p.get("entityId")?.as_str()?.to_string(),
                    p.get("profileId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                ))
            })
            .collect();
        let in_mismatched_run = |guard: &ShapeGuard| {
            embeds.iter().any(|(t, i, p)| {
                t == &guard.entity_type && i == &guard.entity_id && p == &g.mismatched_profile_id
            })
        };
        for guard in &g.must_enqueue {
            assert!(
                in_mismatched_run(guard),
                "the corpus stopped exercising an arm — {} {} should be enqueued in mismatched-dim scope ({})",
                guard.entity_type, guard.entity_id, guard.why
            );
        }
        for guard in &g.must_not_enqueue {
            assert!(
                !in_mismatched_run(guard),
                "the corpus stopped exercising an arm — {} {} must NOT be enqueued in mismatched-dim scope ({})",
                guard.entity_type, guard.entity_id, guard.why
            );
        }
        for guard in &g.never_enqueued {
            assert!(
                !embeds
                    .iter()
                    .any(|(t, i, _)| t == &guard.entity_type && i == &guard.entity_id),
                "the corpus stopped exercising an arm — {} {} must never be enqueued, under any scope ({})",
                guard.entity_type, guard.entity_id, guard.why
            );
        }
    }

    assert_eq!(
        want_tables.keys().cloned().collect::<BTreeSet<_>>(),
        TABLES
            .iter()
            .map(|(t, ..)| t.to_string())
            .collect::<BTreeSet<_>>(),
        "oracle dumped a different table set"
    );

    // Fresh copies so the committed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-er-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-er-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(
        fixtures_dir().join("embedding-remainder-main.db"),
        &work_main,
    )
    .unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(
        fixtures_dir().join("embedding-remainder-mount.db"),
        &work_mount,
    )
    .unwrap_or_else(|e| panic!("copy mount fixture: {e}"));

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    // ── Phase 1: the startup reconcile, over the pristine fixture copy.
    let want_reconcile = oracle_reconcile.expect("oracle emitted no reconcile row — regenerate");
    // A reconcile that found nothing would agree with a port that did nothing,
    // so pin the corpus's own shape: BOTH arms fire, the dedupe reuses, and
    // (P4.D25) the staleness gate actually SKIPS cold-tiered chats.
    // `>= 5` is load-bearing for arm (C): the pre-bug-17 corpus produced
    // exactly 4 incomplete chats from arms (A)+(B), so a fixture edit that
    // stopped seeding the in-window over-budget chunk would fall back to 4 and
    // fail here rather than letting arm (C) go silently unexercised.
    assert!(
        want_reconcile.incomplete_chats >= 5
            && want_reconcile.enqueued >= 1
            && want_reconcile.reused >= 1
            && want_reconcile.skipped_stale >= 1,
        "the corpus stopped exercising the reconcile arms (incl. arm C's in-window chunk) + the dedupe + the stale skip: {want_reconcile:?}"
    );
    // The reconcile is a BOOT pass: synchronous, on the writer connection. Under
    // the async test runtime that needs a blocking thread (`write_blocking`
    // panics if called on a runtime thread).
    let got_reconcile = {
        let db = db.clone();
        let now_ms =
            quilltap_core::clock::iso_to_ms(&spec.render_now_iso).expect("parse renderNow");
        tokio::task::spawn_blocking(move || {
            db.write_blocking(move |ws| {
                Ok(reconcile_conversation_rendering(
                    ws.main().connection(),
                    now_ms,
                ))
            })
        })
        .await
        .expect("join reconcile")
        .expect("reconcile")
    };
    assert_eq!(got_reconcile, want_reconcile, "reconcile counters diverge");

    // ── Phase 2: the render handler, corpus order, state accumulating.
    let mut got_renders: Vec<(String, String, Option<String>)> = Vec::new();
    for rc in &spec.render_cases {
        let payload = ConversationRenderPayload {
            chat_id: rc.chat_id.clone(),
            full_reembed: rc.full_reembed,
        };
        let outcome =
            handle_conversation_render(&db, &spec.user_id, &payload, &spec.render_now_iso).await;
        got_renders.push(match outcome {
            Ok(()) => (rc.name.clone(), "ok".to_string(), None),
            Err(e) => (rc.name.clone(), "error".to_string(), Some(e)),
        });
    }
    assert_eq!(
        got_renders, oracle_renders,
        "render outcome sequence diverges"
    );

    // ── Phase 2b: the boot DIMENSION reconcile (P4.d27). Runs where boot runs it
    // relative to the render reconcile, and BEFORE the reindex phase — after it,
    // `vector_entries` and `vector_indices` are empty (full scope wipes both) and
    // the DELETE and meta-snap arms would both read zero.
    //
    // Like the render reconcile this is a BOOT pass on the writer connection, so
    // it needs a blocking thread; the mount-index connection is handed in from the
    // same writer set, exactly as `seed_built_ins` does. The per-case prelude is
    // harness scaffolding — one UPDATE of `isDefault`, identical on both sides.
    {
        let want_names: Vec<&str> = oracle_dim.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            want_names,
            spec.dimension_reconcile_cases
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            "oracle dimension-reconcile case set/order diverges from the corpus — regenerate"
        );
        // A corpus where every pass skipped, or found nothing, would agree with a
        // port that did nothing at all. Pin that each arm of the pass actually
        // fires somewhere in the sequence.
        let any = |f: fn(&DimensionReconcileResult) -> bool| oracle_dim.iter().any(|(_, r)| f(r));
        assert!(
            any(|r| r.vector_entries_deleted > 0)
                && any(|r| r.vector_index_meta_fixed > 0)
                && any(|r| r.stale_chunk_embeddings_cleared > 0)
                && any(|r| r.mismatched.memories > 0)
                && any(|r| r.mismatched.conversation_chunks > 0)
                && any(|r| r.mismatched.help_docs > 0)
                && any(|r| r.reindex_enqueued)
                && any(|r| r.skipped_reason == Some(SkippedReason::BuiltinProfile))
                && any(|r| r.skipped_reason == Some(SkippedReason::NoFixedDim))
                && any(|r| r.skipped_reason == Some(SkippedReason::NoProfile))
                && any(|r| r.target_dimensions.is_some() && !r.reindex_enqueued),
            "the corpus stopped exercising an arm of the dimension reconcile: {oracle_dim:?}"
        );
        // v4 `7bcd8515` (Bug 16) fixed the mount-chunk count to read the
        // mount-index partition, so it is now LIVE: the corpus holds a
        // non-conforming chunk on an ENABLED mount, so the target-dim case counts
        // it (`mountChunks > 0`). Pin that the corpus still exercises the arm —
        // the per-case `assert_eq!` below is the real comparison against v4.
        assert!(
            oracle_dim
                .iter()
                .any(|(_, r)| r.mismatched.mount_chunks > 0),
            "the corpus stopped exercising the mount-chunk count (Bug 16 fix): {oracle_dim:?}"
        );

        for (case, (want_name, want)) in
            spec.dimension_reconcile_cases.iter().zip(oracle_dim.iter())
        {
            let db2 = db.clone();
            let (clear, set) = (case.clear_default, case.set_default_profile_id.clone());
            let now_ms =
                quilltap_core::clock::iso_to_ms(&spec.render_now_iso).expect("parse renderNow");
            let got = tokio::task::spawn_blocking(move || {
                db2.write_blocking(move |ws| {
                    let main = ws.main().connection();
                    if clear {
                        main.execute("UPDATE embedding_profiles SET isDefault = 0", [])?;
                    } else if let Some(id) = set {
                        main.execute("UPDATE embedding_profiles SET isDefault = 0", [])?;
                        main.execute(
                            "UPDATE embedding_profiles SET isDefault = 1 WHERE id = ?1",
                            [&id],
                        )?;
                    }
                    Ok(reconcile_embedding_dimensions(
                        main,
                        ws.mount_index().map(|mi| mi.connection()),
                        now_ms,
                    ))
                })
            })
            .await
            .expect("join dimension reconcile")
            .expect("dimension reconcile");
            assert_eq!(&got, want, "dimension-reconcile case {want_name} diverges");
        }
    }

    // ── Phase 3: the reindex handler, corpus order. Both sides walk the SAME
    // committed help tree — the oracle by chdir (v4 captures
    // `HELP_DIR = join(process.cwd(), 'help')` at module load), this side
    // through the production host walker.
    let help_files = quilltap_host::files_store::load_help_source_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/help-sync"),
    );
    assert!(
        !help_files.is_empty(),
        "the committed help tree walked empty — both sides would sync nothing"
    );
    let mut got_reindexes: Vec<(String, String, Option<String>)> = Vec::new();
    for rc in &spec.reindex_cases {
        let payload = EmbeddingReindexAllPayload {
            profile_id: rc.profile_id.clone(),
            scope: rc.scope.clone(),
        };
        let outcome = handle_embedding_reindex_all(
            &db,
            &spec.user_id,
            &payload,
            &help_files,
            &spec.render_now_iso,
        )
        .await;
        got_reindexes.push(match outcome {
            Ok(()) => (rc.name.clone(), "ok".to_string(), None),
            Err(e) => (rc.name.clone(), "error".to_string(), Some(e)),
        });
    }
    assert_eq!(
        got_reindexes, oracle_reindexes,
        "reindex outcome sequence diverges"
    );

    // ── Phase 4: the queue-memories action, corpus order. The user id is the
    // CASE's, not the corpus default — one case names the second user, whose
    // missing chat-settings row is the resolver's `null` → 400 arm.
    for (case, (want_name, want_status, want_body)) in
        spec.queue_memories_cases.iter().zip(oracle_queue.iter())
    {
        let got =
            quilltap_core::api::memories::chat_queue_memories(&db, &case.user_id, &case.chat_id)
                .await;
        let (got_status, got_body) = status_body(&got);
        assert_eq!(
            (got_status, &got_body),
            (*want_status, want_body),
            "queue-memories case {want_name} diverges"
        );
    }

    // ── Dump + diff.
    let mut got_tables: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut got_columns: BTreeMap<String, Value> = BTreeMap::new();
    for (table, main, ..) in TABLES {
        let dump = if *main {
            db.read_main(|conn| dump_table_json_conn(conn, table, "id"))
        } else {
            db.read_mount_index(|conn| dump_table_json_conn(conn, table, "id"))
        }
        .unwrap_or_else(|e| panic!("dump {table}: {e:?}"));
        got_columns.insert(table.to_string(), dump["columns"].clone());
        got_tables.insert(
            table.to_string(),
            dump["rows"].as_array().cloned().unwrap_or_default(),
        );
    }
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    normalize(&mut got_tables, &pinned);
    normalize(&mut want_tables, &pinned);

    for (table, ..) in TABLES {
        assert_eq!(
            got_columns[*table], want_columns[*table],
            "{table} column set / order"
        );
        let g = &got_tables[*table];
        let w = &want_tables[*table];
        for (i, (gr, wr)) in g.iter().zip(w.iter()).enumerate() {
            if gr != wr {
                let gp = std::env::temp_dir().join(format!("qt-er-got-{table}.json"));
                let wp = std::env::temp_dir().join(format!("qt-er-want-{table}.json"));
                let _ = std::fs::write(&gp, serde_json::to_string_pretty(g).unwrap());
                let _ = std::fs::write(&wp, serde_json::to_string_pretty(w).unwrap());
                panic!(
                    "{table} rows diverge at index {i} (full dumps: {} / {})\n got: {gr:#}\nwant: {wr:#}",
                    gp.display(),
                    wp.display()
                );
            }
        }
        assert_eq!(
            g.len(),
            w.len(),
            "{table} row-count diverges: got {} vs want {}",
            g.len(),
            w.len()
        );
    }

    println!(
        "embedding_remainder: reconcile + {} render / {} dim-reconcile / {} reindex / {} queue-memories cases, {} tables OK",
        got_renders.len(),
        spec.dimension_reconcile_cases.len(),
        got_reindexes.len(),
        spec.queue_memories_cases.len(),
        TABLES.len()
    );
}
