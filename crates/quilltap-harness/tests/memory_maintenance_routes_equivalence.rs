//! P4.D297 route differential: `api::memory_maintenance_routes::*` (the three
//! maintenance verbs) and `api::memory_maintenance_config::*` (P4.D288's two
//! config arms, pinned here per L-7 / L-9) vs v4's REAL memories route
//! handlers (`app/api/v1/memories/route.ts` at the `83334b9a5` pin) — POST
//! `?action=maintain`, GET `?action=maintenance-status`, POST `?action=repair`,
//! GET/POST `?action=maintenance-config`, and the RETIRED actions — each case
//! over a FRESH copy of the committed memories-{main,mount}.db pair, the
//! maintenance ledger reset per case.
//!
//! Covered (`harness/oracle/fixtures/memory-maintenance-routes.json`):
//! `maintain` — a dry run of one character over every phase, of the LLM-free
//! phases with the bounds (`maxClustersPerRun` clamped to 40, `cap`), of
//! `consolidate` (implying `mirror`), of every character for an LLM-free
//! phase; the dry-run rule refusing a user-wide dry run that may call the
//! model; the live enqueue of one character (payload bytes, the widen on a
//! second call), of every character (an absent body is `{}`); the validation
//! class (a bad phase, an empty list, a bad uuid, the cap bounds, a
//! fractional / zero `maxClustersPerRun`, a bad consolidation patch, a string
//! `dryRun`, an array body); the 404 (before the dry-run rule).
//! `maintenance-status` — every entry / one / present-empty / 404, and the
//! ledger SEEDED from planted COMPLETED / PROCESSING jobs. `repair` — dry,
//! live one, live all (absent body), 404, bad body. `maintenance-config` —
//! the default GET, the toggle merge, replace-the-rest, the 100 / 100000
//! bounds (both edges, the overrides too), unknown keys stripped, and L-7's
//! two stored shapes (no `phases` → every toggle ON; `phases: null` → WHOLE
//! defaults + C1-5's `[InstanceSettings] … failed to parse` WARN, by TAG).
//! The retired actions (`housekeep` GET/POST, `housekeep-sweep`,
//! `housekeeping-config` GET/POST, `consolidate`): v4 answers its `Unknown
//! action` 400; v5's wire has no such variant — the `Request` decode refuses
//! the retired tag with serde's `unknown variant` (R-G: the 400 class).
//!
//! Comparands: status; success bodies whole (a dry report's clocks blanked;
//! minted ids by order of first appearance); errors as status + `error`
//! (v5's house class answers no `details` — R-K); the `background_jobs` rows
//! (payload TEXT byte-compared); the stored `memoryMaintenance` TEXT after a
//! config call (byte-compared); the captured `[Memories API]`, `[Maintenance]`,
//! `memory:maintenance*` and `memory:repair` lines (v4's key order; arrays /
//! objects as `…Json`); the two façade lines by TAG only.
//!
//! **R-N / L-5 (v5-only rows).** With no maintenance runner installed, a dry
//! run answers the loud typed refusal — AFTER the 400 and 404 arms; a live
//! enqueue needs no runner.
//!
//! Generate (Node 24, from the v4 checkout or the round's pin — a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=${M:-/tmp/qt-maintenance-routes-oracle}
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   rm -f ${QT_OUT:-/tmp/qt-maintenance-routes.ndjson}
//!   QT_FIXTURE_MEM_MAIN=$V5W/crates/quilltap-web/tests/fixtures/memories-main.db \
//!   QT_FIXTURE_MEM_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/memories-mount.db \
//!   QT_ORACLE_OUT=${QT_OUT:-/tmp/qt-maintenance-routes.ndjson} \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- memory-maintenance-routes
//!   cd $V5W
//!   QT_ORACLE_MAINTENANCE_ROUTES=${QT_OUT:-/tmp/qt-maintenance-routes.ndjson} \
//!     cargo test -p quilltap-harness --test memory_maintenance_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::api::memory_maintenance_config as config;
use quilltap_core::api::memory_maintenance_routes as routes;
use quilltap_core::api::types::{ErrorKind, Request, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::CannedCompletionProvider;
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::memory_maintenance::ledger::reset_maintenance_ledger_for_tests;
use quilltap_core::services::memory_maintenance::run::run_maintenance;
use quilltap_core::services::memory_maintenance::runner::{
    clear_maintenance_runner, install_maintenance_runner, MaintenanceRunFuture, MaintenanceRunner,
};
use quilltap_core::services::memory_maintenance::types::{MaintenanceContext, MaintenanceRequest};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Map, Value};

