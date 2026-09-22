//! P4.D212 REBUILD-SUMMARY differential (tier 2):
//! `services::chat_admin::chat_rebuild_summary` vs v4's REAL
//! `handleRebuildSummary` (`e7821606f`), driven through v4's real chat route
//! (`POST /api/v1/chats/[id]?action=rebuild-summary`) by
//! `harness/oracle/cases/chat-rebuild-summary.test.ts`.
//!
//! Both sides open a FRESH per-case COPY of the committed
//! `chat-admin-{main,mount}.db` pair (never the pair itself — §R.12), apply the
//! SAME per-copy widen and the SAME planted SQL (both emitted verbatim in the
//! oracle row, so the setup has one source), call the action, and diff:
//!
//! - status + body (`jobId` blanked — both sides mint a UUID);
//! - `distinctJobIds` — the in-band identity witness for the repeat call (no
//!   dedupe → two distinct ids);
//! - the hydrated `chats` rows for the chat under test and the help chat
//!   (`contextSummary`, `summaryAnchorMessageIds`, `lastSummaryTurn`, and a
//!   SEEDED NON-ZERO `lastFullRebuildTurn` the action must leave where it is).
//!   `updatedAt` is minted: v4's must be at-or-after the frozen instant and
//!   v5's the injected `now_iso` — asserted, then blanked. An unwritten row's
//!   `updatedAt` is compared raw (the seed on both sides);
//! - the RAW `background_jobs` rows — `payload` compared as the column's
//!   BYTES, so `{"chatId","connectionProfileId","forceRegenerate":false}` key
//!   ORDER is part of the comparand — plus `type`/`status`/`priority`/
//!   `attempts`/`maxAttempts`/`lastError`/`startedAt`/`completedAt`/`userId`;
//! - the distinct realtime `topic[:id]` set each side published (v4 wraps its
//!   real `publishRealtime`; v5 arms a thread-scoped bus and lets the
//!   coalescer's flush fire on the SAME current-thread runtime before reading —
//!   the earlier timer deadline always fires first, so there is no race).
//!
//! ⚠ The per-copy WIDEN: the committed `chat-admin-*` pair predates v4's
//! `cycleOrderParticipantIds` column, and v4's `chats.update` writes every
//! schema key — so on the unwidened pair v4 answers 500 on every chat write.
//! The oracle widens each COPY through v4's own migration statement, guarded on
//! `pragma_table_info`; this side applies the identical guard.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-chat-rebuild-summary.ndjson npx jest -- chat-rebuild-summary
//! Run:
//!   QT_ORACLE_CHAT_REBUILD_SUMMARY=/tmp/oracle-chat-rebuild-summary.ndjson \
//!     cargo test -p quilltap-harness --test chat_rebuild_summary_equivalence -- --nocapture

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::api::types::{ErrorKind, EventPayload, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::realtime::bus::{
    arm_realtime_bus_for_current_thread, disarm_realtime_bus_for_current_thread, BusSpawner,
    COALESCE_WINDOW_MS,
};
use quilltap_core::services::chat_admin;
use serde::Deserialize;
use serde_json::{json, Value};

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const HELP_CHAT: &str = "c1000000-0000-4000-8000-000000000004";
/// The instant the oracle's TICKING clock starts from (`chat-admin-web.json`).
const FROZEN_NOW_ISO: &str = "2026-05-05T00:00:00.000Z";
/// The fixture's seed timestamp — what an untouched `updatedAt` still reads.
const SEED_ISO: &str = "2026-05-01T00:00:00.000Z";

/// Every case the oracle is expected to emit — a guard against a silently
/// dropped row on EITHER side (the oracle-set vs this set are asserted equal).
const CASES: &[&str] = &[
    "rebuild_happy_path",
    "rebuild_chat_missing",
    "rebuild_running_room_409",
    "rebuild_paused_room_200",
    "rebuild_autonomous_no_run_state_200",
    "rebuild_salon_running_state_200",
    "rebuild_no_profiles_400",
    "rebuild_running_room_no_profiles_409",
    "rebuild_null_cheap_llm_settings_200",
    "rebuild_no_chat_settings_row_200",
    "rebuild_cast_profile_named",
    "rebuild_cast_profile_dangling",
    "rebuild_only_first_seat_consulted",
    "rebuild_fallback_first_profile_by_read_order",
    "rebuild_help_chat",
    "rebuild_twice_no_dedupe",
    "rebuild_update_fails_500",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-admin-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// A fresh per-case COPY of the committed pair, opened.
fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-rs-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-admin-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-admin-mount.db"), &mount).unwrap();
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

/// The oracle's widen (guarded like v4's `addColumnIfMissing`) then its plants,
/// in order, through the writer's own connection.
fn widen_and_plant(rt: &tokio::runtime::Runtime, db: &Db, widen: &[Value], plant: &[Value]) {
    let widen: Vec<(String, String, String)> = widen
        .iter()
        .map(|w| {
            let a = w.as_array().expect("widen triple");
            (
                a[0].as_str().unwrap().to_string(),
                a[1].as_str().unwrap().to_string(),
                a[2].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let plant: Vec<String> = plant
        .iter()
        .map(|s| s.as_str().expect("plant sql").to_string())
        .collect();
    rt.block_on(db.write(move |w| {
        let c = w.main().connection();
        for (table, column, decl) in &widen {
            let have: i64 = c.query_row(
                &format!(
                    "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'"
                ),
                [],
                |r| r.get(0),
            )?;
            if have == 0 {
                c.execute_batch(&format!(
                    "ALTER TABLE \"{table}\" ADD COLUMN \"{column}\" {decl}"
                ))?;
            }
        }
        for sql in &plant {
            c.execute_batch(sql)?;
        }
        Ok(())
    }))
    .expect("widen + plant");
}

/// The web-edge (status, body) for a Response (the HTTP transport mapping).
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatAdmin(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

fn dump_chat(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    db.read_main(move |c| quilltap_core::db::chats_read::find_by_id(c, &cid))
        .unwrap()
        .unwrap_or(Value::Null)
}

/// The RAW `background_jobs` rows in insertion order — `payload` as the column's
/// TEXT, so its key order is compared byte for byte.
fn dump_jobs(db: &Db) -> Value {
    let rows = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT \"userId\", \"type\", \"status\", \"payload\", \"priority\", \"attempts\", \
                 \"maxAttempts\", \"lastError\", \"startedAt\", \"completedAt\" \
                 FROM \"background_jobs\" ORDER BY rowid",
            )?;
            let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let rows = stmt.query_map([], |r| {
                let mut o = serde_json::Map::new();
                for (i, n) in names.iter().enumerate() {
                    let v = match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(x) => json!(x),
                        rusqlite::types::ValueRef::Real(x) => json!(x),
                        rusqlite::types::ValueRef::Text(t) => {
                            Value::String(String::from_utf8_lossy(t).into_owned())
                        }
                        rusqlite::types::ValueRef::Blob(_) => Value::String("<blob>".into()),
                    };
                    o.insert(n.clone(), v);
                }
                Ok(Value::Object(o))
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .unwrap();
    Value::Array(rows)
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

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
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    if let Some(o) = v.as_object_mut() {
        if o.contains_key("jobId") {
            o.insert("jobId".into(), Value::String("<jobId>".into()));
        }
    }
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

/// The minted `updatedAt` on one chat row: when v4 wrote it (it no longer reads
/// the seed), v4's must be at-or-after the frozen instant and v5's exactly the
/// injected `now_iso`; both are then blanked. When v4 did NOT write it, it is
/// left raw so an unwanted v5 write shows as a diff.
fn settle_updated_at(name: &str, which: &str, got: &mut Value, want: &mut Value) -> Vec<String> {
    let mut failed = Vec::new();
    let w = want
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if w.is_empty() || w == SEED_ISO {
        return failed;
    }
    let g = got
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if w.as_str() < FROZEN_NOW_ISO {
        eprintln!("[{name} {which}] v4 updatedAt {w:?} precedes the frozen instant");
        failed.push(format!("{name}_{which}_v4_updatedAt"));
    }
    if g != FROZEN_NOW_ISO {
        eprintln!("[{name} {which}] v5 updatedAt {g:?} != the injected now {FROZEN_NOW_ISO:?}");
        failed.push(format!("{name}_{which}_v5_updatedAt"));
    }
    for v in [got, want] {
        if let Some(o) = v.as_object_mut() {
            o.insert("updatedAt".into(), Value::String("<minted>".into()));
        }
    }
    failed
}

#[test]
fn chat_rebuild_summary_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHAT_REBUILD_SUMMARY") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_REBUILD_SUMMARY (see test header).");
            return;
        }
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let rows: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(
        !rows.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );
    let oracle_names: BTreeSet<String> = rows
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_string())
        .collect();
    let expected: BTreeSet<String> = CASES.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        oracle_names, expected,
        "case-set drift between the oracle and this file's CASES"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    for want in &rows {
        let name = want["name"].as_str().unwrap();
        let chat_id = want["chatId"].as_str().unwrap();
        let calls = want["calls"].as_u64().unwrap_or(1);
        let db = fresh_db(&spec, name);
        widen_and_plant(
            &rt,
            &db,
            want["widen"].as_array().expect("widen"),
            want["plant"].as_array().expect("plant"),
        );

        // A thread-scoped bus whose coalescer flush runs on THIS runtime.
        let (tx, mut rx) = tokio::sync::broadcast::channel(64);
        let handle = rt.handle().clone();
        let spawner: BusSpawner = Arc::new(move |fut| {
            handle.spawn(fut);
        });
        arm_realtime_bus_for_current_thread(tx, spawner, || 0);

        let mut bodies: Vec<(u16, Value)> = Vec::new();
        for _ in 0..calls {
            let r = rt.block_on(chat_admin::chat_rebuild_summary(
                &db,
                &spec.user_id,
                chat_id,
                FROZEN_NOW_ISO,
            ));
            bodies.push(status_body(&r));
        }
        // Let every flush (deadline +COALESCE_WINDOW_MS) fire before reading:
        // same current-thread runtime, so the earlier deadline fires first.
        rt.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS * 2)).await
        });
        disarm_realtime_bus_for_current_thread();
        let mut published: BTreeSet<String> = BTreeSet::new();
        while let Ok(ev) = rx.try_recv() {
            if let EventPayload::Realtime(h) = ev.payload {
                published.insert(match h.id {
                    Some(id) => format!("{}:{id}", h.topic),
                    None => h.topic.clone(),
                });
            }
        }

        let (status, body) = bodies.last().cloned().unwrap();
        let distinct: BTreeSet<String> = bodies
            .iter()
            .filter_map(|(_, b)| b.get("jobId").and_then(Value::as_str).map(str::to_string))
            .collect();

        // status
        let want_status = want["status"].as_u64().unwrap() as u16;
        if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }
        // body
        if norm(&body) != norm(&want["body"]) {
            eprintln!(
                "[{name}] BODY MISMATCH:\n{}",
                first_diff(&norm(&body), &norm(&want["body"]))
            );
            failed.push(format!("{name}_body"));
        }
        // in-band job identity
        let want_distinct = want["distinctJobIds"].as_u64().unwrap() as usize;
        if distinct.len() != want_distinct {
            eprintln!(
                "[{name}] distinct job ids {} != {want_distinct}",
                distinct.len()
            );
            failed.push(format!("{name}_distinctJobIds"));
        }
        // chats rows
        for (which, id) in [("chat", CHAT), ("helpChat", HELP_CHAT)] {
            let mut got = dump_chat(&db, id);
            let mut w = want["tables"][which].clone();
            failed.extend(settle_updated_at(name, which, &mut got, &mut w));
            if norm(&got) != norm(&w) {
                eprintln!(
                    "[{name} {which}] MISMATCH:\n{}",
                    first_diff(&norm(&got), &norm(&w))
                );
                failed.push(format!("{name}_{which}"));
            }
        }
        // raw job rows (payload BYTES)
        let got_jobs = dump_jobs(&db);
        let want_jobs = &want["tables"]["jobs"];
        if norm(&got_jobs) != norm(want_jobs) {
            eprintln!(
                "[{name} jobs] MISMATCH:\n{}",
                first_diff(&norm(&got_jobs), &norm(want_jobs))
            );
            failed.push(format!("{name}_jobs"));
        }
        // realtime topics
        let want_pub: BTreeSet<String> = want["published"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        if published != want_pub {
            eprintln!("[{name}] PUBLISHED {published:?} != {want_pub:?}");
            failed.push(format!("{name}_published"));
        }
        if !failed.iter().any(|f| f.starts_with(name)) {
            eprintln!(
                "[{name}] OK ({status}, {} job row(s)).",
                got_jobs.as_array().unwrap().len()
            );
        }
    }

    assert!(failed.is_empty(), "rebuild-summary mismatches: {failed:?}");
}
