//! P4.6s MEMORIES route-surface differential: `api::memories::*` vs v4's REAL
//! memories route handlers. Both sides read a FRESH copy of the committed
//! memories-{main,mount}.db fixture per case (baked ids identical → no remap).
//! Reads carry the full body; error arms are compared as HTTP status + message
//! (the kind→status recipe). Mutation / job-enqueue arms are added by the later
//! P4.6s commits.
//!
//! **Fixture vintage: v4 `b8449b3e`** (P4.52, 2026-08-20). The committed pair
//! was widened to v4's current schema by
//! `harness/oracle/fixtures/migrate-memories-fixture-columns.ts` — v4's own
//! migration ALTERs for the seven columns it predated (`characters.archivedAt`
//! / `archiveFileId` / `archivedAvatarFileId`,
//! `connection_profiles.multiCharacterPrefill`, `chat_settings.composerEmoji` /
//! `composerUnicode` / `smartTypographySettings`). That retired the
//! `housekeeping_config_set` RULED VINTAGE ROW: v4's
//! `handleWriteHousekeepingConfig` writes through `chatSettings.updateForUser`,
//! whose `$set: validated` names every `chat_settings` column with a Zod
//! default, so at the old vintage it died on `no such column: composerEmoji`
//! and answered a bare 500. It now answers 200 and the row is a plain
//! `check_body` like its siblings. Two `generateDDL` columns are DELIBERATELY
//! still absent (`characters.metadata`, `canChooseOutfit`): both are
//! MANAGED_FIELDS v4 strips from every DB write, no v4 migration adds them, and
//! a real migrated instance does not carry them either — see the migrate
//! script's header for the measured diff and the reasoning.
//!
//! TWO oracle NDJSONs feed this family (the route surface and the config
//! surface); the run stage needs BOTH env vars or the test skips silently.
//!
//! Generate (Node 24, from the v4 checkout; the cases are staged in a /tmp
//! mirror because v4's jest ignores `.claude/` paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-mem-routes-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/memories-routes.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/cases/memories-config.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/memories-web.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/apply-memory-tiers-migration.ts" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
//!   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memories-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- memories-routes
//!   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
//!   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-memories-config.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- memories-config
//! Run:
//!   QT_ORACLE_MEMORIES_ROUTES=/tmp/oracle-memories-routes.ndjson \
//!   QT_ORACLE_MEMORIES_CONFIG=/tmp/oracle-memories-config.ndjson \
//!     cargo test -p quilltap-harness --test memories_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::memories;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::ErasedEmbeddingProvider;
use quilltap_core::model::wire::CannedWireTransport;
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use serde::Deserialize;
use serde_json::{json, Value};

const MNEMO: &str = "b1000000-0000-4000-8000-000000000001";
const ORLA: &str = "b1000000-0000-4000-8000-000000000002";
const CHAT_SALON: &str = "c1000000-0000-4000-8000-000000000001";
const MSG_S1: &str = "d1000000-0000-4000-8000-000000000001";
const MSG_S2A: &str = "d1000000-0000-4000-8000-000000000002";
const MEM_TAGGED_1: &str = "b2000000-0000-4000-8000-0000000000a0";
const MEM_TAGGED_2: &str = "b2000000-0000-4000-8000-0000000000a1";
const MEM_REL_B: &str = "b2000000-0000-4000-8000-0000000000e1";
const MISSING: &str = "00000000-0000-4000-8000-0000000000ff";
/// The wall-clock the search recency ranking is pinned to (mirrors the oracle).
const FIXED_NOW_MS: f64 = 1_783_000_000_000.0;

