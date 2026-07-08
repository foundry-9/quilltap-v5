//! Tier-2 differential test — the enclave lifecycle service (U4.3; v4
//! `lib/services/chat-message/autonomous-room.service.ts` +
//! `lib/background-jobs/handlers/autonomous-run-start.ts` /
//! `autonomous-room-announce.ts`), ported as
//! `quilltap_core::enclave::{announce, lifecycle}`.
//!
//! Both sides run the SAME 38-op sequence (`enclave-lifecycle-tier2.json`) on a
//! fresh copy of the two-database seed fixture, then diff (a) each op's result
//! object and (b) the final `chats` + `background_jobs` + `chat_messages` dumps.
//!
//! ## Injection
//!
//! - **Clock**: both sides run a stepped clock re-anchored per op at
//!   `baseMs + opIndex*stepPerOpMs`, +1 ms per read. Each op's FIRST clock read
//!   is exactly the anchor, so first-read-derived values are PINNED and diffed
//!   byte-for-byte: `runStartedAt` / `scheduleLastRunAt` (manual start),
//!   `runPausedAt` (pause), `runEndedAt` (stop), the resume accumulation
//!   arithmetic (`runPausedAccumMs = 500 + (base9 − base8) = 3600500` — an
//!   exact-diffed NUMBER), the settings-editor cron anchor, and the
//!   reconcile-startup `nowIso` fallback branch.
//! - **UUIDs**: v4's `randomUUID` is an ESM destructured import a tsx oracle
//!   cannot monkey-patch, so minted ids are handled by the shared first-seen
//!   remap below (the folders-remap form); the `payload.runId ↔
//!   chats.currentRunId` relationship verifies through the shared map.
//! - **Cron** (the U4.2 seam, CLOSED at integration): the oracle runs REAL
//!   croner under `TZ=UTC`; the Rust side injects the real
//!   `enclave::cron::try_next_occurrence` with `tz = "UTC"` — so this
//!   differential also proves the lifecycle∘cron COMPOSITION (the cron
//!   engine's own tier-1 corpus is `enclave_cron_equivalence`). The triples
//!   the corpus exercises, pre-verified against croner mechanically during
//!   the parallel phase (`node -e` from the v4 checkout under the identical
//!   Date mock) and now answered by the real engine:
//!
//!   | expr            | anchor ms       | anchor ISO                | result                    |
//!   |-----------------|-----------------|---------------------------|---------------------------|
//!   | `0 9 * * *`     | `1924999200000` | 2031-01-01T02:00:00.000Z  | `2031-01-01T09:00:00.000Z` (`1925024400000`) |
//!   | `30 6 * * 1`    | `1925028000000` | 2031-01-01T10:00:00.000Z  | `2031-01-06T06:30:00.000Z` (`1925447400000`) |
//!   | `0 0 30 2 *`    | `1925031600000` | 2031-01-01T11:00:00.000Z  | `null` (Feb 30 never)      |
//!   | `not a cron`    | any             | —                         | throws (invalid)           |
//!   | `totally bogus` | any             | —                         | throws (invalid)           |
//!
//! ## Normalization (applied identically to both sides)
//!
//! - rows re-sorted by stable content keys (jobs by payload `chatId`, messages
//!   by `chatId` — every minted id is order-unstable pre-remap);
//! - job/message/run UUIDs not in the spec's pinned set → first-seen `<uuid-N>`
//!   tokens (substring remap, so ids inside payload JSON and banner prose
//!   remap consistently with the id columns);
//! - ISO timestamps → kept when 2020-era (seed sentinels) or in the pinned set
//!   (the per-op anchors + the seeded/advanced cron slots), else `<ts>`
//!   (later-than-first-read mints: banner `createdAt`, the addMessage metadata
//!   `lastMessageAt`/`updatedAt` bumps, job `scheduledAt`/`createdAt`/`updatedAt`).
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-enclave-lifecycle-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-enclave-lifecycle-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-enclave-lifecycle-fixture.ts
//!   TZ=UTC \
//!   QT_FIXTURE_ENCLAVE=/tmp/qt-enclave-lifecycle-main.db \
//!   QT_FIXTURE_ENCLAVE_MOUNT=/tmp/qt-enclave-lifecycle-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/enclave-lifecycle-tier2.ts > /tmp/oracle-enclave-lifecycle.ndjson
//! Run:
//!   QT_ORACLE_ENCLAVE=/tmp/oracle-enclave-lifecycle.ndjson \
//!   QT_FIXTURE_ENCLAVE=/tmp/qt-enclave-lifecycle-main.db \
//!   QT_FIXTURE_ENCLAVE_MOUNT=/tmp/qt-enclave-lifecycle-mount.db \
//!     cargo test -p quilltap-harness --test enclave_lifecycle_tier2_equivalence

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::enclave::lifecycle::{
    begin_autonomous_run, pause_autonomous_room, reconcile_autonomous_runs_at_startup,
    reconcile_failed_autonomous_turn, resume_autonomous_room, start_autonomous_room_manually,
    stop_autonomous_room, update_autonomous_room_settings, AutonomousRoomSettingsPatch,
    BeginAutonomousRunInput, BeginAutonomousRunResult, LifecycleDeps, OnEnqueueFailure,
    StartManualRunResult, UpdateAutonomousRoomSettingsResult,
};
use regex::Regex;
use serde_json::{json, Value};

