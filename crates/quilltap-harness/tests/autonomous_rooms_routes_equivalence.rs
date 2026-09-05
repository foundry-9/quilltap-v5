//! P4.6ad AUTONOMOUS-ROOMS route-surface differential: `api::autonomous_rooms::*`
//! vs v4's REAL autonomous-room route handlers. Both sides read a FRESH copy of
//! the committed autonomous-{main,mount}.db fixture per case (baked ids identical
//! → no remap). Reads carry the full body; error arms are compared as HTTP status
//! plus message. Mutation arms blank the minted/computed volatile fields (run
//! ids, job ids, minted timestamps, the computed scheduleNextRunAt and
//! runPausedAccumMs) and prove the write via a structural row/background-jobs dump.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-autonomous-rooms-routes.ndjson npx jest -- autonomous-rooms-routes
//! Run:
//!   QT_ORACLE_AUTONOMOUS_ROUTES=/tmp/oracle-autonomous-rooms-routes.ndjson \
//!     cargo test -p quilltap-harness --test autonomous_rooms_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::autonomous_rooms;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::background_jobs::BackgroundJobsRepository;
use quilltap_core::db::runtime::{Db, DbPaths};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{json, Value};

const ROOM_IDLE: &str = "c1000000-0000-4000-8000-000000000001";
const ROOM_RUNNING: &str = "c1000000-0000-4000-8000-000000000004";
const ROOM_PAUSED: &str = "c1000000-0000-4000-8000-000000000005";
const CHAT_SALON: &str = "c1000000-0000-4000-8000-000000000009";
const ROOM_USER_B: &str = "c1000000-0000-4000-8000-00000000000b";
const MISSING: &str = "00000000-0000-4000-8000-0000000000ff";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    user_id_b: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/autonomous-rooms-web.json")
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
/// Blank the given keys (recursively) to a `<key>` sentinel — the minted /
/// computed fields on the mutation arms (run ids, job ids, minted stamps, the
/// computed scheduleNextRunAt / runPausedAccumMs, the timing-dependent job status).
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
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn norm_blanked(v: &Value, keys: &[&str]) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    blank_keys(&mut v, keys);
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
        // The store-unavailable refusal (P4.23) — v4's deliberate contextful
        // 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}