// === P4.D269: list/search rows (append-only) ===
/// The plant every P4.D269 row runs on its copy — the twin of the oracle
/// case's `P4D269_PLANT`: two cold rows (one superseded by a hot row), a
/// CONSOLIDATED hot digest, a NULL-tier row.
const P4D269_PLANT: &[&str] = &[
    "UPDATE memories SET tier = 'cold', supersededById = 'b2000000-0000-4000-8000-0000000000e0' WHERE id = 'b2000000-0000-4000-8000-0000000000e1'",
    "UPDATE memories SET tier = 'cold' WHERE id = 'b2000000-0000-4000-8000-100000000016'",
    "UPDATE memories SET source = 'CONSOLIDATED', consolidatedFrom = '[\"b2000000-0000-4000-8000-0000000000f0\",\"b2000000-0000-4000-8000-0000000000f1\"]', consolidatedAt = '2026-03-02T00:00:00.000Z' WHERE id = 'b2000000-0000-4000-8000-0000000000a1'",
    "UPDATE memories SET tier = NULL WHERE id = 'b2000000-0000-4000-8000-0000000000b3'",
];
// === end ===

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/memories-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
/// A float-SENSITIVE comparand. [`norm`] runs `canon_numbers`, which collapses
/// `200.0` to `200` on BOTH sides — the exact difference the P4.56 float-literal
/// arms exist to measure, so those arms must not go through it. Keys are still
/// sorted (the families compare bags, not key order); numbers are rendered
/// verbatim by serde_json, so an integer-valued float prints `200.0`.
fn norm_float_exact(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// v4 `lib/api/responses.ts` kind→status.
fn status_of(kind: ErrorKind) -> u16 {
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

fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

/// The differential's embedding provider: the production `ApiEmbeddingProvider`
/// over the fixture db (a canned transport that the BUILTIN path never touches),
/// type-erased so the generic create/search take it by `&`.
fn provider(db: &Db) -> ErasedEmbeddingProvider {
    ErasedEmbeddingProvider::new(ApiEmbeddingProvider::new(
        db.clone(),
        CannedWireTransport::new(),
    ))
}

/// Blank the given keys (recursively) to a `<key>` sentinel — the minted /
/// re-generated fields on create/update (`id`, timestamps, `embedding`).
fn blank_keys(v: &mut Value, keys: &[&str]) {
    match v {
        Value::Object(o) => {
            for k in keys {
                if o.contains_key(*k) {
                    o.insert((*k).to_string(), Value::String(format!("<{k}>")));
                }
            }
            o.iter_mut().for_each(|(_, x)| blank_keys(x, keys));
        }
        Value::Array(a) => a.iter_mut().for_each(|x| blank_keys(x, keys)),
        _ => {}
    }
}
/// Round every float to 6 decimals (absorbs the f32-storage + ln-ULP seams on the
/// search scores + the re-read embedding vectors).
fn round_floats(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.fract() != 0.0 {
                    let r = (f * 1_000_000.0).round() / 1_000_000.0;
                    if let Some(nn) = serde_json::Number::from_f64(r) {
                        *v = Value::Number(nn);
                    }
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(round_floats),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| round_floats(x)),
        _ => {}
    }
}
fn norm_blanked(v: &Value, keys: &[&str]) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_keys(&mut v, keys);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn norm_rounded(v: &Value) -> String {
    let mut v = v.clone();
    round_floats(&mut v);
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}

/// A fresh Db over a private scratch dir; the dir is deleted when this drops
/// (the `Db` field drops first).
struct ScratchDb {
    db: Db,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for ScratchDb {
    type Target = Db;
    fn deref(&self) -> &Db {
        &self.db
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> ScratchDb {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-mem-routes-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("memories-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("memories-mount.db"), &mount).unwrap();
    // P4.88: the committed pair predates the two `78b381a96`-round columns, and
    // three of this family's arms (`count_by_chat`, `by_message_*`,
    // `delete_by_chat`) read a chat through `chats_read`, which always NAMES
    // them. Without this the family 500s with `no such column:
    // cycleOrderParticipantIds` on those arms — a red the workspace gate cannot
    // see, because the family SKIPs when its oracle vars are unset. The
    // repaired-at-boot idiom on the COPY, as `salon_fixture_p4d171_ensure`
    // documents for the salon pair.
    {
        let w = quilltap_core::db::Writer::open_writable(&main, &spec.test_pepper_base64)
            .expect("open the fixture copy writable to heal it");
        quilltap_core::test_support::ensure_p4d171_columns(w.connection());
        // P4.D265 (C1 §13): the memory-tiers columns — the twin of the oracle
        // case's `applyMemoryTiersMigration` on ITS per-case copy. Idempotent on
        // the widened committed pair (§R.3). P4.D266 / P4.D269 inherit this
        // hunk and never edit it.
        quilltap_core::test_support::ensure_memory_tiers_columns(w.connection());
        // The `83334b9a5` unification: the pair predates `embedding_status`, which
        // v4's repository creates lazily on the oracle's per-case copy and the
        // route's `handleEntityDeletion` now touches on both sides.
        quilltap_core::test_support::ensure_embedding_status_table(w.connection());
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    ScratchDb { db, _dir: scratch }
}

/// P4.D95: seed `instance_settings['memoryRecall']` through the REAL writer —
/// the Rust twin of the oracle case's `setMemoryRecallSettings` call.
async fn seed_recall(db: &Db, per_turn: bool) {
    use quilltap_core::db::instance_settings::{set_memory_recall_settings, MemoryRecallSettings};
    let settings = MemoryRecallSettings {
        scope_policy: "exclude".to_string(),
        expand_related: true,
        per_turn_conversation_summaries: per_turn,
    };
    db.write(move |w| set_memory_recall_settings(w.main().connection(), &settings))
        .await
        .expect("seed recall settings");
}

/// P4.55: seed `instance_settings['memoryExtractionLimits']` through the REAL
/// writer — the twin of the oracle's `setMemoryExtractionLimits`.
async fn seed_extraction_limits(db: &Db) {
    let bag = json!({
        "enabled": true,
        "maxPerHour": 77,
        "softStartFraction": 0.25,
        "softFloor": 0.75,
    });
    db.write(move |w| {
        quilltap_core::db::instance_settings::set_memory_extraction_limits(
            w.main().connection(),
            &bag,
        )
    })
    .await
    .expect("seed extraction limits");
}

// === P4.D298: the log + plant helpers (append-only) ===
/// v4 `{level, message, fields}` → v5's capture rendering (`<LEVEL> <target>
/// <message> k=v …`); an array / object field renders through the `…Json`
/// file-layer convention (§R.5).
fn render_v4_line(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let message = line["message"].as_str().unwrap();
    let target = if message.starts_with("[Memories API") {
        "quilltap_core::api::memories"
    } else if message.starts_with("[Messages API v1]") {
        "quilltap_core::api::salon"
    } else {
        "quilltap_core::services::chat_admin"
    };
    let mut out = format!("{level} {target} {message}");
    if let Some(fields) = line["fields"].as_object() {
        for (k, v) in fields {
            match v {
                Value::String(s) => out.push_str(&format!(" {k}={s}")),
                Value::Object(_) | Value::Array(_) => {
                    out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
                }
                other => out.push_str(&format!(" {k}={other}")),
            }
        }
    }
    out
}

/// v5's captured `[Memories API…]` lines — the route's own (siblings' lines
/// are compared by presence, on v4's side, where they are visible).
fn own_v5_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| {
            let rest = l.splitn(3, ' ').nth(2).unwrap_or("");
            rest.starts_with("[Memories API")
        })
        .cloned()
        .collect()
}

/// The twin of the oracle's `plantSql` (a plant on the case's own copy).
fn plant(rt: &tokio::runtime::Runtime, db: &Db, statements: &'static [&'static str]) {
    rt.block_on(db.write(move |w| {
        for sql in statements {
            w.main()
                .connection()
                .execute_batch(sql)
                .map_err(quilltap_core::db::DbError::from)?;
        }
        Ok(())
    }))
    .expect("plant");
}

/// The twin of the oracle's `dumpAllMemories` (ordered by the BAKED id).
fn dump_all_memories(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, characterId, chatId, source, tier, relatedMemoryIds, consolidatedFrom, lastAccessedAt FROM memories ORDER BY id",
        )?;
        let cols = [
            "id",
            "characterId",
            "chatId",
            "source",
            "tier",
            "relatedMemoryIds",
            "consolidatedFrom",
            "lastAccessedAt",
        ];
        let rows = stmt
            .query_map([], |r| {
                let mut m = serde_json::Map::new();
                for (i, c) in cols.iter().enumerate() {
                    let v: Option<String> = r.get(i)?;
                    m.insert((*c).to_string(), v.map(Value::String).unwrap_or(Value::Null));
                }
                Ok(Value::Object(m))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Array(rows))
    })
    .unwrap()
}

/// The twins of the oracle's `P4D298_*` plants.
const P4D298_SPARE_PLANT: &[&str] = &[
    "UPDATE memories SET chatId = 'c1000000-0000-4000-8000-000000000001' WHERE id = 'b2000000-0000-4000-8000-100000000011'",
    "UPDATE memories SET tier = 'cold' WHERE id = 'b2000000-0000-4000-8000-0000000000f1'",
    "UPDATE memories SET source = 'CONSOLIDATED', chatId = 'c1000000-0000-4000-8000-000000000001', consolidatedFrom = '[\"b2000000-0000-4000-8000-0000000000b1\"]' WHERE id = 'b2000000-0000-4000-8000-0000000000b0'",
    "UPDATE memories SET tier = 'cold', supersededById = 'b2000000-0000-4000-8000-0000000000b0' WHERE id = 'b2000000-0000-4000-8000-0000000000b1'",
];
const P4D298_COUNTS_PLANT: &[&str] = &[
    "UPDATE memories SET source = 'MANUAL' WHERE characterId = 'b1000000-0000-4000-8000-000000000002'",
    "UPDATE memories SET characterId = 'b1000000-0000-4000-8000-000000000003' WHERE id IN ('b2000000-0000-4000-8000-100000000011','b2000000-0000-4000-8000-100000000013','b2000000-0000-4000-8000-100000000015','b2000000-0000-4000-8000-100000000017','b2000000-0000-4000-8000-100000000019','b2000000-0000-4000-8000-100000000021')",
    "UPDATE memories SET tier = NULL WHERE id = 'b2000000-0000-4000-8000-100000000011'",
    "UPDATE memories SET tier = 'cold' WHERE id = 'b2000000-0000-4000-8000-100000000013'",
    "UPDATE memories SET tier = 'cold' WHERE id = 'b2000000-0000-4000-8000-0000000000f1'",
    "UPDATE memories SET tier = NULL WHERE id = 'b2000000-0000-4000-8000-0000000000b3'",
];
const P4D298_RETIRE_FAILS_PLANT: &[&str] = &[
    "CREATE TRIGGER qt_p4d298_retire_fails BEFORE DELETE ON memories BEGIN SELECT RAISE(ABORT, 'planted retire failure'); END",
];

/// The three re-attribution / cascade lines this lane owns (`af48f6cc2`) —
/// the routes around them log siblings' lines this family does not compare.
const REATTRIBUTION_LINES: &[&str] = &[
    "[Messages API v1] Cascade deleted memories with message",
    "[Messages API v1] Failed to delete memories during re-attribution",
    "[Chats v1] Failed to delete memories during bulk re-attribution",
];

/// §R.7 `RETIRED_MEMORY_VERB_UNKNOWN_VARIANT` (R-G, L-4) — the seven retired
/// memories verbs, both ways. v4's half: each oracle row answers 400
/// `{"error":"Unknown action: <verb>","availableActions":[…]}` with the verb
/// absent from the list (v4 `104e986c7` / `af48f6cc2`). v5's half: each retired
/// wire type fails `serde_json::from_value::<Request>` with serde's `unknown
/// variant` — the string `dispatch.rs` wraps as `400 Invalid request: …`.
/// (`row`, v4 verb, v5 wire type). Fifteen rows: the order's twelve plus the
/// three `housekeep_invalid_*` Zod rows, which now reach the same envelope.
const RETIRED_MEMORY_VERB_UNKNOWN_VARIANT: &[(&str, &str, &str)] = &[
    ("housekeep_preview", "housekeep", "memoryHousekeepPreview"),
    ("housekeep_dryrun", "housekeep", "memoryHousekeep"),
    ("housekeep_run", "housekeep", "memoryHousekeep"),
    (
        "housekeep_invalid_max_memories",
        "housekeep",
        "memoryHousekeep",
    ),
    (
        "housekeep_invalid_merge_threshold",
        "housekeep",
        "memoryHousekeep",
    ),
    (
        "housekeep_invalid_character_id",
        "housekeep",
        "memoryHousekeep",
    ),
    ("housekeep_sweep", "housekeep-sweep", "memoryHousekeepSweep"),
    (
        "housekeeping_config_get",
        "housekeeping-config",
        "memoryHousekeepingConfigGet",
    ),
    (
        "housekeeping_config_set",
        "housekeeping-config",
        "memoryHousekeepingConfigSet",
    ),
    (
        "housekeeping_config_set_invalid_bool",
        "housekeeping-config",
        "memoryHousekeepingConfigSet",
    ),
    (
        "housekeeping_config_set_out_of_range",
        "housekeeping-config",
        "memoryHousekeepingConfigSet",
    ),
    (
        "housekeeping_config_invalid_writes_nothing",
        "housekeeping-config",
        "memoryHousekeepingConfigSet",
    ),
    (
        "housekeeping_config_set_int_float_literal",
        "housekeeping-config",
        "memoryHousekeepingConfigSet",
    ),
    (
        "extraction_concurrency_get",
        "extraction-concurrency",
        "memoryExtractionConcurrencyGet",
    ),
    (
        "extraction_concurrency_set",
        "extraction-concurrency",
        "memoryExtractionConcurrencySet",
    ),
];

/// §R.7 `MEMORY_BAD_GATEWAY_AS_INTERNAL` (R-H, L-5): v4's two 502s — the
/// manual create's `embedding-failed` and the edit whose new text cannot be
/// embedded — answer `ErrorKind::Internal` with v4's message and `details:
/// {"status": 502}` (no new `ErrorKind`; a dispatch / Tauri caller sees 500).
/// Pinned both ways: v4 must answer 502 with the message, v5 `Internal` + the
/// carrier with the same message.
const MEMORY_BAD_GATEWAY_AS_INTERNAL: &[&str] =
    &["create_embedding_failed_502", "update_text_no_profile_502"];

// The create rows' key ORDER was a §S.5 residue at the lane (P4.D290's
// `without_embedding` dropped the vector with serde_json's `Map::remove` — a
// `swap_remove` under `preserve_order`, so the row's LAST key moved into
// `embedding`'s slot where v4's `withoutEmbedding` keeps `MemorySchema` order).
// `shift_remove` landed at the `83334b9a5` unification; every 200 create row
// now compares by BYTES (values AND order) below.

/// §R.7-style named table `OFFSET_OCCURRED_AT_ROW_SCHEMA` (MEASURED; a
/// finding, not this lane's code): v4's route schema admits an offset
/// `occurredAt` (`z.iso.datetime({ offset: true })`), but its memories
/// repository `create` validates `MemorySchema`, whose `TimestampSchema` is
/// `Z`-only — the ZodError surfaces as the middleware's 400 `Validation
/// error` and no row is written. v5's insert path (P4.D290's `insert_row` over
/// P4.D288's `MemCreate`) validates no row schema and inserts it (201). Pinned
/// both ways: `(row, v4 status, v5 action)`.
const OFFSET_OCCURRED_AT_ROW_SCHEMA: &[(&str, u64, &str)] = &[(
    "create_offset_occurred_at_row_refusal",
    400,
    "insert-linked",
)];

/// The keys a create / edit mints (masked to `<minted>` on both sides).
const MINTED_MEMORY_KEYS: &[&str] = &["id", "createdAt", "updatedAt", "lastReinforcedAt"];

fn mask_minted_memory(body: &mut Value) {
    if let Some(m) = body.get_mut("memory").and_then(Value::as_object_mut) {
        for k in MINTED_MEMORY_KEYS {
            if m.get(*k).is_some_and(|v| !v.is_null()) {
                m.insert((*k).to_string(), json!("<minted>"));
            }
        }
        // An insert-linked row's links name the NEW row's minted id on the
        // neighbour — and the new row's own list names baked ids only.
    }
}

/// `<LEVEL> <target> <rest>` → `<LEVEL> <rest>` (the three lines live in three
/// modules; v4 has none).
fn without_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or("");
    let _target = parts.next();
    format!("{level} {}", parts.next().unwrap_or(""))
}

fn is_reattribution_line(rest_after_level: &str) -> bool {
    REATTRIBUTION_LINES
        .iter()
        .any(|m| rest_after_level.starts_with(m))
}

const P4D298_DELETE_RACE_PLANT: &[&str] = &[
    "CREATE TRIGGER qt_p4d298_keep_row BEFORE DELETE ON memories WHEN old.id = 'b2000000-0000-4000-8000-0000000000e1' BEGIN SELECT RAISE(IGNORE); END",
];
const P4D298_REINFORCED_PLANT: &[&str] = &[
    "UPDATE memories SET reinforcedImportance = 0.9 WHERE id = 'b2000000-0000-4000-8000-100000000019'",
    "UPDATE memories SET reinforcedImportance = 0.1 WHERE id = 'b2000000-0000-4000-8000-100000000027'",
];
const P4D298_NULL_REINFORCED_PLANT: &[&str] = &[
    "UPDATE memories SET reinforcedImportance = NULL WHERE id = 'b2000000-0000-4000-8000-0000000000e0'",
];
const P4D298_STALE_JOBS_PLANT: &[&str] = &[
    "INSERT INTO background_jobs (id, userId, type, status, payload, scheduledAt, createdAt, updatedAt) VALUES ('b9000000-0000-4000-8000-000000000001', 'e18e05bc-63e8-4539-8a85-719b7a508850', 'MEMORY_EXTRACTION', 'PENDING', '{}', '2026-03-01T00:00:00.000Z', '2026-03-01T00:00:00.000Z', '2026-03-01T00:00:00.000Z')",
    "INSERT INTO background_jobs (id, userId, type, status, payload, scheduledAt, createdAt, updatedAt) VALUES ('b9000000-0000-4000-8000-000000000002', 'e18e05bc-63e8-4539-8a85-719b7a508850', 'INTER_CHARACTER_MEMORY', 'PENDING', '{}', '2026-03-01T00:00:00.000Z', '2026-03-01T00:00:00.000Z', '2026-03-01T00:00:00.000Z')",
];

/// The rows whose v4 half lands with a §S.5 handoff, not on this branch —
/// RESIDUE both ways: a mismatch is reported and tolerated; a MATCH fails
/// loudly ("the residue closed — move the row to the strict set"). (3) the
/// embeddings pair's re-point onto P4.D292's `memory_index_backfill`; (4) the
/// search route's re-point onto P4.D291's recall search.
/// EMPTY at the `83334b9a5` unification: §S.5 (4) landed (the search route on
/// P4.D291's `search_memories`) and the paginated filter now compares
/// `reinforcedImportance` as v4's does — both rows compare strictly. The table
/// stays so a future residue has a home (and the judge below keeps the
/// "residue closed" tripwire for it).
const S5_RESIDUE: &[(&str, &str)] = &[];