/// The REAL cron engine under the oracle's `TZ=UTC` (the U4.2 seam, closed):
/// the header table's pre-verified triples now come out of
/// `enclave::cron::try_next_occurrence` itself, proving the lifecycle∘cron
/// composition through this differential.
fn real_next_occurrence(expr: &str, anchor_ms: i64) -> Result<Option<i64>, String> {
    quilltap_core::enclave::cron::try_next_occurrence(expr, anchor_ms, "UTC")
}

fn tri_string(patch: &Value, key: &str) -> Option<Option<String>> {
    match patch.get(key) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(v.as_str().map(String::from)),
    }
}

fn tri_f64(patch: &Value, key: &str) -> Option<Option<f64>> {
    match patch.get(key) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(v.as_f64()),
    }
}

fn parse_patch(patch: &Value) -> AutonomousRoomSettingsPatch {
    AutonomousRoomSettingsPatch {
        title: patch.get("title").and_then(Value::as_str).map(String::from),
        schedule_cron: tri_string(patch, "scheduleCron"),
        schedule_freshness_window_ms: tri_f64(patch, "scheduleFreshnessWindowMs"),
        budget_max_turns: tri_f64(patch, "budgetMaxTurns"),
        budget_max_tokens: tri_f64(patch, "budgetMaxTokens"),
        budget_max_wall_clock_ms: tri_f64(patch, "budgetMaxWallClockMs"),
        budget_estimated_spend_cap_usd: tri_f64(patch, "budgetEstimatedSpendCapUSD"),
        run_visibility: tri_string(patch, "runVisibility"),
        run_destructive_tools_allowed: patch
            .get("runDestructiveToolsAllowed")
            .and_then(Value::as_bool),
        budget_exclude_cache_hits: patch.get("budgetExcludeCacheHits").and_then(Value::as_bool),
    }
}

// ============================================================================
// Normalization
// ============================================================================

struct Normalizer {
    pinned_ids: HashSet<String>,
    pinned_times: HashSet<String>,
    uuid_re: Regex,
    ts_re: Regex,
    map: HashMap<String, String>,
    next_token: usize,
}

impl Normalizer {
    fn new(pinned_ids: HashSet<String>, pinned_times: HashSet<String>) -> Self {
        Normalizer {
            pinned_ids,
            pinned_times,
            uuid_re: Regex::new(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            )
            .unwrap(),
            ts_re: Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap(),
            map: HashMap::new(),
            next_token: 0,
        }
    }

    fn norm_string(&mut self, s: &str) -> String {
        // Timestamps first (patterns are disjoint from UUIDs): keep 2020-era
        // seed sentinels + the pinned set; placeholder every other mint.
        let ts_re = self.ts_re.clone();
        let with_ts = ts_re.replace_all(s, |caps: &regex::Captures| {
            let t = caps.get(0).unwrap().as_str();
            if t.starts_with("2020-") || self.pinned_times.contains(t) {
                t.to_string()
            } else {
                "<ts>".to_string()
            }
        });
        // UUIDs: pinned kept verbatim; minted ids get first-seen tokens through
        // the shared map (relationships verify by token equality).
        let uuid_re = self.uuid_re.clone();
        let out = uuid_re.replace_all(&with_ts, |caps: &regex::Captures| {
            let u = caps.get(0).unwrap().as_str().to_lowercase();
            if self.pinned_ids.contains(&u) {
                u
            } else if let Some(tok) = self.map.get(&u) {
                tok.clone()
            } else {
                let tok = format!("<uuid-{}>", self.next_token);
                self.next_token += 1;
                self.map.insert(u, tok.clone());
                tok
            }
        });
        out.into_owned()
    }

