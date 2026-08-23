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

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-mem-routes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("memories-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("memories-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
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

/// P4.55: seed `chat_settings.autoHousekeepingSettings` through the REAL writer
/// — the Rust twin of the oracle case's `repos.chatSettings.updateForUser` call.
async fn seed_housekeeping(db: &Db, user_id: &str) {
    let bag = json!({
        "enabled": true,
        "perCharacterCap": 1234,
        "perCharacterCapOverrides": {},
        "autoMergeSimilarThreshold": 0.5,
        "mergeSimilar": true,
    })
    .to_string();
    let uid = user_id.to_string();
    let now = quilltap_core::clock::now_iso();
    db.write(move |w| {
        quilltap_core::db::chat_settings::update_for_user(
            w.main().connection(),
            &uid,
            &[(
                "autoHousekeepingSettings",
                quilltap_core::db::chat_settings::SettingsColVal::Text(bag),
            )],
            &now,
        )
    })
    .await
    .expect("seed housekeeping settings");
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

#[test]
fn memories_routes_match_oracle() {
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
            &memories::memory_list(&db, MNEMO, None, None, None, None, None, Some(20), Some(0)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lp2");
        check_body(
            "list_paginated_p2",
            &memories::memory_list(&db, MNEMO, None, None, None, None, None, Some(20), Some(20)),
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
            &memories::memory_list(&db, ORLA, None, None, None, None, None, None, None),
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
            ),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "lmc");
        check_error(
            "list_missing_character",
            &memories::memory_list(&db, MISSING, None, None, None, None, None, None, None),
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

    // Create/update blank the minted/regenerated fields (id + timestamps +
    // embedding — the vector fidelity is proven by the search + W4.7e2 seams).
    let create_blank = [
        "id",
        "createdAt",
        "updatedAt",
        "lastReinforcedAt",
        "embedding",
    ];
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

    // --- Create (gate) ---
    {
        let db = fresh_db(&spec, "ci");
        let p = provider(&db);
        let resp = rt.block_on(memories::memory_create(
            &db,
            &p,
            &uid,
            json!({
                "characterId": MNEMO,
                "content": "The clocktower chimes thirteen times on the winter solstice.",
                "summary": "The clocktower chimes thirteen at solstice.",
                "keywords": ["clocktower", "solstice"],
                "importance": 0.55,
                "source": "MANUAL",
            }),
        ));
        check_blanked("create_insert", &resp, &create_blank, &mut failed);
    }
    {
        let db = fresh_db(&spec, "cnd");
        let p = provider(&db);
        let resp = rt.block_on(memories::memory_create(
            &db,
            &p,
            &uid,
            json!({
                "characterId": MNEMO,
                "content": "Mnemo always double-checks the barometer before hoisting the mainsail.",
                "summary": "Mnemo checks the barometer before the mainsail.",
                "source": "AUTO",
            }),
        ));
        check_blanked("create_near_duplicate", &resp, &create_blank, &mut failed);
        check_tables(
            "create_near_duplicate",
            &dump_rows(&db, &["b2000000-0000-4000-8000-0000000000d0"]),
            &mut failed,
        );
    }
    // --- Update (no re-embed) ---
    {
        let db = fresh_db(&spec, "up");
        let resp = rt.block_on(memories::memory_update(
            &db,
            MEM_TAGGED_1,
            json!({ "importance": 0.95, "summary": "Alden, the stern mentor." }),
        ));
        check_blanked("update", &resp, &["updatedAt"], &mut failed);
    }
    // --- Delete (+ unlink scrub) ---
    {
        let db = fresh_db(&spec, "del");
        let resp = rt.block_on(memories::memory_delete(&db, MEM_REL_B));
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

    // --- Housekeeping (deletes-nothing config → clock-independent) ---
    {
        let db = fresh_db(&spec, "hkp");
        let resp = rt.block_on(memories::memory_housekeep_preview(
            &db,
            MNEMO,
            Some(10000),
            None,
            Some(0.0),
            None,
        ));
        check_body("housekeep_preview", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "hkd");
        let resp = rt.block_on(memories::memory_housekeep(
            &db,
            json!({ "characterId": MNEMO, "maxMemories": 10000, "minImportance": 0, "dryRun": true }),
        ));
        check_body("housekeep_dryrun", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "hkr");
        let resp = rt.block_on(memories::memory_housekeep(
            &db,
            json!({ "characterId": MNEMO, "maxMemories": 10000, "minImportance": 0, "dryRun": false }),
        ));
        check_body("housekeep_run", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "hks");
        let resp = rt.block_on(memories::memory_housekeep_sweep(&db, &uid));
        check_blanked("housekeep_sweep", &resp, &["jobId"], &mut failed);
    }
    // --- Configs ---
    {
        let db = fresh_db(&spec, "hcg");
        check_body(
            "housekeeping_config_get",
            &memories::memory_housekeeping_config_get(&db, &uid),
            &mut failed,
        );
    }
    {
        // P4.52 retired this row's fixture-vintage ruling: v4's whole-row
        // `chatSettings.updateForUser` now survives the widened fixture and
        // answers 200 with the settings bag, so both sides compare plainly.
        let db = fresh_db(&spec, "hcs");
        let resp = rt.block_on(memories::memory_housekeeping_config_set(
            &db,
            &uid,
            json!({ "enabled": true, "perCharacterCap": 1500 }),
        ));
        check_body("housekeeping_config_set", &resp, &mut failed);
    }
    // P4.55 (the merge-verb silent-keep sweep): `housekeepingConfigSchema`
    // safeParses BEFORE the read/merge/write — a present-but-invalid value 400s
    // and stores NOTHING. v5 used to merge and PERSIST the garbage.
    {
        let db = fresh_db(&spec, "hcib");
        let resp = rt.block_on(memories::memory_housekeeping_config_set(
            &db,
            &uid,
            json!({ "enabled": "yes" }),
        ));
        check_error("housekeeping_config_set_invalid_bool", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "hcir");
        let resp = rt.block_on(memories::memory_housekeeping_config_set(
            &db,
            &uid,
            json!({ "perCharacterCap": -5 }),
        ));
        check_error("housekeeping_config_set_out_of_range", &resp, &mut failed);
    }
    {
        let db = fresh_db(&spec, "hciw");
        rt.block_on(seed_housekeeping(&db, &uid));
        let resp = rt.block_on(memories::memory_housekeeping_config_set(
            &db,
            &uid,
            json!({ "perCharacterCap": "lots" }),
        ));
        let name = "housekeeping_config_invalid_writes_nothing";
        check_error(name, &resp, &mut failed);
        let uid2 = uid.clone();
        let stored = db
            .read_main(move |main| {
                quilltap_core::db::chat_settings::find_auto_housekeeping_settings_by_user_id(
                    main, &uid2,
                )
            })
            .expect("read housekeeping settings after refused set")
            .unwrap_or(Value::Null);
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
        // P4.56 — the float-literal nit, measured with a float-SENSITIVE
        // comparand. Zod's `.int()` is `Number.isInteger`, so `200.0` PASSES;
        // JS has no float/int distinction, so v4 stores and echoes `200`.
        // v5's serde_json keeps the float. The family's `canon_numbers`
        // normalizer collapses exactly that difference, so this arm goes
        // through `norm_float_exact` on BOTH the echo and the stored bag.
        let db = fresh_db(&spec, "hcfl");
        let name = "housekeeping_config_set_int_float_literal";
        let resp = rt.block_on(memories::memory_housekeeping_config_set(
            &db,
            &uid,
            json!({ "perCharacterCap": 200.0, "autoMergeSimilarThreshold": 1.0 }),
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
        let uid2 = uid.clone();
        let stored = db
            .read_main(move |main| {
                quilltap_core::db::chat_settings::find_auto_housekeeping_settings_by_user_id(
                    main, &uid2,
                )
            })
            .expect("read housekeeping settings after the float-literal set")
            .unwrap_or(Value::Null);
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
    {
        let db = fresh_db(&spec, "ecg");
        check_body(
            "extraction_concurrency_get",
            &memories::memory_extraction_concurrency_get(&db),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "ecs");
        let resp = rt.block_on(memories::memory_extraction_concurrency_set(
            &db,
            json!({ "concurrency": 8 }),
        ));
        check_body("extraction_concurrency_set", &resp, &mut failed);
    }

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

    assert!(failed.is_empty(), "mismatched cases: {failed:?}");
}