/// v4's oracle seam: a dry run's `runMaintenance` reads this fixed clock.
const RUN_NOW: &str = "2026-10-08T12:00:00.000Z";

const TAGS: [&str; 2] = [
    "[MemorySettings] Resolved memory settings",
    "[InstanceSettings] memoryMaintenance failed to parse — using defaults",
];

/// The dry run over canned providers (no model is reachable: the fixture's
/// consolidation candidates are unembedded), "as of" [`RUN_NOW`].
struct FixtureClockRunner {
    completion: CannedCompletionProvider,
    embedding: CannedEmbeddingProvider,
    executor: CheapLlmTaskExecutor,
    now_ms: i64,
}

impl MaintenanceRunner for FixtureClockRunner {
    fn run<'a>(
        &'a self,
        db: &'a Db,
        request: MaintenanceRequest,
        mut ctx: MaintenanceContext,
    ) -> MaintenanceRunFuture<'a> {
        ctx.now_ms = ctx.now_ms.or(Some(self.now_ms));
        Box::pin(run_maintenance(
            db,
            &self.completion,
            &self.embedding,
            &self.executor,
            request,
            ctx,
        ))
    }
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 423,
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn status_and_body(r: &Response) -> (u16, Value) {
    match r {
        Response::Error(e) => (status_of(e.kind), json!({ "error": e.message })),
        other => {
            let v = serde_json::to_value(other).unwrap();
            (200, v.get("data").cloned().unwrap_or(Value::Null))
        }
    }
}

/// Per-case, per-side id masking: every uuid not in `keep` → `<id#n>` by first
/// appearance.
struct Masker<'k> {
    keep: &'k [String],
    minted: HashMap<String, usize>,
}

impl<'k> Masker<'k> {
    fn new(keep: &'k [String]) -> Self {
        Masker {
            keep,
            minted: HashMap::new(),
        }
    }