    /// Walk a JSON value, normalizing every string leaf in deterministic order
    /// (object entries in map order — both sides parse/build through the same
    /// serde_json, so iteration order is identical).
    fn norm_value(&mut self, v: &mut Value) {
        match v {
            Value::String(s) => *s = self.norm_string(s),
            Value::Array(arr) => {
                for item in arr {
                    self.norm_value(item);
                }
            }
            Value::Object(map) => {
                for (_k, item) in map.iter_mut() {
                    self.norm_value(item);
                }
            }
            _ => {}
        }
    }
}

/// Extract `payload.chatId` from a background_jobs row (for the stable sort).
fn job_sort_key(row: &Value) -> (String, String) {
    let payload = row
        .get("payload")
        .and_then(Value::as_str)
        .and_then(|p| serde_json::from_str::<Value>(p).ok());
    let chat_id = payload
        .as_ref()
        .and_then(|p| p.get("chatId"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let job_type = row
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    (job_type, chat_id)
}

/// Re-sort a dump's rows by a stable content key (minted ids make the oracle's
/// `orderBy: 'id'` unstable across sides), IN PLACE.
fn sort_rows(dump: &mut Value, key: impl Fn(&Value) -> (String, String)) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| key(r));
    }
}

#[tokio::test]
async fn enclave_lifecycle_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_ENCLAVE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ENCLAVE to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_ENCLAVE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_ENCLAVE to the main seed fixture .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_ENCLAVE_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_ENCLAVE_MOUNT to the mount seed fixture .db.");
            return;
        }
    };

    let spec_text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../harness/oracle/fixtures/enclave-lifecycle-tier2.json"),
    )
    .unwrap_or_else(|e| panic!("read spec: {e}"));
    let spec: Value = serde_json::from_str(&spec_text).expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-enclave-rust-main-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-enclave-rust-mount-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture, &work_main).unwrap_or_else(|e| panic!("copy main fixture: {e}"));
    std::fs::copy(&fixture_mount, &work_mount)
        .unwrap_or_else(|e| panic!("copy mount fixture: {e}"));

    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        pepper,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    // The stepped clock (re-anchored per op) + real-uuid mint + the REAL cron.
    let clock = AtomicI64::new(0);
    let now_ms = || clock.fetch_add(1, Ordering::SeqCst);
    let mint_uuid = quilltap_core::enclave::announce::system_mint_uuid;
    let next_occurrence = real_next_occurrence;
    let deps = LifecycleDeps {
        now_ms: &now_ms,
        mint_uuid: &mint_uuid,
        next_occurrence: &next_occurrence,
    };

    let base_ms = spec["baseMs"].as_i64().unwrap();
    let step_ms = spec["stepPerOpMs"].as_i64().unwrap();
    let ops = spec["ops"].as_array().unwrap();

    let mut got_results: Vec<Value> = Vec::new();

    for (i, op) in ops.iter().enumerate() {
        let anchor = base_ms + (i as i64) * step_ms;
        clock.store(anchor, Ordering::SeqCst);
        let kind = op["kind"].as_str().unwrap();
        let chat_id = op.get("chatId").and_then(Value::as_str);
        let user_id = op.get("userId").and_then(Value::as_str);

        match kind {
            "manualStart" => {
                let r =
                    start_autonomous_room_manually(&db, &deps, chat_id.unwrap(), user_id.unwrap())
                        .await
                        .unwrap_or_else(|e| panic!("manualStart op {i}: {e:?}"));
                got_results.push(match r {
                    StartManualRunResult::Ok { run_id, job_id } => json!({
                        "op": "manualStart", "chatId": chat_id, "ok": true,
                        "runId": run_id, "jobId": job_id, "reason": null, "message": null,
                    }),
                    StartManualRunResult::Failed { reason, message } => json!({
                        "op": "manualStart", "chatId": chat_id, "ok": false,
                        "runId": null, "jobId": null,
                        "reason": reason.as_str(), "message": message,
                    }),
                });
            }
            "pause" => {
                let r = pause_autonomous_room(&db, &deps, chat_id.unwrap())
                    .await
                    .unwrap_or_else(|e| panic!("pause op {i}: {e:?}"));
                got_results.push(json!({
                    "op": "pause", "chatId": chat_id, "ok": r.ok, "message": r.message,
                }));
            }
            "stop" => {
                let r = stop_autonomous_room(&db, &deps, chat_id.unwrap())
                    .await
                    .unwrap_or_else(|e| panic!("stop op {i}: {e:?}"));
                got_results.push(json!({
                    "op": "stop", "chatId": chat_id, "ok": r.ok, "message": r.message,
                }));
            }
            "resume" => {
                let r = resume_autonomous_room(&db, &deps, chat_id.unwrap(), user_id.unwrap())
                    .await
                    .unwrap_or_else(|e| panic!("resume op {i}: {e:?}"));
                got_results.push(match r {
                    StartManualRunResult::Ok { run_id, job_id } => json!({
                        "op": "resume", "chatId": chat_id, "ok": true,
                        "runId": run_id, "jobId": job_id, "reason": null, "message": null,
                    }),
                    StartManualRunResult::Failed { reason, message } => json!({
                        "op": "resume", "chatId": chat_id, "ok": false,
                        "runId": null, "jobId": null,
                        "reason": reason.as_str(), "message": message,
                    }),
                });
            }
            "updateSettings" => {
                let patch = parse_patch(op.get("patch").unwrap_or(&Value::Null));
                let r = update_autonomous_room_settings(
                    &db,
                    &deps,
                    chat_id.unwrap(),
                    user_id.unwrap(),
                    &patch,
                )
                .await
                .unwrap_or_else(|e| panic!("updateSettings op {i}: {e:?}"));
                got_results.push(match r {
                    UpdateAutonomousRoomSettingsResult::Ok {
                        clamped_destructive,
                    } => json!({
                        "op": "updateSettings", "chatId": chat_id, "ok": true,
                        "clampedDestructive": clamped_destructive,
                        "reason": null, "message": null,
                    }),
                    UpdateAutonomousRoomSettingsResult::Failed { reason, message } => json!({
                        "op": "updateSettings", "chatId": chat_id, "ok": false,
                        "clampedDestructive": null,
                        "reason": reason.as_str(), "message": message,
                    }),
                });
            }
            "begin" => {
                let now_iso = quilltap_core::clock::iso_from_unix_ms(anchor);
                let r = begin_autonomous_run(
                    &db,
                    &deps,
                    BeginAutonomousRunInput {
                        chat_id: chat_id.unwrap().to_string(),
                        user_id: user_id.unwrap().to_string(),
                        run_id: op["runId"].as_str().unwrap().to_string(),
                        now_iso,
                        schedule_last_run_at: None,
                        schedule_next_run_at: None,
                        on_enqueue_failure: OnEnqueueFailure::Error,
                    },
                )
                .await
                .unwrap_or_else(|e| panic!("begin op {i}: {e:?}"));
                got_results.push(match r {
                    BeginAutonomousRunResult::Ok { run_id, job_id } => json!({
                        "op": "begin", "chatId": chat_id, "ok": true,
                        "runId": run_id, "jobId": job_id,
                        "reason": null, "message": null, "hasCause": false,
                    }),
                    BeginAutonomousRunResult::Failed {
                        reason,
                        message,
                        cause,
                    } => json!({
                        "op": "begin", "chatId": chat_id, "ok": false,
                        "runId": null, "jobId": null,
                        "reason": reason.as_str(), "message": message,
                        "hasCause": cause.is_some(),
                    }),
                });
            }
            "reconcileFailed" => {
                let run_id = op.get("runId").and_then(Value::as_str);
                reconcile_failed_autonomous_turn(
                    &db,
                    &deps,
                    chat_id,
                    run_id,
                    op["reason"].as_str().unwrap(),
                )
                .await;
                got_results.push(json!({
                    "op": "reconcileFailed", "chatId": chat_id, "done": true,
                }));
            }
            "reconcileStartup" => {
                let count = reconcile_autonomous_runs_at_startup(&db, &deps)
                    .await
                    .unwrap_or_else(|e| panic!("reconcileStartup op {i}: {e:?}"));
                got_results.push(json!({
                    "op": "reconcileStartup", "reconciledCount": count,
                }));
            }
            other => panic!("unknown op kind: {other}"),
        }
    }

    // Dumps (chats already stable by pinned id; jobs/messages re-sorted below).
    let mut got_dumps = json!({
        "chats": db
            .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
            .expect("dump chats"),
        "background_jobs": db
            .read_main(|conn| dump_table_json_conn(conn, "background_jobs", "id"))
            .expect("dump background_jobs"),
        "chat_messages": db
            .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "id"))
            .expect("dump chat_messages"),
    });
    let mut oracle_dumps = oracle["dumps"].clone();

    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    // ------------------------------------------------------------------
    // Normalization (identical on both sides)
    // ------------------------------------------------------------------
    // Pinned ids: every uuid literal appearing in the spec (fixture ids,
    // seeded run ids, the wrong-run/begin probes).
    let uuid_re =
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap();
    let pinned_ids: HashSet<String> = uuid_re
        .find_iter(&spec_text)
        .map(|m| m.as_str().to_lowercase())
        .collect();
    // Pinned times: every per-op anchor + every 2031-era timestamp literal in
    // the spec (the seeded cron slots) + the canned cron outputs.
    let mut pinned_times: HashSet<String> = (0..ops.len())
        .map(|i| quilltap_core::clock::iso_from_unix_ms(base_ms + (i as i64) * step_ms))
        .collect();
    let ts_re = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap();
    for m in ts_re.find_iter(&spec_text) {
        pinned_times.insert(m.as_str().to_string());
    }
    for ms in [1_925_024_400_000_i64, 1_925_447_400_000_i64] {
        pinned_times.insert(quilltap_core::clock::iso_from_unix_ms(ms));
    }

    let normalize_side = |dumps: &mut Value, results: &mut Vec<Value>| {
        let mut n = Normalizer::new(pinned_ids.clone(), pinned_times.clone());
        // Stable content sorts BEFORE the first-seen walk.
        sort_rows(&mut dumps["background_jobs"], job_sort_key);
        sort_rows(&mut dumps["chat_messages"], |r| {
            (
                r.get("chatId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                String::new(),
            )
        });
        // Walk order: chats → jobs → messages (rows sorted, columns in dump
        // order), then the op results in op order.
        for table in ["chats", "background_jobs", "chat_messages"] {
            n.norm_value(&mut dumps[table]);
        }
        for r in results.iter_mut() {
            n.norm_value(r);
        }
    };

    let mut oracle_results: Vec<Value> = oracle["results"].as_array().unwrap().clone();
    normalize_side(&mut got_dumps, &mut got_results);
    normalize_side(&mut oracle_dumps, &mut oracle_results);

    // ------------------------------------------------------------------
    // Diff
    // ------------------------------------------------------------------
    assert_eq!(
        got_results.len(),
        oracle_results.len(),
        "op-count mismatch — regenerate the oracle NDJSON"
    );
    for (i, (got, want)) in got_results.iter().zip(oracle_results.iter()).enumerate() {
        assert_eq!(
            got,
            want,
            "op {i} ({}) result diverged\n  rust:   {got}\n  oracle: {want}",
            ops[i]["kind"].as_str().unwrap_or("?")
        );
    }

    for table in ["chats", "background_jobs", "chat_messages"] {
        let got = &got_dumps[table];
        let want = &oracle_dumps[table];
        assert_eq!(got["columns"], want["columns"], "{table} column set/order");
        let got_rows = got["rows"].as_array().unwrap();
        let want_rows = want["rows"].as_array().unwrap();
        assert_eq!(
            got_rows.len(),
            want_rows.len(),
            "{table} row count (rust {} vs oracle {})",
            got_rows.len(),
            want_rows.len()
        );
        for (i, (g, w)) in got_rows.iter().zip(want_rows.iter()).enumerate() {
            if g != w {
                // Column-level diff for a readable failure.
                let (gobj, wobj) = (g.as_object().unwrap(), w.as_object().unwrap());
                for (k, gv) in gobj {
                    let wv = &wobj[k];
                    assert_eq!(
                        gv, wv,
                        "{table} row {i} column {k} diverged\n  rust:   {gv}\n  oracle: {wv}"
                    );
                }
                panic!("{table} row {i} diverged (key-set difference)");
            }
        }
    }

    eprintln!(
        "OK: enclave-lifecycle tier-2 matched oracle ({} ops; {} chats, {} jobs, {} banners).",
        got_results.len(),
        got_dumps["chats"]["rows"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
        got_dumps["background_jobs"]["rows"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
        got_dumps["chat_messages"]["rows"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0),
    );
}