fn response_data(r: &Response) -> Value {
    let v = serde_json::to_value(r).unwrap();
    v.get("data").cloned().unwrap_or(Value::Null)
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch =
        std::env::temp_dir().join(format!("qt-auto-routes-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("autonomous-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("autonomous-mount.db"), &mount).unwrap();
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

/// Raw dump of the autonomous columns of one chat row (matches the oracle's raw
/// SELECT; numeric columns via f64 → canon_numbers integer-izes whole values).
fn dump_room_row(db: &Db, id: &str) -> Value {
    let id = id.to_string();
    db.read_main(move |conn| {
        let row = conn
            .query_row(
                "SELECT title, isManuallyRenamed, scheduleCron, scheduleNextRunAt, \
                 scheduleFreshnessWindowMs, budgetMaxTurns, budgetMaxTokens, budgetMaxWallClockMs, \
                 budgetEstimatedSpendCapUSD, budgetExcludeCacheHits, runVisibility, \
                 runDestructiveToolsAllowed, runState, runStateMessage, currentRunId, runStartedAt, \
                 runEndedAt, runPausedAt, runPausedAccumMs, runTurnsConsumed, runTokensConsumed \
                 FROM chats WHERE id = ?1",
                [&id],
                |row| {
                    let s = |i: usize| -> rusqlite::Result<Value> {
                        Ok(row
                            .get::<_, Option<String>>(i)?
                            .map(Value::String)
                            .unwrap_or(Value::Null))
                    };
                    let n = |i: usize| -> rusqlite::Result<Value> {
                        Ok(match row.get::<_, Option<f64>>(i)? {
                            Some(f) => serde_json::Number::from_f64(f)
                                .map(Value::Number)
                                .unwrap_or(Value::Null),
                            None => Value::Null,
                        })
                    };
                    Ok(json!({
                        "title": s(0)?,
                        "isManuallyRenamed": n(1)?,
                        "scheduleCron": s(2)?,
                        "scheduleNextRunAt": s(3)?,
                        "scheduleFreshnessWindowMs": n(4)?,
                        "budgetMaxTurns": n(5)?,
                        "budgetMaxTokens": n(6)?,
                        "budgetMaxWallClockMs": n(7)?,
                        "budgetEstimatedSpendCapUSD": n(8)?,
                        "budgetExcludeCacheHits": n(9)?,
                        "runVisibility": s(10)?,
                        "runDestructiveToolsAllowed": n(11)?,
                        "runState": s(12)?,
                        "runStateMessage": s(13)?,
                        "currentRunId": s(14)?,
                        "runStartedAt": s(15)?,
                        "runEndedAt": s(16)?,
                        "runPausedAt": s(17)?,
                        "runPausedAccumMs": n(18)?,
                        "runTurnsConsumed": n(19)?,
                        "runTokensConsumed": n(20)?,
                    }))
                },
            )
            .optional()?;
        Ok(row.unwrap_or(Value::Null))
    })
    .unwrap()
}

/// The AUTONOMOUS_ROOM_TURN jobs (enqueue proof) as `{jobs: [{type,status,payload}]}`.
fn dump_auto_turn_jobs(db: &Db) -> Value {
    db.read_main(|conn| {
        let repo = BackgroundJobsRepository::new(conn);
        let jobs = repo.find_recent_by_type("AUTONOMOUS_ROOM_TURN", 10)?;
        Ok(json!({
            "jobs": jobs.iter().map(|j| json!({
                "type": j.job_type,
                "status": j.status,
                "payload": serde_json::from_str::<Value>(&j.payload).unwrap_or(Value::Null),
            })).collect::<Vec<_>>(),
        }))
    })
    .unwrap()
}

#[test]
fn autonomous_rooms_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_AUTONOMOUS_ROUTES") else {
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let uid_a = spec.user_id.clone();
    let uid_b = spec.user_id_b.clone();

    // The volatile fields blanked on every mutation arm (minted or wall-clock).
    let volatile = [
        "scheduleNextRunAt",
        "currentRunId",
        "runEndedAt",
        "runPausedAt",
        "runPausedAccumMs",
    ];
    // Response-envelope minted ids.
    let ids = ["runId", "jobId"];
    // Enqueue-job volatile fields (minted runId + timing-dependent status).
    let job_blank = ["runId", "status"];

    // Exact 200-body check (baked ids → no blanking).
    let check_body = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        assert_eq!(
            rec["status"].as_u64().unwrap_or(0),
            200,
            "[{name}] expected 200"
        );
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
    // Blanked 200-body check (mutation envelopes with minted ids).
    let check_body_blanked =
        |name: &str, got: &Response, keys: &[&str], failed: &mut Vec<String>| {
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
    // Error-arm check: HTTP status + `{error}` message.
    let check_error = |name: &str, got: &Response, failed: &mut Vec<String>| {
        let rec = &oracle[name];
        let want_status = rec["status"].as_u64().unwrap_or(0) as u16;
        let want_msg = rec["body"]["error"].as_str().unwrap_or("");
        match got {
            Response::Error(e) => {
                if status_of(e.kind) != want_status || e.message != want_msg {
                    eprintln!(
                        "[{name}] MISMATCH: got {} {:?} / want {want_status} {want_msg:?}",
                        status_of(e.kind),
                        e.message
                    );
                    failed.push(name.to_string());
                } else {
                    eprintln!("[{name}] OK ({want_status}).");
                }
            }
            other => {
                eprintln!("[{name}] expected Error, got {:?}", response_data(other));
                failed.push(name.to_string());
            }
        }
    };
    let check_tables_blanked =
        |name: &str, got: &Value, keys: &[&str], failed: &mut Vec<String>| {
            let want = &oracle[name]["tables"];
            if norm_blanked(got, keys) != norm_blanked(want, keys) {
                eprintln!(
                    "[{name} tables] MISMATCH:\n{}",
                    first_diff(&norm_blanked(got, keys), &norm_blanked(want, keys))
                );
                failed.push(format!("{name}_tables"));
            } else {
                eprintln!("[{name} tables] OK.");
            }
        };

    // --- Listing ---
    {
        let db = fresh_db(&spec, "list");
        check_body(
            "listing",
            &autonomous_rooms::system_autonomous_rooms(&db, &uid_a),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "listb");
        check_body(
            "listing_user_b",
            &autonomous_rooms::system_autonomous_rooms(&db, &uid_b),
            &mut failed,
        );
    }
    // --- Status ---
    {
        let db = fresh_db(&spec, "str");
        check_body(
            "status_running",
            &autonomous_rooms::autonomous_room_status(&db, ROOM_RUNNING),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "sti");
        check_body(
            "status_idle",
            &autonomous_rooms::autonomous_room_status(&db, ROOM_IDLE),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "stm");
        check_error(
            "status_missing",
            &autonomous_rooms::autonomous_room_status(&db, MISSING),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "stn");
        check_error(
            "status_non_autonomous",
            &autonomous_rooms::autonomous_room_status(&db, CHAT_SALON),
            &mut failed,
        );
    }
    // --- Start ---
    {
        let db = fresh_db(&spec, "sti2");
        let resp = rt.block_on(autonomous_rooms::autonomous_room_start(
            &db, &uid_a, ROOM_IDLE,
        ));
        check_body_blanked("start_idle", &resp, &ids, &mut failed);
        check_tables_blanked(
            "start_idle",
            &dump_auto_turn_jobs(&db),
            &job_blank,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "star");
        check_error(
            "start_already_running",
            &rt.block_on(autonomous_rooms::autonomous_room_start(
                &db,
                &uid_a,
                ROOM_RUNNING,
            )),
            &mut failed,
        );
    }
    // --- Pause ---
    {
        let db = fresh_db(&spec, "pau");
        let resp = rt.block_on(autonomous_rooms::autonomous_room_pause(&db, ROOM_RUNNING));
        check_body("pause", &resp, &mut failed);
        check_tables_blanked(
            "pause",
            &dump_room_row(&db, ROOM_RUNNING),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "paun");
        check_error(
            "pause_non_autonomous",
            &rt.block_on(autonomous_rooms::autonomous_room_pause(&db, CHAT_SALON)),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "paum");
        check_error(
            "pause_missing",
            &rt.block_on(autonomous_rooms::autonomous_room_pause(&db, MISSING)),
            &mut failed,
        );
    }
    // --- Stop ---
    {
        let db = fresh_db(&spec, "sto");
        let resp = rt.block_on(autonomous_rooms::autonomous_room_stop(&db, ROOM_RUNNING));
        check_body("stop", &resp, &mut failed);
        check_tables_blanked(
            "stop",
            &dump_room_row(&db, ROOM_RUNNING),
            &volatile,
            &mut failed,
        );
    }
    // --- Resume ---
    {
        let db = fresh_db(&spec, "resp");
        let resp = rt.block_on(autonomous_rooms::autonomous_room_resume(
            &db,
            &uid_a,
            ROOM_PAUSED,
        ));
        check_body_blanked("resume_paused", &resp, &ids, &mut failed);
        check_tables_blanked(
            "resume_paused",
            &dump_room_row(&db, ROOM_PAUSED),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "resi");
        let resp = rt.block_on(autonomous_rooms::autonomous_room_resume(
            &db, &uid_a, ROOM_IDLE,
        ));
        check_body_blanked("resume_idle", &resp, &ids, &mut failed);
        check_tables_blanked(
            "resume_idle",
            &dump_auto_turn_jobs(&db),
            &job_blank,
            &mut failed,
        );
    }
    // --- Update settings ---
    let upd = |db: &Db, uid: &str, chat: &str, settings: Value| -> Response {
        rt.block_on(autonomous_rooms::autonomous_room_update_settings(
            db, uid, chat, &settings,
        ))
    };
    {
        let db = fresh_db(&spec, "ucap");
        let resp = upd(
            &db,
            &uid_a,
            ROOM_IDLE,
            json!({
                "budgetMaxTurns": 25, "budgetMaxTokens": 60000, "budgetMaxWallClockMs": 1800000,
                "budgetEstimatedSpendCapUSD": 2.25, "scheduleFreshnessWindowMs": 7200000,
                "runVisibility": "household",
            }),
        );
        check_body("update_caps", &resp, &mut failed);
        check_tables_blanked(
            "update_caps",
            &dump_room_row(&db, ROOM_IDLE),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "uclr");
        let resp = upd(&db, &uid_a, ROOM_RUNNING, json!({ "budgetMaxTurns": null }));
        check_body("update_clear_cap", &resp, &mut failed);
        check_tables_blanked(
            "update_clear_cap",
            &dump_room_row(&db, ROOM_RUNNING),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "ucrs");
        let resp = upd(
            &db,
            &uid_a,
            ROOM_IDLE,
            json!({ "scheduleCron": "0 9 * * *" }),
        );
        check_body("update_cron_set", &resp, &mut failed);
        check_tables_blanked(
            "update_cron_set",
            &dump_room_row(&db, ROOM_IDLE),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "ucrc");
        let resp = upd(&db, &uid_a, ROOM_RUNNING, json!({ "scheduleCron": null }));
        check_body("update_cron_clear", &resp, &mut failed);
        check_tables_blanked(
            "update_cron_clear",
            &dump_room_row(&db, ROOM_RUNNING),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "utit");
        let resp = upd(
            &db,
            &uid_a,
            ROOM_IDLE,
            json!({ "title": "  The Renamed Room  " }),
        );
        check_body("update_title", &resp, &mut failed);
        check_tables_blanked(
            "update_title",
            &dump_room_row(&db, ROOM_IDLE),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "uinv");
        check_error(
            "update_invalid_cron",
            &upd(
                &db,
                &uid_a,
                ROOM_IDLE,
                json!({ "scheduleCron": "not a cron" }),
            ),
            &mut failed,
        );
    }
    // P4.55 (the merge-verb silent-keep sweep): v4 runs
    // `updateSettingsSchema.parse` BEFORE the service call, so a
    // present-but-invalid field is a 400 `Validation error`. v5 used to DROP it
    // silently and answer 200 — `update_invalid_cron` above is the SERVICE-layer
    // guard and was blind to this whole class.
    for (name, patch) in [
        (
            "update_invalid_cap_type",
            json!({ "budgetMaxTurns": "many" }),
        ),
        (
            "update_invalid_cap_negative",
            json!({ "budgetMaxTurns": -5 }),
        ),
        (
            "update_invalid_cap_fractional",
            json!({ "budgetMaxTurns": 2.5 }),
        ),
        (
            "update_invalid_visibility",
            json!({ "runVisibility": "bogus" }),
        ),
        (
            "update_invalid_title_too_long",
            json!({ "title": "x".repeat(400) }),
        ),
    ] {
        let db = fresh_db(&spec, name);
        check_error(name, &upd(&db, &uid_a, ROOM_IDLE, patch), &mut failed);
    }
    // Zod ≥ 4.5.4 (v4 `6e1a64ea6`) measures the `.max(300)` in CODE POINTS once
    // the UTF-16 count overflows it: 151 astral characters are 302 units but 151
    // code points, so this title is now ACCEPTED (`jsstr::zod_len_max_ok`). It
    // was `update_invalid_title_astral`, a 400 row pinning Zod 4.4.3's UTF-16
    // rule; the `d883a5ee1` unification's neutrality sweep moved it.
    {
        let db = fresh_db(&spec, "utaw");
        let resp = upd(&db, &uid_a, ROOM_IDLE, json!({ "title": "😀".repeat(151) }));
        check_body("update_title_astral_within_max", &resp, &mut failed);
        check_tables_blanked(
            "update_title_astral_within_max",
            &dump_room_row(&db, ROOM_IDLE),
            &volatile,
            &mut failed,
        );
    }
    {
        // The refusal writes NOTHING. ROOM_RUNNING already carries stored caps,
        // so the row dump is a real keep-current measurement — the valid
        // `budgetMaxTurns` riding alongside the invalid `runVisibility` must
        // not land either.
        let db = fresh_db(&spec, "uinvw");
        let name = "update_invalid_writes_nothing";
        check_error(
            name,
            &upd(
                &db,
                &uid_a,
                ROOM_RUNNING,
                json!({ "budgetMaxTurns": 99, "runVisibility": "bogus" }),
            ),
            &mut failed,
        );
        check_tables_blanked(
            name,
            &dump_room_row(&db, ROOM_RUNNING),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "unon");
        check_error(
            "update_non_autonomous",
            &upd(&db, &uid_a, CHAT_SALON, json!({ "budgetMaxTurns": 5 })),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "umis");
        check_error(
            "update_missing",
            &upd(&db, &uid_a, MISSING, json!({ "budgetMaxTurns": 5 })),
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "uclf");
        let resp = upd(
            &db,
            &uid_a,
            ROOM_IDLE,
            json!({ "runDestructiveToolsAllowed": true }),
        );
        check_body("update_clamp_false", &resp, &mut failed);
        check_tables_blanked(
            "update_clamp_false",
            &dump_room_row(&db, ROOM_IDLE),
            &volatile,
            &mut failed,
        );
    }
    {
        let db = fresh_db(&spec, "uclt");
        let resp = upd(
            &db,
            &uid_b,
            ROOM_USER_B,
            json!({ "runDestructiveToolsAllowed": true }),
        );
        check_body("update_clamp_true", &resp, &mut failed);
        check_tables_blanked(
            "update_clamp_true",
            &dump_room_row(&db, ROOM_USER_B),
            &volatile,
            &mut failed,
        );
    }

    assert!(failed.is_empty(), "mismatched cases: {failed:?}");
}