    fn text(&mut self, s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        while i < b.len() {
            if i + 36 <= b.len()
                && (0..36).all(|k| match k {
                    8 | 13 | 18 | 23 => b[i + k] == b'-',
                    _ => b[i + k].is_ascii_hexdigit(),
                })
            {
                let id = &s[i..i + 36];
                if self.keep.iter().any(|k| k == id) {
                    out.push_str(id);
                } else {
                    let n = self.minted.len();
                    let k = *self.minted.entry(id.to_string()).or_insert(n);
                    out.push_str(&format!("<id#{k}>"));
                }
                i += 36;
            } else {
                let ch = s[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out
    }
}

/// Numbers compared as f64; a dry report's clocks blanked.
fn normalize(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| {
                    let x = match k.as_str() {
                        "finishedAt" | "durationMs" => json!("<clock>"),
                        _ => normalize(x),
                    };
                    (k.clone(), x)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

fn mask_duration(s: &str) -> String {
    match s.find("durationMs=") {
        Some(i) => {
            let tail = &s[i + "durationMs=".len()..];
            let end = tail.find(' ').unwrap_or(tail.len());
            format!("{}durationMs=<ms>{}", &s[..i], &tail[end..])
        }
        None => s.to_string(),
    }
}

fn render_oracle_line(line: &Value) -> String {
    let mut out = format!(
        "{} {}",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
    if let Some(ctx) = line["context"].as_object() {
        for (k, v) in ctx {
            match v {
                Value::String(s) => out.push_str(&format!(" {k}={s}")),
                Value::Array(_) | Value::Object(_) => {
                    out.push_str(&format!(" {k}Json={}", serde_json::to_string(v).unwrap()))
                }
                // L-8: `repair`'s `repaired` is an object-valued field, so its
                // key is `repairedJson` on every arm — `null` included.
                Value::Null if k == "repaired" => out.push_str(" repairedJson=null"),
                other => out.push_str(&format!(" {k}={other}")),
            }
        }
    }
    mask_duration(&out)
}

fn strip_target(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next().unwrap_or_default();
    let _ = parts.next();
    mask_duration(&format!("{level} {}", parts.next().unwrap_or_default()))
}

fn v5_captured(line: &str) -> bool {
    let message = line.split_once(' ').map_or("", |(_, rest)| rest);
    !TAGS.iter().any(|t| line.contains(t))
        && (message.starts_with("[Memories API]")
            || message.starts_with("[Maintenance]")
            || line.contains(" module=memory:maintenance")
            || line.contains(" module=memory:repair"))
}

fn open_copy(dir: &std::path::Path, pepper: &str) -> Db {
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures");
    let main = dir.join("main.db");
    let mount = dir.join("mount.db");
    std::fs::copy(fx.join("memories-main.db"), &main).unwrap();
    std::fs::copy(fx.join("memories-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open the memories-web copy")
}

fn jobs(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare(
            "SELECT id, userId, type, status, payload, maxAttempts, scheduledAt, createdAt \
             FROM background_jobs ORDER BY type, payload, status",
        )?;
        let rows = stmt.query_map([], |r| {
            let scheduled: String = r.get(6)?;
            let created: String = r.get(7)?;
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "userId": r.get::<_, String>(1)?,
                "type": r.get::<_, String>(2)?,
                "status": r.get::<_, String>(3)?,
                "payload": r.get::<_, String>(4)?,
                "maxAttempts": r.get::<_, f64>(5)?,
                "scheduledDelayMs": quilltap_core::clock::iso_to_ms(&scheduled).unwrap()
                    - quilltap_core::clock::iso_to_ms(&created).unwrap(),
            }))
        })?;
        Ok(Value::Array(rows.collect::<Result<Vec<_>, _>>()?))
    })
    .unwrap()
}

fn stored_maintenance(db: &Db) -> Value {
    db.read_main(|c| {
        Ok(c.query_row(
            "SELECT \"value\" FROM \"instance_settings\" WHERE \"key\" = 'memoryMaintenance'",
            [],
            |r| r.get::<_, String>(0),
        )
        .map_or(Value::Null, Value::String))
    })
    .unwrap()
}

async fn call(db: &Db, user_id: &str, c: &Value) -> Response {
    let action = c["action"].as_str().unwrap();
    let method = c["method"].as_str().unwrap();
    let body = c.get("body").cloned().unwrap_or(Value::Null);
    if let Some(tag) = c.get("retiredTag").and_then(Value::as_str) {
        // v5's wire: the retired verb no longer decodes (the `Request` enum
        // carries no such variant) — the transports answer the decode error.
        let mut env = match &body {
            Value::Object(o) => o.clone(),
            _ => Map::new(),
        };
        env.insert("type".into(), json!(tag));
        return match serde_json::from_value::<Request>(Value::Object(env)) {
            Ok(r) => panic!("retired tag {tag} still decodes: {r:?}"),
            Err(e) => Response::error(ErrorKind::BadRequest, e.to_string()),
        };
    }
    match (action, method) {
        ("maintain", _) => routes::memory_maintain(db, user_id, body, "UTC").await,
        ("maintenance-status", _) => {
            routes::memory_maintenance_status(db, c["query"]["characterId"].as_str())
        }
        ("repair", _) => routes::memory_repair(db, user_id, body).await,
        ("maintenance-config", "GET") => config::memory_maintenance_config_get(db),
        ("maintenance-config", _) => config::memory_maintenance_config_set(db, user_id, body).await,
        other => panic!("unknown route {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn memory_maintenance_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MAINTENANCE_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_MAINTENANCE_ROUTES (see header).");
        return;
    };
    global_capture::install();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures");
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("memories-web.json")).unwrap())
            .unwrap();
    let corpus: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("memory-maintenance-routes.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let user_id = spec["userId"].as_str().unwrap();
    let keep: Vec<String> = [
        "b1000000-0000-4000-8000-000000000001",
        "b1000000-0000-4000-8000-000000000002",
        "b1000000-0000-4000-8000-000000000003",
        "00000000-0000-4000-8000-0000000000ff",
        "b1b00000-0000-4000-8000-000000000001",
        "b1b00000-0000-4000-8000-000000000002",
        user_id,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    install_maintenance_runner(Arc::new(FixtureClockRunner {
        completion: CannedCompletionProvider::new(),
        embedding: CannedEmbeddingProvider::new(),
        executor: CheapLlmTaskExecutor::new(),
        now_ms: quilltap_core::clock::iso_to_ms(RUN_NOW).unwrap(),
    }));

    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        oracle.len(),
        "case count — regenerate the oracle"
    );
    let mut failures: Vec<String> = Vec::new();
    let mut lines_compared = 0usize;
    for (c, want) in cases.iter().zip(&oracle) {
        let name = c["name"].as_str().unwrap();
        assert_eq!(want["name"], name, "case order");
        reset_maintenance_ledger_for_tests();
        let scratch = tempfile::Builder::new()
            .prefix("qt-maintenance-routes-rust-")
            .tempdir()
            .unwrap();
        let db = open_copy(scratch.path(), pepper);
        let stored = c.get("stored").and_then(Value::as_str).map(str::to_string);
        let plants: Vec<String> = c
            .get("plantSql")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
            .unwrap_or_default();
        db.write(move |w| {
            let conn = w.main().connection();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL)",
            )?;
            if let Some(v) = stored {
                conn.execute(
                    "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('memoryMaintenance', ?1) \
                     ON CONFLICT(\"key\") DO UPDATE SET \"value\" = excluded.\"value\"",
                    [v],
                )?;
            }
            for sql in plants {
                conn.execute_batch(&sql)?;
            }
            Ok(())
        })
        .await
        .unwrap();