/// A minted value inside a captured line (`jobId=<uuid>`) → `<minted>`.
fn mask_field(line: &str, key: &str) -> String {
    let pat = format!(" {key}=");
    let Some(at) = line.find(&pat) else {
        return line.to_string();
    };
    let start = at + pat.len();
    let end = line[start..]
        .find(' ')
        .map(|i| start + i)
        .unwrap_or(line.len());
    format!("{}<minted>{}", &line[..start], &line[end..])
}
// === end ===

#[test]
fn memories_routes_match_oracle() {
    // P4.D298: one global capturing subscriber for the whole binary, armed
    // before any callsite is reached (memory note `capture-rig-writer-thread`).
    quilltap_core::test_support::global_capture::install();
    let Some(oracle_path) = env_or_skip("QT_ORACLE_MEMORIES_ROUTES") else {
        return;
    };
    let Some(config_path) = env_or_skip("QT_ORACLE_MEMORIES_CONFIG") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for path in [&oracle_path, &config_path] {
        for line in std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
        {
            let v: Value = serde_json::from_str(line).unwrap();
            oracle.insert(v["name"].as_str().unwrap().to_string(), v);
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let uid = spec.user_id.clone();

    // A 200-body check (baked ids identical → no blanking).
    let check_body = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_u64().unwrap_or(0);
        if want_status != 200 {
            eprintln!("[{name}] expected an error arm; use check_error");
            failed.push(name.to_string());
            return;
        }
        let got = response_data(got);
        let want = &rec["body"];
        if norm(&got) != norm(want) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm(&got), &norm(want))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };
    // An error-arm check: HTTP status + message (v4 `{error}` body).
    let check_error = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_u64().unwrap_or(0) as u16;
        let want_msg = rec["body"]["error"].as_str().unwrap_or("");
        match got {
            Response::Error(e) => {
                let got_status = status_of(e.kind);
                if got_status != want_status || e.message != want_msg {
                    eprintln!(
                        "[{name}] MISMATCH: got {got_status} {:?} / want {want_status} {want_msg:?}",
                        e.message
                    );
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK ({got_status}).");
                }
            }
            other => {
                eprintln!("[{name}] expected Error, got {:?}", response_data(other));
                failed.push(name.to_string());
            }
        }
    };

    // --- List (paginated) ---
    {
        let db = fresh_db(&spec, "lp");
        check_body(
            "list_paginated",
            &memories::memory_list(
                &db,
                MNEMO,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(20),
                Some(0),
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lp2");
        check_body(
            "list_paginated_p2",
            &memories::memory_list(
                &db,
                MNEMO,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(20),
                Some(20),
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lpi");
        check_body(
            "list_paginated_importance_asc",
            &memories::memory_list(
                &db,
                MNEMO,
                None,
                None,
                None,
                None,
                Some("importance"),
                Some("asc"),
                Some(15),
                None,
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lps");
        check_body(
            "list_paginated_search",
            &memories::memory_list(
                &db,
                MNEMO,
                Some("smuggler"),
                None,
                None,
                None,
                None,
                None,
                Some(50),
                None,
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lpm");
        check_body(
            "list_paginated_minImportance",
            &memories::memory_list(
                &db,
                MNEMO,
                None,
                Some(0.7),
                None,
                None,
                None,
                None,
                Some(50),
                None,
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lpsrc");
        check_body(
            "list_paginated_source_manual",
            &memories::memory_list(
                &db,
                MNEMO,
                None,
                None,
                Some("MANUAL"),
                None,
                None,
                None,
                Some(50),
                None,
            ),
            &mut failed,
        );
    }
    // --- List (legacy) ---
    {
        let db = fresh_db(&spec, "ll");
        check_body(
            "list_legacy",
            &memories::memory_list(&db, ORLA, None, None, None, None, None, None, None, None),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lls");
        check_body(
            "list_legacy_search",
            &memories::memory_list(
                &db,
                MNEMO,
                Some("barometer"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lmc");
        check_error(
            "list_missing_character",
            &memories::memory_list(&db, MISSING, None, None, None, None, None, None, None, None),
            &mut failed,
        );
    }
    // --- Item GET ---
    {
        let db = fresh_db(&spec, "get");
        check_body(
            "get",
            &rt.block_on(memories::memory_get(&db, MEM_TAGGED_2)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "getm");
        check_error(
            "get_missing",
            &rt.block_on(memories::memory_get(&db, MISSING)),
            &mut failed,
        );
    }
    // --- Count by chat ---
    {
        let db = fresh_db(&spec, "cbc");
        check_body(
            "count_by_chat",
            &memories::memory_count_by_chat(&db, CHAT_SALON),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "cbcm");
        check_error(
            "count_by_chat_missing",
            &memories::memory_count_by_chat(&db, MISSING),
            &mut failed,
        );
    }
    // --- By message ---
    {
        let db = fresh_db(&spec, "bms");
        check_body(
            "by_message_swipe",
            &memories::memory_by_message(&db, &uid, MSG_S2A),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "bm1");
        check_body(
            "by_message_single",
            &memories::memory_by_message(&db, &uid, MSG_S1),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "bmm");
        check_error(
            "by_message_missing",
            &memories::memory_by_message(&db, &uid, MISSING),
            &mut failed,
        );
    }
    // --- Character memory counts ---
    {
        let db = fresh_db(&spec, "cc");
        check_body(
            "character_counts",
            &memories::memory_character_counts(&db, &uid),
            &mut failed,
        );
    }

    // Dump the reinforcement/link state of specific memory rows (the create /
    // delete structural checks) — mirrors the oracle's `dumpMemoryRows`.
    let dump_rows = |db: &Db, ids: &[&str]| -> Value {
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        db.read_main(move |conn| {
            let rows: Vec<Value> = ids
                .iter()
                .map(
                    |id| match quilltap_core::db::memories_read::find_by_id(conn, id).unwrap() {
                        None => json!({ "id": id, "present": false }),
                        Some(m) => json!({
                            "id": id,
                            "present": true,
                            "reinforcementCount": m.get("reinforcementCount"),
                            "relatedMemoryIds": m.get("relatedMemoryIds"),
                        }),
                    },
                )
                .collect();
            Ok(json!({ "rows": rows }))
        })
        .unwrap()
    };

    let check_blanked = |name: &str, got: &Response, keys: &[&str], failed: &mut Vec<String>| {
        let want = &oracle[name]["body"];
        let got = response_data(got);
        if norm_blanked(&got, keys) != norm_blanked(want, keys) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm_blanked(&got, keys), &norm_blanked(want, keys))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };
    let check_tables = |name: &str, got: &Value, failed: &mut Vec<String>| {
        let want = &oracle[name]["tables"];
        if norm(got) != norm(want) {
            eprintln!(
                "[{name} tables] MISMATCH:\n{}",
                first_diff(&norm(got), &norm(want))
            );
            failed.push(format!("{name}_tables"));
        } else {
            eprintln!("[{name} tables] OK.");
        }
    };
    let check_rounded = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let want = &oracle[name]["body"];
        let got = response_data(got);
        if norm_rounded(&got) != norm_rounded(want) {
            eprintln!(
                "[{name}] MISMATCH:\n{}",
                first_diff(&norm_rounded(&got), &norm_rounded(want))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    };

    // --- Create — P4.D298 re-pinned these rows below (`{memory, action}`,
    // admission, the route's lines); the gate-era checks left with the gate.
    // --- Update — P4.D298 re-pinned this row below (`update_memory`).
    // --- Delete (+ unlink scrub) ---
    {
        let db = fresh_db(&spec, "del");
        let resp = rt.block_on(memories::memory_delete(&db, &uid, MEM_REL_B, FIXED_NOW_MS));
        check_body("delete", &resp, &mut failed);
        check_tables(
            "delete",
            &dump_rows(&db, &["b2000000-0000-4000-8000-0000000000e0", MEM_REL_B]),
            &mut failed,
        );
    }
    // --- Delete by chat ---
    {
        let db = fresh_db(&spec, "dbc");
        let resp = rt.block_on(memories::memory_delete_by_chat(&db, CHAT_SALON));
        check_body("delete_by_chat", &resp, &mut failed);
        let remaining = db
            .read_main(|conn| quilltap_core::db::memories_read::count_by_chat_id(conn, CHAT_SALON))
            .unwrap();
        check_tables(
            "delete_by_chat",
            &json!({ "remaining": remaining }),
            &mut failed,
        );
    }
    // --- Search (builtin TF-IDF; pinned clock) ---
    {
        let db = fresh_db(&spec, "srch");
        let p = provider(&db);
        let resp = rt.block_on(memories::memory_search(
            &db,
            &p,
            &uid,
            FIXED_NOW_MS,
            json!({ "characterId": MNEMO, "query": "smuggler cove hidden by the reef", "limit": 5 }),
        ));
        check_rounded("search", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "srchi");
        let p = provider(&db);
        let resp = rt.block_on(memories::memory_search(
            &db,
            &p,
            &uid,
            FIXED_NOW_MS,
            json!({
                "characterId": MNEMO,
                "query": "lighthouse keeper storm beacon",
                "limit": 10,
                "minImportance": 0.7,
            }),
        ));
        check_rounded("search_min_importance", &resp, &mut failed);
    }

    // P4.D288 (compile-forced, R-G): the housekeep / housekeep-sweep /
    // housekeeping-config verbs and their handlers are DELETED with v4's
    // (`104e986c7` — v4 answers its `Unknown action` envelope at the pin);
    // their rows left with them. P4.D298 re-pins this family at the pin.
    {
        let db = fresh_db(&spec, "rcg");
        check_body(
            "recall_config_get",
            &memories::memory_recall_config_get(&db),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "rcs");
        let resp = rt.block_on(memories::memory_recall_config_set(
            &db,
            json!({ "scopePolicy": "exclude" }),
        ));
        check_body("recall_config_set", &resp, &mut failed);
    }
    // P4.D95 (v4 `870a57fa`): the third recall field — a lone patch leaves the
    // other two at their stored values.
    {
        let db = fresh_db(&spec, "rcp");
        let resp = rt.block_on(memories::memory_recall_config_set(
            &db,
            json!({ "perTurnConversationSummaries": true }),
        ));
        check_body("recall_config_set_per_turn", &resp, &mut failed);
    }
    // P4.D95: the READ path over a NON-DEFAULT stored bag (a row left at the
    // default measures nothing about the reader).
    {
        let db = fresh_db(&spec, "rcgs");
        rt.block_on(seed_recall(&db, true));
        check_body(
            "recall_config_get_seeded",
            &memories::memory_recall_config_get(&db),
            &mut failed,
        );
    }
    // P4.D95: the merge OVER a stored non-default bag — clearing the new field
    // must not disturb the other two (the true → false direction).
    {
        let db = fresh_db(&spec, "rcms");
        rt.block_on(seed_recall(&db, true));
        let resp = rt.block_on(memories::memory_recall_config_set(
            &db,
            json!({ "perTurnConversationSummaries": false }),
        ));
        check_body("recall_config_merge_over_stored", &resp, &mut failed);
    }
    // §3 of the c8a3cf77 unification: a present-but-invalid value 400s (v4
    // `recallConfigSchema.safeParse` → `validationError`) and stores NOTHING —
    // never silently dropped in favour of the stored value. Status + `error`
    // message compared; the `details` issues array is the recorded no-details
    // divergence class.
    {
        let db = fresh_db(&spec, "rcie");
        let resp = rt.block_on(memories::memory_recall_config_set(
            &db,
            json!({ "scopePolicy": "both" }),
        ));
        check_error("recall_config_set_invalid_enum", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "rcib");
        let resp = rt.block_on(memories::memory_recall_config_set(
            &db,
            json!({ "perTurnConversationSummaries": 1 }),
        ));
        check_error("recall_config_set_invalid_bool", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "rciw");
        rt.block_on(seed_recall(&db, true));
        let resp = rt.block_on(memories::memory_recall_config_set(
            &db,
            json!({ "expandRelated": "yes" }),
        ));
        let name = "recall_config_invalid_writes_nothing";
        check_error(name, &resp, &mut failed);
        // The refusal wrote nothing: the stored bag survives, compared against
        // v4's real getter read after ITS refusal (`storedAfter`).
        let stored = db
            .read_main(quilltap_core::db::instance_settings::get_memory_recall_settings)
            .expect("read recall settings after refused set");
        let want = &oracle[name]["storedAfter"];
        if norm(&stored.to_json()) != norm(want) {
            eprintln!(
                "[{name}] storedAfter MISMATCH:\n{}",
                first_diff(&norm(&stored.to_json()), &norm(want))
            );
            failed.push(format!("{name}:storedAfter"));
        } else {
            eprintln!("[{name}] storedAfter OK.");
        }
    }
    {
        let db = fresh_db(&spec, "elg");
        check_body(
            "extraction_limits_get",
            &memories::memory_extraction_limits_get(&db),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "els");
        let resp = rt.block_on(memories::memory_extraction_limits_set(
            &db,
            json!({ "maxPerHour": 50 }),
        ));
        check_body("extraction_limits_set", &resp, &mut failed);
    }
    // P4.55: the `extractionLimitsConfigSchema` sibling of the housekeeping arms.
    {
        let db = fresh_db(&spec, "elit");
        let resp = rt.block_on(memories::memory_extraction_limits_set(
            &db,
            json!({ "maxPerHour": "many" }),
        ));
        check_error("extraction_limits_set_invalid_type", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "elir");
        let resp = rt.block_on(memories::memory_extraction_limits_set(
            &db,
            json!({ "softFloor": 1.5 }),
        ));
        check_error("extraction_limits_set_out_of_range", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "eliw");
        rt.block_on(seed_extraction_limits(&db));
        let resp = rt.block_on(memories::memory_extraction_limits_set(
            &db,
            json!({ "enabled": 1 }),
        ));
        let name = "extraction_limits_invalid_writes_nothing";
        check_error(name, &resp, &mut failed);
        let stored = db
            .read_main(quilltap_core::db::instance_settings::get_memory_extraction_limits)
            .expect("read extraction limits after refused set");
        let want = &oracle[name]["storedAfter"];
        if norm(&stored) != norm(want) {
            eprintln!(
                "[{name}] storedAfter MISMATCH:\n{}",
                first_diff(&norm(&stored), &norm(want))
            );
            failed.push(format!("{name}:storedAfter"));
        } else {
            eprintln!("[{name}] storedAfter OK.");
        }
    }
    {
        // P4.56 — the extraction-limits sibling of the float-literal arm.
        let db = fresh_db(&spec, "elfl");
        let name = "extraction_limits_set_int_float_literal";
        let resp = rt.block_on(memories::memory_extraction_limits_set(
            &db,
            json!({ "maxPerHour": 200.0, "softFloor": 1.0 }),
        ));
        let got = response_data(&resp);
        let want = &oracle[name]["body"];
        if norm_float_exact(&got) != norm_float_exact(want) {
            eprintln!(
                "[{name}] MISMATCH (float-exact):\n{}",
                first_diff(&norm_float_exact(&got), &norm_float_exact(want))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK (float-exact).");
        }
        let stored = db
            .read_main(quilltap_core::db::instance_settings::get_memory_extraction_limits)
            .expect("read extraction limits after the float-literal set");
        let want_stored = &oracle[name]["storedAfter"];
        if norm_float_exact(&stored) != norm_float_exact(want_stored) {
            eprintln!(
                "[{name}] storedAfter MISMATCH (float-exact):\n{}",
                first_diff(&norm_float_exact(&stored), &norm_float_exact(want_stored))
            );
            failed.push(format!("{name}:storedAfter"));
        } else {
            eprintln!("[{name}] storedAfter OK (float-exact).");
        }
    }
    // P4.D288 (compile-forced, R-G): the extraction-concurrency pair is
    // DELETED with v4's (`104e986c7`).

    // --- Regenerate + backfill status ---
    {
        let db = fresh_db(&spec, "bfp");
        check_body(
            "backfill_progress",
            &memories::memory_backfill_progress(&db, &uid),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "rgs");
        check_body(
            "regenerate_status",
            &memories::memory_regenerate_all_status(&db, &uid),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "rga");
        let resp = rt.block_on(memories::memory_regenerate_all(&db, &uid));
        // jobId minted → blank; message compared verbatim (cleared=0).
        check_blanked("regenerate_all", &resp, &["jobId"], &mut failed);
        // The enqueued fan-out row: v4's processor auto-claims it to PROCESSING in
        // the jest env while v5 (no processor) leaves it PENDING — a timing
        // artifact, so `status` is blanked. type + payload are the port's proof.
        let rows = db
            .read_main(|conn| {
                let repo = quilltap_core::db::background_jobs::BackgroundJobsRepository::new(conn);
                let jobs = repo.find_recent_by_type("MEMORY_REGENERATE_ALL", 10)?;
                Ok(json!({
                    "jobs": jobs.iter().map(|j| json!({
                        "type": j.job_type,
                        "status": j.status,
                        "payload": serde_json::from_str::<Value>(&j.payload).unwrap_or(Value::Null),
                    })).collect::<Vec<_>>(),
                }))
            })
            .unwrap();
        let want = &oracle["regenerate_all"]["tables"];
        if norm_blanked(&rows, &["status"]) != norm_blanked(want, &["status"]) {
            eprintln!(
                "[regenerate_all tables] MISMATCH:\n{}",
                first_diff(
                    &norm_blanked(&rows, &["status"]),
                    &norm_blanked(want, &["status"])
                )
            );
            failed.push("regenerate_all_tables".into());
        } else {
            eprintln!("[regenerate_all tables] OK.");
        }
    }
    // --- Embedding status + backfill (tier 2) ---
    {
        let db = fresh_db(&spec, "est");
        check_body(
            "embedding_status",
            &memories::memory_embedding_status(&db, &uid, MNEMO),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "bfs");
        let resp = rt.block_on(memories::memory_backfill_start(
            &db,
            &uid,
            json!({ "characterId": MNEMO, "batchSize": 500 }),
        ));
        check_body("backfill_start", &resp, &mut failed);
        // The order-independent embedding-job dump (count + sorted entityIds + profileIds).
        let dump = db
            .read_main(|conn| {
                let repo = quilltap_core::db::background_jobs::BackgroundJobsRepository::new(conn);
                let jobs = repo.find_recent_by_type("EMBEDDING_GENERATE", 100)?;
                let payloads: Vec<Value> = jobs
                    .iter()
                    .filter_map(|j| serde_json::from_str::<Value>(&j.payload).ok())
                    .collect();
                let mut entity_ids: Vec<String> = payloads
                    .iter()
                    .filter_map(|p| {
                        p.get("entityId")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect();
                entity_ids.sort();
                let mut profile_ids: Vec<String> = payloads
                    .iter()
                    .filter_map(|p| {
                        p.get("profileId")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .collect();
                profile_ids.sort();
                profile_ids.dedup();
                Ok(json!({
                    "count": payloads.len(),
                    "entityIds": entity_ids,
                    "profileIds": profile_ids,
                }))
            })
            .unwrap();
        check_tables("backfill_start", &dump, &mut failed);
    }

    // === P4.D269: list/search rows (append-only) ===
    // v4 `d58548051`: the list's `?tier=` on BOTH paths (the paginated exact
    // `"tier" = ?` lists a NULL-tier row under neither tier; the legacy
    // `(m.tier ?? 'hot') === tier` lists it as hot) + `source=CONSOLIDATED`;
    // the search's `includeCold: true` + the CONSOLIDATED enum + its Zod
    // refusals (R-E). Every row plants `P4D269_PLANT` (the oracle's twin) on
    // its copy after the shared tier migration. Bodies compare by BYTES in
    // wire key order (memory note `serde-json-map-eq-ignores-order`): the
    // search rows round floats to 6 places first, nothing is key-sorted.
    {
        let planted = |tag: &str| {
            let db = fresh_db(&spec, tag);
            rt.block_on(db.write(|w| {
                for sql in P4D269_PLANT {
                    w.main()
                        .connection()
                        .execute_batch(sql)
                        .map_err(quilltap_core::db::DbError::from)?;
                }
                Ok(())
            }))
            .expect("plant the P4.D269 rows");
            db
        };
        let check_bytes = |name: &str, got: &Response, rounded: bool, failed: &mut Vec<String>| {
            let rec = &oracle[name];
            let want_status = rec["status"].as_u64().unwrap_or(0) as u16;
            let (got_status, mut got_body) = match got {
                Response::Error(e) => (
                    status_of(e.kind),
                    e.validation_wire_body()
                        .unwrap_or_else(|| json!({ "error": e.message })),
                ),
                other => (200, response_data(other)),
            };
            let mut want_body = rec["body"].clone();
            for v in [&mut got_body, &mut want_body] {
                if rounded {
                    round_floats(v);
                }
                canon_numbers(v);
            }
            let (g, w) = (
                serde_json::to_string_pretty(&got_body).unwrap(),
                serde_json::to_string_pretty(&want_body).unwrap(),
            );
            if got_status != want_status || g != w {
                eprintln!(
                    "[{name}] MISMATCH (status {got_status} vs {want_status}):\n{}",
                    first_diff(&g, &w)
                );
                failed.push(name.to_string());
            } else {
                eprintln!("[{name}] OK.");
            }
        };
        let list = |tag: &str,
                    search: Option<&str>,
                    source: Option<&str>,
                    tier: Option<&str>,
                    limit: Option<i64>| {
            let db = planted(tag);
            memories::memory_list(
                &db, MNEMO, search, None, source, tier, None, None, limit, None,
            )
        };
        for (name, search, source, tier, limit) in [
            ("list_paginated_tier_hot", None, None, Some("hot"), Some(50)),
            (
                "list_paginated_tier_cold",
                None,
                None,
                Some("cold"),
                Some(50),
            ),
            (
                "list_paginated_tier_bogus",
                None,
                None,
                Some("warm"),
                Some(50),
            ),
            (
                "list_paginated_source_consolidated",
                None,
                Some("CONSOLIDATED"),
                None,
                Some(50),
            ),
            (
                "list_paginated_tier_cold_search",
                Some("reef"),
                None,
                Some("cold"),
                Some(50),
            ),
            (
                "list_paginated_null_tier",
                Some("Unindexed"),
                None,
                Some("hot"),
                Some(50),
            ),
            ("list_legacy_tier_hot", None, None, Some("hot"), None),
            ("list_legacy_tier_cold", None, None, Some("cold"), None),
            (
                "list_legacy_source_consolidated",
                None,
                Some("CONSOLIDATED"),
                None,
                None,
            ),
            (
                "list_legacy_null_tier",
                Some("Unindexed"),
                None,
                Some("hot"),
                None,
            ),
        ] {
            check_bytes(
                name,
                &list(name, search, source, tier, limit),
                false,
                &mut failed,
            );
        }
        let search = |tag: &str, body: Value| {
            let db = planted(tag);
            let p = provider(&db);
            rt.block_on(memories::memory_search(&db, &p, &uid, FIXED_NOW_MS, body))
        };
        for (name, body) in [
            (
                "search_include_cold",
                json!({ "characterId": MNEMO, "query": "smuggler cove hidden by the reef", "limit": 5 }),
            ),
            (
                "search_source_consolidated",
                json!({ "characterId": MNEMO, "query": "Alden trusts the harbor master", "source": "CONSOLIDATED" }),
            ),
            (
                "search_source_bogus",
                json!({ "characterId": MNEMO, "query": "reef", "source": "BOGUS" }),
            ),
            (
                "search_query_empty",
                json!({ "characterId": MNEMO, "query": "" }),
            ),
            (
                "search_bad_uuid",
                json!({ "characterId": "nope", "query": "reef" }),
            ),
            (
                "search_limit_101",
                json!({ "characterId": MNEMO, "query": "reef", "limit": 101 }),
            ),
            (
                "search_many_issues",
                json!({ "characterId": 5, "query": 5, "limit": "5", "minScore": -1, "source": null }),
            ),
        ] {
            check_bytes(name, &search(name, body), true, &mut failed);
        }
        // The two pre-existing search rows, byte-compared too (Tier 2 #15 —
        // `{...memory, score, usedEmbedding, tagDetails}`; their `check_rounded`
        // above sorts keys, so it could not see the order).
        for (name, body) in [
            (
                "search",
                json!({ "characterId": MNEMO, "query": "smuggler cove hidden by the reef", "limit": 5 }),
            ),
            (
                "search_min_importance",
                json!({ "characterId": MNEMO, "query": "lighthouse keeper storm beacon", "limit": 10, "minImportance": 0.7 }),
            ),
        ] {
            let db = fresh_db(&spec, &format!("{name}-bytes"));
            let p = provider(&db);
            let resp = rt.block_on(memories::memory_search(&db, &p, &uid, FIXED_NOW_MS, body));
            check_bytes(name, &resp, true, &mut failed);
        }
    }
    // === end ===

    // === P4.D298: item GET / DELETE, the chat wipe, the counts (append-only) ===
    // Bodies by BYTES in wire key order (memory note
    // `serde-json-map-eq-ignores-order`), refusals as status + the `{error}`
    // body; the route's own `[Memories API…]` lines captured on the caller
    // thread and compared with v4's, in order; tables as the oracle dumps them.
    {
        use quilltap_core::test_support::global_capture;
        let check_row_in = |oracle: &HashMap<String, Value>,
                            name: &str,
                            got: &Response,
                            lines: &[String],
                            tables: Option<Value>,
                            failed: &mut Vec<String>| {
            let rec = &oracle[name];
            let want_status = rec["status"].as_u64().unwrap_or(0) as u16;
            let (got_status, mut got_body) = match got {
                Response::Error(e) => (
                    status_of(e.kind),
                    e.validation_wire_body()
                        .unwrap_or_else(|| json!({ "error": e.message })),
                ),
                other => (200, response_data(other)),
            };
            let mut want_body = rec["body"].clone();
            canon_numbers(&mut got_body);
            canon_numbers(&mut want_body);
            let (g, w) = (
                serde_json::to_string_pretty(&got_body).unwrap(),
                serde_json::to_string_pretty(&want_body).unwrap(),
            );
            let mut bad = Vec::new();
            if got_status != want_status {
                bad.push(format!("status {got_status} vs {want_status}"));
            }
            if g != w {
                bad.push(format!("body:\n{}", first_diff(&g, &w)));
            }
            let want_lines: Vec<String> = rec["logs"]
                .as_array()
                .map(|a| a.iter().map(render_v4_line).collect())
                .unwrap_or_default();
            let got_lines = own_v5_lines(lines);
            if got_lines != want_lines {
                bad.push(format!(
                    "lines:\n--- v4 ---\n{}\n--- v5 ---\n{}",
                    want_lines.join("\n"),
                    got_lines.join("\n")
                ));
            }
            if let Some(t) = tables {
                let (gt, wt) = (
                    serde_json::to_string_pretty(&t).unwrap(),
                    serde_json::to_string_pretty(&rec["tables"]).unwrap(),
                );
                if gt != wt {
                    bad.push(format!("tables:\n{}", first_diff(&gt, &wt)));
                }
            }
            if bad.is_empty() {
                eprintln!("[{name}] OK (P4.D298).");
            } else {
                eprintln!("[{name}] MISMATCH (P4.D298):\n{}", bad.join("\n"));
                failed.push(name.to_string());
            }
        };
        let check_row = |name: &str,
                         got: &Response,
                         lines: &[String],
                         tables: Option<Value>,
                         failed: &mut Vec<String>| {
            check_row_in(&oracle, name, got, lines, tables, failed)
        };
        // The sibling chokepoints' lines, by PRESENCE on v4's side (v5's run on
        // the writer thread — invisible to the caller-thread rig).
        let expect_sibling = |name: &str, msg: &str, failed: &mut Vec<String>| {
            let present = oracle[name]["siblingMessages"]
                .as_array()
                .map(|a| a.iter().any(|m| m.as_str() == Some(msg)))
                .unwrap_or(false);
            if !present {
                eprintln!("[{name}] v4 sibling line absent: {msg}");
                failed.push(format!("{name}:sibling"));
            }
        };

        // Item GET — bug 219: no stamp; v4's DEBUG.
        for name in ["get", "get_does_not_stamp"] {
            let db = fresh_db(&spec, name);
            let (resp, lines) =
                global_capture::capture(|| rt.block_on(memories::memory_get(&db, MEM_TAGGED_2)));
            let tables = (name == "get_does_not_stamp").then(|| {
                let rows = dump_all_memories(&db);
                let row = rows
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["id"] == MEM_TAGGED_2)
                    .cloned();
                json!({
                    "present": row.is_some(),
                    "lastAccessedAt": row.map(|r| r["lastAccessedAt"].clone()).unwrap_or(Value::Null),
                })
            });
            check_row(name, &resp, &lines, tables, &mut failed);
        }
        {
            let db = fresh_db(&spec, "getm-d298");
            let (resp, lines) =
                global_capture::capture(|| rt.block_on(memories::memory_get(&db, MISSING)));
            check_row("get_missing", &resp, &lines, None, &mut failed);
        }
        // Item DELETE through the ONE retire path; the INFO.
        {
            let db = fresh_db(&spec, "del-d298");
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_delete(&db, &uid, MEM_REL_B, FIXED_NOW_MS))
            });
            check_row("delete", &resp, &lines, None, &mut failed);
            expect_sibling("delete", "memory:retire|Retired memories", &mut failed);
        }
        // `retired === 0` → 404 (a trigger swallows the delete).
        {
            let name = "delete_race_retired_zero";
            let db = fresh_db(&spec, name);
            plant(&rt, &db, P4D298_DELETE_RACE_PLANT);
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_delete(&db, &uid, MEM_REL_B, FIXED_NOW_MS))
            });
            let tables = dump_rows(&db, &["b2000000-0000-4000-8000-0000000000e0", MEM_REL_B]);
            // `dump_rows` sorts nothing — the oracle's `dumpMemoryRows` shape.
            let mut t = tables;
            canon_numbers(&mut t);
            check_row(name, &resp, &lines, Some(t), &mut failed);
        }
        // The chat wipe — bug 201's spares, `sparedCount`, the renamed DEBUG.
        {
            let db = fresh_db(&spec, "dbc-d298");
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_delete_by_chat(&db, CHAT_SALON))
            });
            check_row("delete_by_chat", &resp, &lines, None, &mut failed);
            expect_sibling(
                "delete_by_chat",
                "memory:retire|Wiped a chat's extracted memories",
                &mut failed,
            );
        }
        {
            let name = "delete_by_chat_spares_manual_cold_digest";
            let db = fresh_db(&spec, name);
            plant(&rt, &db, P4D298_SPARE_PLANT);
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_delete_by_chat(&db, CHAT_SALON))
            });
            check_row(
                name,
                &resp,
                &lines,
                Some(dump_all_memories(&db)),
                &mut failed,
            );
            expect_sibling(
                name,
                "memory:retire|Chat wipe spared manual, cold and digest rows",
                &mut failed,
            );
        }
        {
            let db = fresh_db(&spec, "dbcm-d298");
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_delete_by_chat(&db, MISSING))
            });
            check_row("delete_by_chat_missing", &resp, &lines, None, &mut failed);
        }
        // The counts — bug 210's four keys and two-key sort.
        {
            let db = fresh_db(&spec, "cc-d298");
            let (resp, lines) =
                global_capture::capture(|| memories::memory_character_counts(&db, &uid));
            check_row("character_counts", &resp, &lines, None, &mut failed);
        }
        {
            let name = "character_counts_breakdown";
            let db = fresh_db(&spec, name);
            plant(&rt, &db, P4D298_COUNTS_PLANT);
            let (resp, lines) =
                global_capture::capture(|| memories::memory_character_counts(&db, &uid));
            check_row(name, &resp, &lines, None, &mut failed);
        }

        // --- The three re-attribution / cascade lines, each with its silence
        // leg (the retire succeeding) and a forced retire failure for the two
        // ERRORs (a `RAISE(ABORT)` trigger on every memory delete).
        {
            const MSG_S2A_TARGET: &str = "e1000000-0000-4000-8000-000000000002";
            const SOURCE_P: &str = "e1000000-0000-4000-8000-000000000001";
            for (name, fails) in [
                ("message_delete_cascade_line", false),
                ("reattribute_line_silent", false),
                ("reattribute_retire_fails_line", true),
                ("bulk_reattribute_line_silent", false),
                ("bulk_reattribute_retire_fails_line", true),
            ] {
                let db = fresh_db(&spec, name);
                if fails {
                    plant(&rt, &db, P4D298_RETIRE_FAILS_PLANT);
                }
                let (resp, lines) = global_capture::capture(|| {
                    if name.starts_with("message_delete") {
                        rt.block_on(quilltap_core::api::salon::message_delete(
                            &db,
                            &uid,
                            MSG_S2A,
                            Some("DELETE_MEMORIES"),
                            false,
                        ))
                    } else if name.starts_with("reattribute") {
                        rt.block_on(
                            quilltap_core::services::message_reattribute::message_reattribute(
                                &db,
                                &uid,
                                MSG_S2A,
                                MSG_S2A_TARGET,
                            ),
                        )
                    } else {
                        rt.block_on(quilltap_core::services::chat_admin::chat_bulk_reattribute(
                            &db,
                            CHAT_SALON,
                            Some(SOURCE_P),
                            MSG_S2A_TARGET,
                            None,
                        ))
                    }
                });
                let rec = &oracle[name];
                let got_status = match &resp {
                    Response::Error(e) => status_of(e.kind),
                    _ => 200,
                };
                let want_lines: Vec<String> = rec["logs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|l| without_target(&render_v4_line(l)))
                    .filter(|l| is_reattribution_line(l.split_once(' ').map_or("", |x| x.1)))
                    .collect();
                let got_lines: Vec<String> = lines
                    .iter()
                    .map(|l| without_target(l))
                    .filter(|l| is_reattribution_line(l.split_once(' ').map_or("", |x| x.1)))
                    .collect();
                // `vectorsRemoved` compares WHOLE: the lane's `CASCADE_VECTORS_REMOVED`
                // pin (v4 `0`, v5 `2`) measured v4's PROCESS-singleton index cache
                // leaking across the oracle's per-case copies; with the cache
                // reloaded per case v4 answers `2` too (the `83334b9a5` unification).
                let mut bad = Vec::new();
                let (gt, wt) = (
                    serde_json::to_string_pretty(&dump_all_memories(&db)).unwrap(),
                    serde_json::to_string_pretty(&rec["tables"]).unwrap(),
                );
                if got_status as u64 != rec["status"].as_u64().unwrap() {
                    bad.push(format!("status {got_status} vs {}", rec["status"]));
                }
                if got_lines != want_lines {
                    bad.push(format!(
                        "lines:\n--- v4 ---\n{}\n--- v5 ---\n{}",
                        want_lines.join("\n"),
                        got_lines.join("\n")
                    ));
                }
                if gt != wt {
                    bad.push(format!("tables:\n{}", first_diff(&gt, &wt)));
                }
                // The capture leg must have seen the line it pins (and the
                // silence leg none): never a vacuous equal.
                let pinned = want_lines.len();
                let expected = usize::from(name.starts_with("message_delete") || fails);
                if pinned != expected {
                    bad.push(format!(
                        "v4 logged {pinned} pinned lines, the row expects {expected}"
                    ));
                }
                if bad.is_empty() {
                    eprintln!("[{name}] OK (P4.D298).");
                } else {
                    eprintln!("[{name}] MISMATCH (P4.D298):\n{}", bad.join("\n"));
                    failed.push(name.to_string());
                }
            }
        }

        // --- L-6: `z.iso.datetime({ offset: true })`, tier-1 against the pin's zod.
        {
            let rows = oracle["iso_datetime_offset_rows"]["rows"]
                .as_array()
                .unwrap();
            let mut bad = Vec::new();
            for row in rows {
                let input = row["input"].as_str().unwrap();
                let want = row["ok"].as_bool().unwrap();
                let got = quilltap_core::api::zod_issues::zod_iso_datetime_offset_ok(input);
                if got != want {
                    bad.push(format!("{input}: v5 {got} / v4 {want}"));
                }
            }
            if rows.len() < 15 || !bad.is_empty() {
                eprintln!("[iso_datetime_offset_rows] MISMATCH: {bad:?}");
                failed.push("iso_datetime_offset_rows".into());
            } else {
                eprintln!("[iso_datetime_offset_rows] OK ({} rows).", rows.len());
            }
        }

        // --- The manual create through admission (bug 218 / 236). Bodies by
        // bytes with the minted keys masked; refusals as status + `error` (L-1:
        // v4's `details` are the recorded no-details class); the route's lines
        // with the minted `memoryId` masked; the row count around the call.
        let create_cases: Vec<(&str, Value, Option<&'static [&'static str]>, bool)> = vec![
            (
                "create_insert",
                json!({
                    "characterId": MNEMO,
                    "content": "The clocktower chimes thirteen times on the winter solstice.",
                    "summary": "The clocktower chimes thirteen at solstice.",
                    "keywords": ["clocktower", "solstice"],
                    "importance": 0.55,
                    "source": "MANUAL",
                }),
                None,
                true,
            ),
            (
                "create_near_duplicate",
                json!({
                    "characterId": MNEMO,
                    "content": "Mnemo always double-checks the barometer before hoisting the mainsail.",
                    "summary": "Mnemo checks the barometer before the mainsail.",
                    "source": "AUTO",
                }),
                None,
                true,
            ),
            (
                "create_manual_trims",
                json!({
                    "characterId": MNEMO,
                    "content": "\u{a0}  The lamp oil sits under the third stair. \n",
                    "summary": "\u{feff}Lamp oil under the third stair.\u{85}",
                    "keywords": ["lamp", "oil"],
                }),
                None,
                true,
            ),
            (
                "create_manual_trims_no_keywords",
                json!({
                    "characterId": MNEMO,
                    "content": "\u{a0}  The lamp oil sits under the third stair. \n",
                    "summary": "\u{feff}Lamp oil under the third stair.\u{85}",
                }),
                None,
                true,
            ),
            (
                "create_plain_lamp_text",
                json!({
                    "characterId": MNEMO,
                    "content": "The lamp oil is kept under the third stair.",
                    "summary": "Lamp oil under the third stair.",
                }),
                None,
                true,
            ),
            (
                "create_blank_after_trim",
                json!({ "characterId": MNEMO, "content": "   \t", "summary": "x" }),
                None,
                true,
            ),
        ];
        let base = json!({
            "characterId": MNEMO,
            "content": "The tide clock above the chandlery runs nine minutes fast.",
            "summary": "The chandlery tide clock runs fast.",
        });
        let with = |extra: Value| -> Value {
            let mut b = base.clone();
            for (k, v) in extra.as_object().unwrap() {
                b[k] = v.clone();
            }
            b
        };
        const CHAT_PROJECT_PLANT: &[&str] = &[
            "UPDATE chats SET projectId = 'b7000000-0000-4000-8000-000000000001' WHERE id = 'c1000000-0000-4000-8000-000000000001'",
        ];
        let mut create_cases = create_cases;
        create_cases.extend([
            (
                "create_project_derived_from_chat",
                with(json!({ "chatId": CHAT_SALON })),
                Some(CHAT_PROJECT_PLANT),
                true,
            ),
            (
                "create_missing_project_404",
                with(json!({ "projectId": MISSING })),
                None,
                true,
            ),
            (
                "create_missing_chat_404",
                with(json!({ "chatId": MISSING })),
                None,
                true,
            ),
            (
                "create_missing_character_first_404",
                with(json!({ "characterId": MISSING, "chatId": MISSING })),
                None,
                true,
            ),
            (
                "create_kind_occurred_entities",
                with(json!({
                    "kind": "episodic",
                    "occurredAt": "2026-03-01T10:00:00.000Z",
                    "entities": ["Alden", "harbor"],
                })),
                None,
                true,
            ),
            (
                "create_offset_occurred_at_row_refusal",
                with(json!({ "occurredAt": "2026-03-01T10:00:00+05:30" })),
                None,
                true,
            ),
            (
                "create_bad_offset_400",
                with(json!({ "occurredAt": "2026-03-01T10:00:00-0800" })),
                None,
                true,
            ),
            (
                "create_source_skipgate_stripped",
                with(json!({ "source": "AUTO", "skipGate": true })),
                None,
                true,
            ),
            (
                "create_source_null_logged",
                with(json!({ "source": null })),
                None,
                true,
            ),
            (
                "create_source_manual_silent",
                with(json!({ "source": "MANUAL" })),
                None,
                true,
            ),
            (
                "create_reinforce_200",
                json!({
                    "characterId": MNEMO,
                    "content": "Alden the lighthouse keeper trusts only the harbor master.",
                    "summary": "Alden trusts the harbor master.",
                }),
                None,
                true,
            ),
            ("create_embedding_failed_502", base.clone(), None, false),
            (
                "create_entities_over_cap_400",
                with(json!({
                    "entities": (0..13).map(|i| format!("entity-{i}")).collect::<Vec<_>>(),
                })),
                None,
                true,
            ),
            // P4.D311 — zod 4.6.7's `capSize`: past the cap, ONLY the array's
            // `too_big`; no element issue first.
            (
                "create_entities_over_cap_non_string_first_400",
                with(json!({
                    "entities": std::iter::once(json!(7))
                        .chain((0..12).map(|i| json!(format!("entity-{i}"))))
                        .collect::<Vec<_>>(),
                })),
                None,
                true,
            ),
            (
                "create_entities_over_cap_empty_first_400",
                with(json!({
                    "entities": std::iter::once(String::new())
                        .chain((0..12).map(|i| format!("entity-{i}")))
                        .collect::<Vec<_>>(),
                })),
                None,
                true,
            ),
        ]);
        for (name, body, plant_sql, with_provider) in create_cases {
            let db = fresh_db(&spec, &format!("{name}-d298"));
            if let Some(sql) = plant_sql {
                plant(&rt, &db, sql);
            }
            let count = |db: &Db| {
                db.read_main(|c| {
                    c.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get::<_, i64>(0))
                        .map_err(Into::into)
                })
                .unwrap()
            };
            let before = count(&db);
            let p = provider(&db);
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_create(
                    &db,
                    with_provider.then_some(&p),
                    &uid,
                    FIXED_NOW_MS,
                    "UTC",
                    body,
                ))
            });
            let after = count(&db);
            let rec = &oracle[name];
            let mut bad = Vec::new();
            let want_status = rec["status"].as_u64().unwrap();
            // `create_plain_lamp_text` compares like every create row: the lane's
            // `ADMISSION_SINGLE_TERM_LINK` pin (v4 `insert` at `bestCosine: 0`, v5
            // `insert-linked`) measured v4's neighbour search over the PROCESS-
            // singleton index cache another case's copy had loaded; reloaded per
            // case, v4 links `f1` too (the `83334b9a5` unification).
            if let Some((_, v4_status, v5_action)) = OFFSET_OCCURRED_AT_ROW_SCHEMA
                .iter()
                .find(|(n, _, _)| *n == name)
            {
                let got_action = response_data(&resp)["action"].clone();
                if want_status == *v4_status && got_action == json!(v5_action) {
                    eprintln!(
                        "[{name}] OFFSET_OCCURRED_AT_ROW_SCHEMA holds (v4 {v4_status}, v5 {v5_action})."
                    );
                } else {
                    eprintln!(
                        "[{name}] OFFSET_OCCURRED_AT_ROW_SCHEMA moved: v4 {want_status}, v5 {got_action}"
                    );
                    failed.push(format!("{name}:offset-row-schema"));
                }
                continue;
            }
            // Status + body (the L-5 carrier for the 502; L-1 for the 400s).
            match &resp {
                Response::Error(e) => {
                    let bgw = MEMORY_BAD_GATEWAY_AS_INTERNAL.contains(&name);
                    let carried = e.kind == ErrorKind::Internal
                        && e.details.as_deref() == Some(&json!({ "status": 502 }));
                    if bgw {
                        if want_status != 502 || !carried {
                            bad.push(format!(
                                "MEMORY_BAD_GATEWAY_AS_INTERNAL moved: v4 {want_status}, v5 {:?} {:?}",
                                e.kind, e.details
                            ));
                        }
                    } else if u64::from(status_of(e.kind)) != want_status {
                        bad.push(format!("status {:?} vs {want_status}", e.kind));
                    }
                    if rec["body"]["error"].as_str() != Some(e.message.as_str()) {
                        bad.push(format!("error {:?} vs {}", e.message, rec["body"]["error"]));
                    }
                }
                other => {
                    let mut got = response_data(other);
                    let mut want = rec["body"].clone();
                    mask_minted_memory(&mut got);
                    mask_minted_memory(&mut want);
                    canon_numbers(&mut got);
                    canon_numbers(&mut want);
                    let (g, w) = (norm(&got), norm(&want));
                    // By BYTES: `MemorySchema` key order (strict since the
                    // `83334b9a5` unification — `shift_remove` landed).
                    let (gs, ws) = (
                        serde_json::to_string(&got).unwrap(),
                        serde_json::to_string(&want).unwrap(),
                    );
                    if gs != ws {
                        bad.push(format!("key order / bytes:\n  v5 {gs}\n  v4 {ws}"));
                    }
                    if want_status != 200 && want_status != 201 {
                        bad.push(format!("v5 answered 200, v4 {want_status}"));
                    }
                    // 201 ⇔ an insert (`admissionInserted`): the split rides `action`.
                    let inserted =
                        matches!(got["action"].as_str(), Some("insert" | "insert-linked"));
                    if (want_status == 201) != inserted {
                        bad.push(format!(
                            "201/200 split: v4 {want_status}, action {}",
                            got["action"]
                        ));
                    }
                    if g != w {
                        let keys = |v: &Value| -> Vec<String> {
                            v["memory"]
                                .as_object()
                                .map(|o| o.keys().cloned().collect())
                                .unwrap_or_default()
                        };
                        bad.push(format!(
                            "body (keys sorted):\n{}  keys v5: {:?}\n  keys v4: {:?}",
                            first_diff(&g, &w),
                            keys(&got),
                            keys(&want)
                        ));
                    }
                }
            }
            let want_lines: Vec<String> = rec["logs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| mask_field(&render_v4_line(l), "memoryId"))
                .collect();
            let got_lines: Vec<String> = own_v5_lines(&lines)
                .iter()
                .map(|l| mask_field(l, "memoryId"))
                .collect();
            if got_lines != want_lines {
                bad.push(format!(
                    "lines:\n--- v4 ---\n{}\n--- v5 ---\n{}",
                    want_lines.join("\n"),
                    got_lines.join("\n")
                ));
            }
            let want_rows = (
                rec["tables"]["rowsBefore"].as_i64(),
                rec["tables"]["rowsAfter"].as_i64(),
            );
            if let (Some(wb), Some(wa)) = want_rows {
                // The oracle counts AFTER its plant; the row delta is the comparand.
                if after - before != wa - wb {
                    bad.push(format!(
                        "rows written: v5 {} / v4 {}",
                        after - before,
                        wa - wb
                    ));
                }
            }
            if bad.is_empty() {
                eprintln!("[{name}] OK (P4.D298).");
            } else {
                eprintln!("[{name}] MISMATCH (P4.D298):\n{}", bad.join("\n"));
                failed.push(format!("{name}:d298"));
            }
        }

        // --- The item PUT through `update_memory` (bug 203 / 236). The provider
        // message after `Could not embed the edited memory: ` is provider-specific
        // and MASKED (L-2); `updatedAt` masked; floats (the re-embedded vector)
        // rounded to 6 places; bodies by BYTES (update_memory answers
        // `MemorySchema` order).
        const NO_PROFILE: &[&str] = &["DELETE FROM embedding_profiles"];
        let mask_embed_message = |s: &str| -> String {
            const PREFIX: &str = "Could not embed the edited memory: ";
            match s.find(PREFIX) {
                Some(i) => format!("{}{PREFIX}<provider message>", &s[..i]),
                None => s.to_string(),
            }
        };
        for (name, id, body, plant_sql) in [
            (
                "update",
                MEM_TAGGED_1,
                json!({ "importance": 0.95, "summary": "Alden, the stern mentor." }),
                None,
            ),
            (
                "update_text_reembeds",
                MEM_TAGGED_1,
                json!({ "content": "The old captain Alden mentored every apprentice at the harbor." }),
                None,
            ),
            (
                "update_importance_only_no_profile",
                MEM_TAGGED_1,
                json!({ "importance": 0.35 }),
                Some(NO_PROFILE),
            ),
            (
                "update_text_no_profile_502",
                MEM_TAGGED_1,
                json!({ "summary": "Alden mentored them all." }),
                Some(NO_PROFILE),
            ),
            (
                "update_invalid_related_400",
                MEM_TAGGED_1,
                json!({ "relatedMemoryIds": [MISSING] }),
                None,
            ),
            (
                "update_related_links_both_ways",
                MEM_TAGGED_1,
                json!({ "relatedMemoryIds": [MEM_TAGGED_2] }),
                None,
            ),
            (
                "update_blank_after_trim_400",
                MEM_TAGGED_1,
                json!({ "summary": " \u{a0} " }),
                None,
            ),
            (
                "update_bad_body_missing_memory_404",
                MISSING,
                json!({ "importance": "very" }),
                None,
            ),
            (
                "update_null_about_character",
                MEM_TAGGED_1,
                json!({ "aboutCharacterId": null, "keywords": ["alden"] }),
                None,
            ),
        ] {
            let db = fresh_db(&spec, &format!("{name}-d298"));
            if let Some(sql) = plant_sql {
                plant(&rt, &db, sql);
            }
            let p = provider(&db);
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_update(
                    &db,
                    Some(&p),
                    &uid,
                    id,
                    "UTC",
                    FIXED_NOW_MS,
                    body,
                ))
            });
            let rec = &oracle[name];
            let want_status = rec["status"].as_u64().unwrap();
            let mut bad = Vec::new();
            match &resp {
                Response::Error(e) => {
                    if MEMORY_BAD_GATEWAY_AS_INTERNAL.contains(&name) {
                        let carried = e.kind == ErrorKind::Internal
                            && e.details.as_deref() == Some(&json!({ "status": 502 }));
                        if want_status != 502 || !carried {
                            bad.push(format!(
                                "MEMORY_BAD_GATEWAY_AS_INTERNAL moved: v4 {want_status}, v5 {:?} {:?}",
                                e.kind, e.details
                            ));
                        }
                    } else if u64::from(status_of(e.kind)) != want_status {
                        bad.push(format!("status {:?} vs {want_status}", e.kind));
                    }
                    let want_error =
                        mask_embed_message(rec["body"]["error"].as_str().unwrap_or(""));
                    if mask_embed_message(&e.message) != want_error {
                        bad.push(format!("error {:?} vs {want_error:?}", e.message));
                    }
                }
                other => {
                    let mut got = response_data(other);
                    let mut want = rec["body"].clone();
                    for v in [&mut got, &mut want] {
                        if let Some(m) = v.get_mut("memory").and_then(Value::as_object_mut) {
                            m.insert("updatedAt".into(), json!("<minted>"));
                        }
                        round_floats(v);
                        canon_numbers(v);
                    }
                    let (g, w) = (
                        serde_json::to_string_pretty(&got).unwrap(),
                        serde_json::to_string_pretty(&want).unwrap(),
                    );
                    if want_status != 200 {
                        bad.push(format!("v5 answered 200, v4 {want_status}"));
                    }
                    if g != w {
                        bad.push(format!("body:\n{}", first_diff(&g, &w)));
                    }
                }
            }
            let render = |l: &str| mask_embed_message(l);
            let want_lines: Vec<String> = rec["logs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| render(&render_v4_line(l)))
                .collect();
            let got_lines: Vec<String> = own_v5_lines(&lines).iter().map(|l| render(l)).collect();
            if got_lines != want_lines {
                bad.push(format!(
                    "lines:\n--- v4 ---\n{}\n--- v5 ---\n{}",
                    want_lines.join("\n"),
                    got_lines.join("\n")
                ));
            }
            // The edited row and its neighbour as the oracle dumps them.
            let dumped: Vec<Value> = dump_all_memories(&db)
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["id"] == id || r["id"] == MEM_TAGGED_2)
                .cloned()
                .collect();
            let (gt, wt) = (
                serde_json::to_string_pretty(&dumped).unwrap(),
                serde_json::to_string_pretty(&rec["tables"]).unwrap(),
            );
            // (The pre-P4.D298 `update` row records no tables.)
            if rec.get("tables").is_some() && gt != wt {
                bad.push(format!("tables:\n{}", first_diff(&gt, &wt)));
            }
            if bad.is_empty() {
                eprintln!("[{name}] OK (P4.D298).");
            } else {
                eprintln!("[{name}] MISMATCH (P4.D298):\n{}", bad.join("\n"));
                failed.push(format!("{name}:d298"));
            }
        }

        // --- §R.7 `RETIRED_MEMORY_VERB_UNKNOWN_VARIANT` (both halves).
        for (row, verb, wire_type) in RETIRED_MEMORY_VERB_UNKNOWN_VARIANT {
            let rec = &oracle[*row];
            let mut bad = Vec::new();
            let actions: Vec<&str> = rec["body"]["availableActions"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if rec["status"] != 400
                || rec["body"]["error"] != json!(format!("Unknown action: {verb}"))
                || actions.is_empty()
                || actions.contains(verb)
            {
                bad.push(format!("v4 half moved: {} {}", rec["status"], rec["body"]));
            }
            match serde_json::from_value::<quilltap_core::api::types::Request>(
                json!({ "type": wire_type }),
            ) {
                Err(e)
                    if e.to_string()
                        .contains(&format!("unknown variant `{wire_type}`")) => {}
                other => bad.push(format!("v5 half moved: {other:?}")),
            }
            if bad.is_empty() {
                eprintln!("[{row}] OK (RETIRED_MEMORY_VERB_UNKNOWN_VARIANT).");
            } else {
                eprintln!(
                    "[{row}] RETIRED_MEMORY_VERB_UNKNOWN_VARIANT: {}",
                    bad.join("; ")
                );
                failed.push(format!("{row}:retired-verb"));
            }
        }

        // --- The list's `filterImportance`; the search's `TOOL_MIN_IMPORTANCE`.
        let mut residue_rows: Vec<(&str, bool, String)> = Vec::new();
        for (name, limit) in [
            ("list_min_importance_reinforced_legacy", None),
            ("list_min_importance_reinforced_paginated", Some(50)),
        ] {
            let db = fresh_db(&spec, name);
            plant(&rt, &db, P4D298_REINFORCED_PLANT);
            let resp = memories::memory_list(
                &db,
                MNEMO,
                None,
                Some(0.7),
                None,
                None,
                Some("importance"),
                Some("asc"),
                limit,
                None,
            );
            if S5_RESIDUE.iter().any(|(n, _)| *n == name) {
                let (g, w) = (
                    serde_json::to_string_pretty(&response_data(&resp)).unwrap(),
                    serde_json::to_string_pretty(&oracle[name]["body"]).unwrap(),
                );
                residue_rows.push((name, g == w, first_diff(&g, &w)));
            } else {
                check_row(name, &resp, &[], None, &mut failed);
            }
        }

        // A §S.5 residue row: compared, then judged against `S5_RESIDUE`.
        let residue_or_strict = |name: &str, ok: bool, detail: String, failed: &mut Vec<String>| {
            match (S5_RESIDUE.iter().find(|(n, _)| *n == name), ok) {
                (None, true) => eprintln!("[{name}] OK (P4.D298)."),
                (None, false) => {
                    eprintln!("[{name}] MISMATCH (P4.D298):\n{detail}");
                    failed.push(name.to_string());
                }
                (Some((_, owner)), false) => {
                    eprintln!("[{name}] RESIDUE ({owner} — lands at the unification):\n{detail}")
                }
                (Some((_, owner)), true) => {
                    eprintln!(
                        "[{name}] the {owner} residue CLOSED on this branch — move the row to the strict set"
                    );
                    failed.push(format!("{name}:residue-closed"));
                }
            }
        };
        for (name, ok, detail) in residue_rows.drain(..) {
            residue_or_strict(name, ok, detail, &mut failed);
        }
        {
            let name = "search_min_importance_default";
            let db = fresh_db(&spec, name);
            plant(&rt, &db, P4D298_NULL_REINFORCED_PLANT);
            let p = provider(&db);
            let resp = rt.block_on(memories::memory_search(
                &db,
                &p,
                &uid,
                FIXED_NOW_MS,
                json!({ "characterId": MNEMO, "query": "smuggler cove hidden by the reef", "limit": 5 }),
            ));
            let mut got = response_data(&resp);
            let mut want = oracle[name]["body"].clone();
            for v in [&mut got, &mut want] {
                round_floats(v);
                canon_numbers(v);
            }
            let (g, w) = (
                serde_json::to_string_pretty(&got).unwrap(),
                serde_json::to_string_pretty(&want).unwrap(),
            );
            let head: String = g.chars().take(400).collect();
            residue_or_strict(
                name,
                g == w,
                format!("{}  GOT (head): {head}", first_diff(&g, &w)),
                &mut failed,
            );
        }

        // --- The extraction limits through C1-5's ONE patch parser (L-1's
        // envelope: status + `error`; v4's `details` are the recorded
        // no-details class); bug 244's patch keeps an unsent key.
        let check_stored = |name: &str, db: &Db, failed: &mut Vec<String>| {
            let stored = db
                .read_main(quilltap_core::db::instance_settings::get_memory_extraction_limits)
                .expect("read limits");
            let want = &oracle[name]["storedAfter"];
            let (g, w) = (
                serde_json::to_string(&stored).unwrap(),
                serde_json::to_string(want).unwrap(),
            );
            if g != w {
                eprintln!("[{name}] storedAfter MISMATCH:\n  GOT : {g}\n  WANT: {w}");
                failed.push(format!("{name}:storedAfter"));
            }
        };
        let strip_details = |name: &str| {
            let mut rec = oracle[name].clone();
            if let Some(b) = rec["body"].as_object_mut() {
                b.remove("details");
            }
            rec
        };
        for (name, body, seed_floor) in [
            ("extraction_limits_set", json!({ "maxPerHour": 50 }), None),
            (
                "extraction_limits_patch_keeps_soft_floor",
                json!({ "maxPerHour": 40 }),
                Some(0.42),
            ),
            (
                "extraction_limits_set_invalid_type",
                json!({ "maxPerHour": "many" }),
                None,
            ),
            (
                "extraction_limits_set_out_of_range",
                json!({ "softFloor": 1.5 }),
                None,
            ),
            (
                "extraction_limits_set_over_max",
                json!({ "maxPerHour": 10001 }),
                None,
            ),
            (
                "extraction_limits_set_null",
                json!({ "softFloor": null }),
                None,
            ),
        ] {
            let db = fresh_db(&spec, name);
            if let Some(floor) = seed_floor {
                let bag = json!({ "enabled": true, "maxPerHour": 77, "softStartFraction": 0.25, "softFloor": floor });
                rt.block_on(db.write(move |w| {
                    quilltap_core::db::instance_settings::set_memory_extraction_limits(
                        w.main().connection(),
                        &bag,
                    )
                }))
                .expect("seed limits");
            }
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_extraction_limits_set(&db, body))
            });
            let rec = strip_details(name);
            let mut local = oracle.clone();
            local.insert(name.to_string(), rec);
            check_row_in(&local, name, &resp, &lines, None, &mut failed);
            if oracle[name].get("storedAfter").is_some() {
                check_stored(name, &db, &mut failed);
            }
        }
        // The recall INFO (a Tier-2 pre-existing gap closed).
        for (name, body) in [
            ("recall_config_set", json!({ "scopePolicy": "exclude" })),
            (
                "recall_config_set_per_turn",
                json!({ "perTurnConversationSummaries": true }),
            ),
        ] {
            let db = fresh_db(&spec, &format!("{name}-d298"));
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_recall_config_set(&db, body))
            });
            check_row(name, &resp, &lines, None, &mut failed);
        }

        // --- POST `embeddings` without `batchSize` (Zod strips any value).
        for (name, batch) in [
            ("embeddings_batchsize_ignored_999", json!(999)),
            ("embeddings_batchsize_ignored_string", json!("x")),
            ("embeddings_batchsize_ignored_zero", json!(0)),
        ] {
            // The dispatch layer drops the unknown key exactly as Zod strips it.
            let req: quilltap_core::api::types::Request = serde_json::from_value(json!({
                "type": "memoryGenerateEmbeddings",
                "characterId": MNEMO,
                "batchSize": batch,
            }))
            .expect("serde ignores `batchSize`");
            let quilltap_core::api::types::Request::MemoryGenerateEmbeddings { character_id } = req
            else {
                unreachable!()
            };
            let db = fresh_db(&spec, name);
            let p = provider(&db);
            let resp = rt.block_on(memories::memory_generate_embeddings(
                &db,
                &p,
                &uid,
                &character_id,
                "UTC",
                FIXED_NOW_MS,
            ));
            check_row(name, &resp, &[], None, &mut failed);
        }
        // PUT `embeddings` — `failed` = `countWithoutEmbedding` at the pin.
        {
            let name = "rebuild_index_failed_counts_unembedded";
            let db = fresh_db(&spec, name);
            let resp = rt.block_on(memories::memory_rebuild_index(&db, MNEMO, true));
            let (g, w) = (
                serde_json::to_string(&response_data(&resp)).unwrap(),
                serde_json::to_string(&oracle[name]["body"]).unwrap(),
            );
            if g == w {
                eprintln!("[{name}] OK (P4.D298).");
            } else {
                eprintln!("[{name}] MISMATCH (P4.D298):\n  GOT : {g}\n  WANT: {w}");
                failed.push(name.to_string());
            }
        }

        // --- The backfill through ONE deduped batch (`enqueueMissingEmbeddings`).
        let embedding_jobs = |db: &Db| -> Value {
            db.read_main(|conn| {
                let repo = quilltap_core::db::background_jobs::BackgroundJobsRepository::new(conn);
                let jobs = repo.find_recent_by_type("EMBEDDING_GENERATE", 100)?;
                let payloads: Vec<Value> = jobs
                    .iter()
                    .filter_map(|j| serde_json::from_str::<Value>(&j.payload).ok())
                    .collect();
                let mut entity_ids: Vec<String> = payloads
                    .iter()
                    .filter_map(|p| p["entityId"].as_str().map(str::to_string))
                    .collect();
                entity_ids.sort();
                let mut profile_ids: Vec<String> = payloads
                    .iter()
                    .filter_map(|p| p["profileId"].as_str().map(str::to_string))
                    .collect();
                profile_ids.sort();
                profile_ids.dedup();
                Ok(json!({ "count": payloads.len(), "entityIds": entity_ids, "profileIds": profile_ids }))
            })
            .unwrap()
        };
        {
            let name = "backfill_start";
            let db = fresh_db(&spec, &format!("{name}-d298"));
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_backfill_start(
                    &db,
                    &uid,
                    json!({ "characterId": MNEMO, "batchSize": 500 }),
                ))
            });
            check_row(name, &resp, &lines, Some(embedding_jobs(&db)), &mut failed);
        }
        {
            let name = "backfill_start_skips_queued";
            let db = fresh_db(&spec, name);
            let (resp, lines) = global_capture::capture(|| {
                let bag = json!({ "characterId": MNEMO, "batchSize": 500 });
                let _ = rt.block_on(memories::memory_backfill_start(&db, &uid, bag.clone()));
                rt.block_on(memories::memory_backfill_start(&db, &uid, bag))
            });
            check_row(name, &resp, &lines, Some(embedding_jobs(&db)), &mut failed);
        }
        {
            let name = "backfill_start_all_characters";
            let db = fresh_db(&spec, name);
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_backfill_start(&db, &uid, json!({})))
            });
            check_row(name, &resp, &lines, Some(embedding_jobs(&db)), &mut failed);
        }
        {
            let name = "backfill_start_bad_character_id";
            let db = fresh_db(&spec, name);
            let (resp, lines) = global_capture::capture(|| {
                rt.block_on(memories::memory_backfill_start(
                    &db,
                    &uid,
                    json!({ "characterId": "nope", "batchSize": 2001 }),
                ))
            });
            let mut local = oracle.clone();
            local.insert(name.to_string(), strip_details(name));
            check_row_in(&local, name, &resp, &lines, None, &mut failed);
        }

        // --- Regenerate-all: three types cleared; the retired type survives.
        let jobs_of = |db: &Db, t: &'static str| -> Value {
            db.read_main(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT type, status, payload FROM background_jobs WHERE type = ? ORDER BY id",
                )?;
                let rows = stmt
                    .query_map([t], |r| {
                        let payload: String = r.get(2)?;
                        Ok(json!({
                            "type": r.get::<_, String>(0)?,
                            "status": r.get::<_, String>(1)?,
                            "payload": serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null),
                        }))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({ "jobs": rows }))
            })
            .unwrap()
        };
        for (name, stale) in [
            ("regenerate_all", false),
            ("regenerate_all_keeps_retired_type", true),
        ] {
            let db = fresh_db(&spec, &format!("{name}-d298"));
            if stale {
                plant(&rt, &db, P4D298_STALE_JOBS_PLANT);
            }
            let (resp, lines) =
                global_capture::capture(|| rt.block_on(memories::memory_regenerate_all(&db, &uid)));
            let lines: Vec<String> = lines.iter().map(|l| mask_field(l, "jobId")).collect();
            let mut rec = oracle[name].clone();
            rec["body"]["jobId"] = json!("<minted>");
            if let Some(logs) = rec["logs"].as_array_mut() {
                for l in logs.iter_mut() {
                    if l["fields"].get("jobId").is_some() {
                        l["fields"]["jobId"] = json!("<minted>");
                    }
                }
            }
            let mut resp_masked = response_data(&resp);
            resp_masked["jobId"] = json!("<minted>");
            let mut local = oracle.clone();
            local.insert(name.to_string(), rec);
            let tables = stale.then(|| {
                json!({
                    "interCharacter": jobs_of(&db, "INTER_CHARACTER_MEMORY"),
                    "extraction": jobs_of(&db, "MEMORY_EXTRACTION"),
                })
            });
            check_row_in(
                &local,
                name,
                &Response::Memory(resp_masked),
                &lines,
                tables,
                &mut failed,
            );
        }
    }
    // === end ===

    // The search rows (`search`, `search_min_importance`, `search_include_cold`)
    // were the §S.5 (4) residue at the lane (the base's `memory_service` shim
    // against v4's recall search); the route rides P4.D291's `search_memories`
    // since the `83334b9a5` unification and the rows are strict — once the
    // oracle case reloaded v4's process-singleton index cache per case (see
    // `memories-routes.test.ts`), they agree row for row.
    // === end ===

    assert!(failed.is_empty(), "mismatched cases: {failed:?}");
}