        let twice = c.get("twice").and_then(Value::as_bool).unwrap_or(false);
        let ((first, resp), captured) = global_capture::capture_async(Box::pin(async {
            let first = call(&db, user_id, c).await;
            let resp = if twice {
                call(&db, user_id, c).await
            } else {
                first.clone()
            };
            (first, resp)
        }))
        .await;
        let mut gm = Masker::new(&keep);
        let mut wm = Masker::new(&keep);
        let (status, got_body) = status_and_body(&resp);
        let want_status = want["status"].as_u64().unwrap();
        if u64::from(status) != want_status {
            failures.push(format!(
                "{name}: status v5 {status} v4 {want_status} ({got_body})"
            ));
        }
        if c.get("retiredTag").is_some() {
            // R-G: the 400 class, the retired tag named on both sides.
            let tag = c["retiredTag"].as_str().unwrap();
            let action = c["action"].as_str().unwrap();
            let got_err = got_body["error"].as_str().unwrap_or_default();
            if !got_err.contains(&format!("unknown variant `{tag}`")) {
                failures.push(format!("{name}: v5 decode error {got_err:?}"));
            }
            if want["body"]["error"] != json!(format!("Unknown action: {action}")) {
                failures.push(format!("{name}: v4 error {}", want["body"]["error"]));
            }
            continue;
        }
        let want_body = if want_status == 200 {
            want["body"].clone()
        } else {
            json!({ "error": want["body"]["error"] })
        };
        let g = gm.text(&normalize(&got_body).to_string());
        let w = wm.text(&normalize(&want_body).to_string());
        if g != w {
            failures.push(format!("{name}: body\n  v5 {g}\n  v4 {w}"));
        }
        if twice {
            let (_, first_body) = status_and_body(&first);
            if first_body["jobId"] != got_body["jobId"] {
                failures.push(format!("{name}: the widen returned a different job id"));
            }
        }
        if let Some(want_jobs) = want.get("jobs") {
            let g = gm.text(&normalize(&jobs(&db)).to_string());
            let w = wm.text(&normalize(want_jobs).to_string());
            if g != w {
                failures.push(format!("{name}: jobs\n  v5 {g}\n  v4 {w}"));
            }
        }
        if let Some(want_stored) = want.get("storedAfter") {
            let got_stored = stored_maintenance(&db);
            if &got_stored != want_stored {
                failures.push(format!(
                    "{name}: storedAfter\n  v5 {got_stored}\n  v4 {want_stored}"
                ));
            }
        }
        let got_lines: Vec<String> = captured
            .iter()
            .map(|l| strip_target(l))
            .filter(|l| v5_captured(l))
            .map(|l| gm.text(&l))
            .collect();
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| !TAGS.contains(&l["message"].as_str().unwrap()))
            .map(|l| wm.text(&render_oracle_line(l)))
            .collect();
        lines_compared += want_lines.len();
        if got_lines != want_lines {
            failures.push(format!(
                "{name}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
        for tag in TAGS {
            let g = captured.iter().filter(|l| l.contains(tag)).count();
            let w = want["lines"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| l["message"] == tag)
                .count();
            if g != w {
                failures.push(format!("{name}: tag {tag:?} v5 {g} v4 {w}"));
            }
        }
    }

    // ── R-N / L-5: no runner → the refusal, after the 400 / 404 arms ───────
    clear_maintenance_runner();
    {
        let scratch = tempfile::Builder::new()
            .prefix("qt-maintenance-routes-rust-")
            .tempdir()
            .unwrap();
        let db = open_copy(scratch.path(), pepper);
        let orla = "b1000000-0000-4000-8000-000000000002";
        for (label, body, want_status, want_error) in [
            (
                "no_runner_dry",
                json!({"characterId": orla, "dryRun": true, "phases": ["demote"]}),
                500u16,
                routes::MAINTENANCE_RUNNER_MISSING,
            ),
            (
                "no_runner_400_first",
                json!({"characterId": orla, "dryRun": true, "phases": ["dedup"]}),
                400,
                "Validation error",
            ),
            (
                "no_runner_404_first",
                json!({"characterId": "00000000-0000-4000-8000-0000000000ff", "dryRun": true}),
                404,
                "Character not found",
            ),
            (
                "no_runner_dry_rule_first",
                json!({"dryRun": true}),
                400,
                "A dry run that includes consolidation names one character (characterId)",
            ),
        ] {
            let (status, body) =
                status_and_body(&routes::memory_maintain(&db, user_id, body, "UTC").await);
            if status != want_status || body["error"] != json!(want_error) {
                failures.push(format!("{label}: {status} {body}"));
            }
        }
        let (status, body) = status_and_body(
            &routes::memory_maintain(&db, user_id, json!({"characterId": orla}), "UTC").await,
        );
        if status != 200 || body["jobIds"].as_array().map(Vec::len) != Some(1) {
            failures.push(format!("no_runner_live_enqueues: {status} {body}"));
        }
    }

    reset_maintenance_ledger_for_tests();
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(lines_compared >= 50, "lines compared: {lines_compared}");
    eprintln!(
        "OK: memory maintenance routes matched oracle ({} cases, {lines_compared} lines; R-N 5 v5-only rows).",
        cases.len()
    );
}
