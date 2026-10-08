//! P4.D259 tier-2 differential — v4 `f5e953a3f`'s PHASE 0.75 daily backup +
//! optimize pass and the never-ported physical backups it calls:
//! `quilltap_core::services::daily_db_optimize` and
//! `quilltap_core::services::physical_backup` against v4's REAL
//! `runDailyDbOptimize` / `optimizeDatabase` / `localDateStamp` and
//! `createPhysicalBackup` / `createLLMLogsPhysicalBackup` /
//! `createMountIndexPhysicalBackup` / `applyRetentionPolicy`
//! (`harness/oracle/cases/daily-db-optimize.ts`).
//!
//! Each `row` is rebuilt from its recipe — the committed `chat-delete-*` trio
//! COPIED into a fresh `<base>/data/`, the deleted-rows plant run through a
//! keyed writable open, the garbage sibling, the state file (bytes or a
//! DIRECTORY at the path), the `data/backups/` tree (the planted names, each
//! file `plant <name>`) — opened as the host opens an instance (`Db::open`,
//! absent files as absent partitions), and its actions run with the SAME
//! `now_ms` and zone:
//!   - `pass` → `run_daily_db_optimize`;
//!   - `phase2` → `run_startup_backups` (backend.ts `connect()`'s chained
//!     trio + retention, in v4's microtask order);
//!   - `retention` → `apply_retention_policy`;
//!   - `optimizeReadonly` → `optimize_database` over a read-pool (read-only)
//!     connection.
//!
//! Compared per action: every line of the two modules (and the four
//! root-logger lines the feature owns) — count, order, level, message, keys
//! IN ORDER, values — with v5's `…Json` fields parsed back into v4's array /
//! object, the base dir normalized to `<base>`, and the volatile values
//! (`ms`, `elapsedMs`, `reclaimedBytes`, `sizeBefore`, `sizeAfter`,
//! `sizeBytes`) normalized to `"<n>"`. Per row: the state file's BYTES, the
//! `data/backups/` listing (names + each new backup's table count opened with
//! the key — the copy is keyed), and per database `page_count`,
//! `freelist_count` and every `sqlite_stat1` row. Tier 1: `stamp` rows over
//! `local_date_stamp`. The journal header is in NO comparand (v4's migration
//! openers flip each file to WAL; v5's writers are TRUNCATE — R-B).
//!
//! Named both-ways tables (a convergence trips them):
//!   - `STAT4_NOT_COMPILED` — v4's `better-sqlite3-multiple-ciphers` is built
//!     with `SQLITE_ENABLE_STAT4` (measured: `PRAGMA compile_options`); this
//!     workspace's `quilltap-sqlite3mc-sys` is not. So v4's `ANALYZE`
//!     repopulates `sqlite_stat4` and v5's EMPTIES it (the committed trio
//!     already carries v4-written stat4 rows), leaving v5's file smaller. Per
//!     database: `freelist_count` and every `sqlite_stat1` row compare
//!     EXACTLY; `page_count` compares exactly whenever the stat4 counts agree,
//!     and otherwise ONLY this explanation is accepted (v5's build without
//!     STAT4, v5's table empty, v4's populated, v4's file no smaller). A build
//!     that gains STAT4 fails the table — retire it then. (A FINDING for the
//!     human, not ported: the define lives in the pinned sys crate.)
//!   - `DEGRADED_OPTIMIZE_ERROR_TEXT` — the degraded sibling's per-database
//!     ERROR: v4 re-opens and quotes SQLite (`file is not a database`); v5
//!     never re-opens and renders the partition's degraded sentence. The key
//!     is compared; the value is pinned to EXACTLY these two texts.
//!   - `WAL_CHECKPOINT_UNDER_TRUNCATE` — v4's post-VACUUM `wal_checkpoint`
//!     has no v5 analog: its WARN is pinned absent on BOTH sides over the
//!     whole corpus (v4 never fails the checkpoint here; v5 never runs it).
//!
//! Red-first (the lane record): on unported `main` this binary did not compile
//! (no `daily_db_optimize` / `physical_backup` module); with the two entry
//! points stubbed to today's behaviour (no pass, no backups, no retention)
//! every pass / phase2 / retention row was RED.
//!
//! Generate the oracles (Node 24, from the v4 checkout — or the pinned
//! worktree; a `tsx` script, run once per zone):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_DDO_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-main.db \
//!     QT_FIXTURE_DDO_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-mount.db \
//!     QT_FIXTURE_DDO_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-llmlogs.db \
//!     TZ=UTC QT_ORACLE_OUT=/tmp/oracle-daily-db-optimize.ndjson \
//!     $N/npx tsx $V5W/harness/oracle/cases/daily-db-optimize.ts
//!   QT_FIXTURE_DDO_MAIN=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-main.db \
//!     QT_FIXTURE_DDO_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-mount.db \
//!     QT_FIXTURE_DDO_LLMLOGS=$V5W/crates/quilltap-web/tests/fixtures/chat-delete-llmlogs.db \
//!     TZ=America/Chicago QT_ORACLE_OUT=/tmp/oracle-daily-db-optimize-chicago.ndjson \
//!     $N/npx tsx $V5W/harness/oracle/cases/daily-db-optimize.ts
//! Run:
//!   QT_ORACLE_DAILY_DB_OPTIMIZE=/tmp/oracle-daily-db-optimize.ndjson \
//!     QT_ORACLE_DAILY_DB_OPTIMIZE_CHICAGO=/tmp/oracle-daily-db-optimize-chicago.ndjson \
//!     cargo test -p quilltap-harness --test daily_db_optimize_equivalence -- --nocapture
//!
//! Skips (does not fail) when an env var is unset — the standing gated
//! discipline (each zone independently).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::host_zone::TimeZone;
use quilltap_core::services::daily_db_optimize::{
    local_date_stamp, optimize_database, run_daily_db_optimize,
};
use quilltap_core::services::physical_backup::{apply_retention_policy, run_startup_backups};
use serde_json::{json, Map, Value};

/// The rows' lines, as v4 records them: `(level, message, fields)`.
type Line = (String, String, Vec<(String, Value)>);

const OWNED_MODULES: [&str; 2] = ["startup:daily-db-optimize", "database:physical-backup"];
const OWNED_ROOT: [&str; 4] = [
    "Startup physical backup or retention policy failed",
    "LLM logs startup physical backup failed",
    "Mount index startup physical backup failed",
    "Daily database optimize failed — continuing startup",
];
const VOLATILE: [&str; 6] = [
    "ms",
    "elapsedMs",
    "reclaimedBytes",
    "sizeBefore",
    "sizeAfter",
    "sizeBytes",
];

/// `DEGRADED_OPTIMIZE_ERROR_TEXT` — (v4's value, v5's value) for the degraded
/// sibling's per-database ERROR.
const DEGRADED_OPTIMIZE_ERROR_TEXT: (&str, &str) = (
    "file is not a database",
    "LLM logs database is in degraded mode",
);

/// `WAL_CHECKPOINT_UNDER_TRUNCATE` — the WARN neither side may log.
const WAL_CHECKPOINT_UNDER_TRUNCATE: &str = "Post-optimize WAL checkpoint failed";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fixture_of(db: &str) -> PathBuf {
    fixtures_dir().join(match db {
        "main" => "chat-delete-main.db",
        "llmLogs" => "chat-delete-llmlogs.db",
        "mountIndex" => "chat-delete-mount.db",
        other => panic!("unknown db {other}"),
    })
}

fn file_of(db: &str) -> &'static str {
    match db {
        "main" => "quilltap.db",
        "llmLogs" => "quilltap-llm-logs.db",
        "mountIndex" => "quilltap-mount-index.db",
        other => panic!("unknown db {other}"),
    }
}

// ---- a structured, thread-scoped capture ------------------------------------

struct Fields {
    message: String,
    fields: Vec<(String, Value)>,
}

impl tracing::field::Visit for Fields {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.fields.push((f.name().to_string(), Value::from(v)));
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.fields.push((f.name().to_string(), Value::from(v)));
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.fields.push((f.name().to_string(), Value::from(v)));
    }
    fn record_f64(&mut self, f: &tracing::field::Field, v: f64) {
        // A JS number: a whole value is an integer in JSON.
        let value = if v.fract() == 0.0 && v.abs() < 9.0e15 {
            Value::from(v as i64)
        } else {
            Value::from(v)
        };
        self.fields.push((f.name().to_string(), value));
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        self.fields.push((f.name().to_string(), Value::from(v)));
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            self.message = format!("{v:?}");
        } else {
            self.fields
                .push((f.name().to_string(), Value::from(format!("{v:?}"))));
        }
    }
}

struct Capture(Arc<Mutex<Vec<Line>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut f = Fields {
            message: String::new(),
            fields: Vec::new(),
        };
        event.record(&mut f);
        self.0
            .lock()
            .unwrap()
            .push((event.metadata().level().to_string(), f.message, f.fields));
    }
}

fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<Line>) {
    use tracing_subscriber::layer::SubscriberExt;
    let lines = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Capture(lines.clone()));
    let out = {
        let _guard = tracing::subscriber::set_default(subscriber);
        f()
    };
    let lines = lines.lock().unwrap().clone();
    (out, lines)
}

// ---- normalization ----------------------------------------------------------

fn owned(fields: &[(String, Value)], message: &str) -> bool {
    match fields.iter().find(|(k, _)| k == "module") {
        Some((_, Value::String(m))) => OWNED_MODULES.contains(&m.as_str()),
        _ => OWNED_ROOT.contains(&message),
    }
}

fn normalize_value(key: &str, v: Value, base: Option<&str>) -> Value {
    if VOLATILE.contains(&key) {
        return Value::from("<n>");
    }
    match v {
        Value::String(s) => match base {
            Some(b) => Value::String(s.replace(b, "<base>")),
            None => Value::String(s),
        },
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|i| match i {
                    Value::Object(o) => Value::Object(
                        o.into_iter()
                            .map(|(k, v)| {
                                let n = normalize_value(&k, v, base);
                                (k, n)
                            })
                            .collect(),
                    ),
                    other => other,
                })
                .collect(),
        ),
        other => other,
    }
}

/// v4's recorded line, normalized: the root `service` / `environment` keys
/// dropped (v5 renders no root context).
fn v4_lines(lines: &Value) -> Vec<Line> {
    lines
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let fields = l["fields"]
                .as_object()
                .unwrap()
                .iter()
                .filter(|(k, _)| *k != "service" && *k != "environment")
                .map(|(k, v)| (k.clone(), normalize_value(k, v.clone(), None)))
                .collect();
            (
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap().to_string(),
                fields,
            )
        })
        .collect()
}

/// v5's captured lines, filtered to the feature's own and normalized: each
/// `…Json` field parsed back into v4's value under v4's key.
fn v5_lines(lines: Vec<Line>, base: &str) -> Vec<Line> {
    lines
        .into_iter()
        .filter(|(_, m, f)| owned(f, m))
        .map(|(level, message, fields)| {
            let fields = fields
                .into_iter()
                .map(|(k, v)| match k.strip_suffix("Json") {
                    Some(stem) => {
                        let parsed: Value = serde_json::from_str(v.as_str().unwrap()).unwrap();
                        (stem.to_string(), normalize_value(stem, parsed, Some(base)))
                    }
                    None => {
                        let n = normalize_value(&k, v, Some(base));
                        (k, n)
                    }
                })
                .collect();
            (level, message, fields)
        })
        .collect()
}

fn render(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|(l, m, f)| {
            let mut s = format!("{l} {m}");
            for (k, v) in f {
                s.push_str(&format!(" {k}={v}"));
            }
            s
        })
        .collect()
}

// ---- the instance -----------------------------------------------------------

fn key_hex(pepper: &str) -> String {
    quilltap_core::dbkey::pepper_b64_to_key_hex(pepper).unwrap()
}

fn open_keyed_readonly(path: &Path, pepper: &str) -> rusqlite::Connection {
    let c = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    c.execute_batch(&format!("PRAGMA key = \"x'{}'\";", key_hex(pepper)))
        .unwrap();
    c
}

struct Recipe {
    plant_sql: Vec<String>,
    garbage: (usize, usize, usize),
    pepper: String,
}

fn garbage(r: &Recipe) -> Vec<u8> {
    let (len, mul, add) = r.garbage;
    (0..len).map(|i| ((i * mul + add) & 255) as u8).collect()
}

/// Rebuild the row's instance under `base`; answers the planted backup names.
fn plant(base: &Path, row: &Value, recipe: &Recipe) -> Vec<String> {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let p = &row["plant"];
    let garbage_db = p["garbage"].as_str();
    let deleted: Vec<&str> = p["deleted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for d in p["dbs"].as_array().unwrap() {
        let d = d.as_str().unwrap();
        let path = data.join(file_of(d));
        if garbage_db == Some(d) {
            std::fs::write(&path, garbage(recipe)).unwrap();
            continue;
        }
        std::fs::copy(fixture_of(d), &path).unwrap();
        if deleted.contains(&d) {
            let w = Writer::open_writable(&path, &recipe.pepper).unwrap();
            for sql in &recipe.plant_sql {
                w.connection().execute_batch(sql).unwrap();
            }
        }
    }
    let state_path = data.join("db-optimize-state.json");
    match p["state"]["kind"].as_str().unwrap() {
        "text" => std::fs::write(&state_path, p["state"]["text"].as_str().unwrap()).unwrap(),
        "dir" => std::fs::create_dir(&state_path).unwrap(),
        "absent" => {}
        other => panic!("state kind {other}"),
    }
    let backups = data.join("backups");
    let mut names = Vec::new();
    match p["backups"]["kind"].as_str().unwrap() {
        "file" => std::fs::write(&backups, "not a directory").unwrap(),
        "tree" => {
            std::fs::create_dir(&backups).unwrap();
            for n in p["backups"]["files"].as_array().unwrap() {
                let n = n.as_str().unwrap();
                std::fs::write(backups.join(n), format!("plant {n}")).unwrap();
                names.push(n.to_string());
            }
        }
        "absent" => {}
        other => panic!("backups kind {other}"),
    }
    names
}

fn open_instance(data: &Path, pepper: &str) -> Db {
    let optional = |name: &str| {
        let p = data.join(name);
        p.exists().then_some(p)
    };
    Db::open(
        DbPaths {
            main: data.join("quilltap.db"),
            mount_index: optional("quilltap-mount-index.db"),
            llm_logs: optional("quilltap-llm-logs.db"),
        },
        pepper,
    )
    .expect("open the instance")
}

/// The row's outcome in the oracle's shape.
fn outcome(base: &Path, row: &Value, planted: &[String], pepper: &str) -> Value {
    let data = base.join("data");
    let state_path = data.join("db-optimize-state.json");
    let state_after = if state_path.is_dir() {
        Value::from("<dir>")
    } else if state_path.exists() {
        Value::from(std::fs::read_to_string(&state_path).unwrap())
    } else {
        Value::Null
    };
    let backups = data.join("backups");
    let backups_after = if backups.is_dir() {
        let mut names: Vec<String> = std::fs::read_dir(&backups)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        Value::Array(
            names
                .into_iter()
                .map(|name| {
                    let tables = if planted.contains(&name) {
                        Value::Null
                    } else {
                        let c = open_keyed_readonly(&backups.join(&name), pepper);
                        let n: i64 = c
                            .query_row(
                                "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
                                [],
                                |r| r.get(0),
                            )
                            .unwrap();
                        Value::from(n)
                    };
                    json!({ "name": name, "tables": tables })
                })
                .collect(),
        )
    } else if backups.exists() {
        Value::from("<file>")
    } else {
        Value::Null
    };
    let mut dbs = Map::new();
    let garbage_db = row["plant"]["garbage"].as_str();
    for d in row["plant"]["dbs"].as_array().unwrap() {
        let d = d.as_str().unwrap();
        if garbage_db == Some(d) {
            continue;
        }
        let c = open_keyed_readonly(&data.join(file_of(d)), pepper);
        let page_count: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
        let freelist: i64 = c
            .query_row("PRAGMA freelist_count", [], |r| r.get(0))
            .unwrap();
        let stat1 = if has_table(&c, "sqlite_stat1") {
            let mut st = c
                .prepare("SELECT tbl, idx, stat FROM sqlite_stat1 ORDER BY tbl, idx")
                .unwrap();
            let rows: Vec<Value> = st
                .query_map([], |r| {
                    Ok(json!({
                        "tbl": r.get::<_, Option<String>>(0)?,
                        "idx": r.get::<_, Option<String>>(1)?,
                        "stat": r.get::<_, Option<String>>(2)?,
                    }))
                })
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            Value::Array(rows)
        } else {
            Value::Null
        };
        let stat4_rows = if has_table(&c, "sqlite_stat4") {
            let n: i64 = c
                .query_row("SELECT count(*) FROM sqlite_stat4", [], |r| r.get(0))
                .unwrap();
            Value::from(n)
        } else {
            Value::Null
        };
        dbs.insert(
            d.to_string(),
            json!({ "pageCount": page_count, "freelist": freelist, "stat1": stat1, "stat4Rows": stat4_rows }),
        );
    }
    json!({ "stateAfter": state_after, "backupsAfter": backups_after, "dbs": Value::Object(dbs) })
}

fn has_table(c: &rusqlite::Connection, name: &str) -> bool {
    c.query_row(
        "SELECT count(*) FROM sqlite_master WHERE name = ?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

/// Whether the workspace's SQLite3MC build has `SQLITE_ENABLE_STAT4`.
fn build_has_stat4() -> bool {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    let mut st = c.prepare("PRAGMA compile_options").unwrap();
    let opts: Vec<String> = st
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    opts.iter().any(|o| o == "ENABLE_STAT4")
}

/// One database's outcome under `STAT4_NOT_COMPILED` (the module doc).
fn compare_db(name: &str, d: &str, v4: &Value, v5: &Value, failures: &mut Vec<String>) {
    for key in ["freelist", "stat1"] {
        if v4[key] != v5[key] {
            failures.push(format!(
                "{name} dbs.{d}.{key}: v4 {} v5 {}",
                v4[key], v5[key]
            ));
        }
    }
    if v4["stat4Rows"] == v5["stat4Rows"] {
        if v4["pageCount"] != v5["pageCount"] {
            failures.push(format!(
                "{name} dbs.{d}.pageCount: v4 {} v5 {}",
                v4["pageCount"], v5["pageCount"]
            ));
        }
        return;
    }
    // STAT4_NOT_COMPILED: v4's ANALYZE repopulated `sqlite_stat4`; v5's build
    // has no STAT4, so its ANALYZE EMPTIED the table (SQLite's non-STAT4
    // `openStatTable` deletes the rows of a stat table it does not maintain)
    // and the file is no larger (smaller once the samples outgrow a page). Any
    // other difference — or the
    // build gaining STAT4 — is a failure.
    let explained = !build_has_stat4()
        && v5["stat4Rows"] == json!(0)
        && v4["stat4Rows"].as_i64().is_some_and(|n| n > 0)
        && v4["pageCount"].as_i64() >= v5["pageCount"].as_i64();
    if !explained {
        failures.push(format!(
            "{name} dbs.{d}: STAT4_NOT_COMPILED does not explain v4 {v4} v5 {v5}"
        ));
    }
}

fn load(var: &str) -> Option<Vec<Value>> {
    let Ok(path) = std::env::var(var) else {
        eprintln!("SKIP: {var} unset (see the header)");
        return None;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    // 1 recipe + 6 stamp rows + 20 instance rows: a truncated regen must not
    // pass vacuously.
    assert_eq!(rows.len(), 27, "{var}: oracle row count");
    let starting = rows
        .iter()
        .filter(|r| r["kind"] == "row")
        .flat_map(|r| r["actions"].as_array().unwrap())
        .flat_map(|a| a["lines"].as_array().unwrap())
        .filter(|l| l["message"] == "Daily database optimize starting")
        .count();
    assert!(starting > 0, "{var}: a stale oracle (no pass ran)");
    Some(rows)
}

fn run_zone(rows: &[Value], expect_tz: &str) {
    let recipe_row = rows.iter().find(|r| r["kind"] == "recipe").unwrap();
    let g = &recipe_row["garbage"];
    let recipe = Recipe {
        plant_sql: recipe_row["plantSql"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect(),
        garbage: (
            g["len"].as_u64().unwrap() as usize,
            g["mul"].as_u64().unwrap() as usize,
            g["add"].as_u64().unwrap() as usize,
        ),
        pepper: recipe_row["pepper"].as_str().unwrap().to_string(),
    };

    let mut failures: Vec<String> = Vec::new();
    let mut compared = 0;
    for row in rows {
        match row["kind"].as_str().unwrap() {
            "recipe" => {}
            "stamp" => {
                assert_eq!(row["tz"], expect_tz);
                let zone = TimeZone::get(expect_tz).unwrap();
                let ms = row["nowMs"].as_i64().unwrap();
                let got = local_date_stamp(ms, &zone);
                if got != row["stamp"].as_str().unwrap() {
                    failures.push(format!("stamp {ms}: v4 {} v5 {got}", row["stamp"]));
                }
                compared += 1;
            }
            "row" => {
                assert_eq!(row["tz"], expect_tz);
                let zone = TimeZone::get(expect_tz).unwrap();
                let name = row["name"].as_str().unwrap();
                let scratch = tempfile::Builder::new()
                    .prefix(&format!("qt-ddo-{name}-"))
                    .tempdir()
                    .unwrap();
                let base = scratch.path();
                let base_text = base.display().to_string();
                let planted = plant(base, row, &recipe);
                let data = base.join("data");
                let db = open_instance(&data, &recipe.pepper);

                for (i, action) in row["actions"].as_array().unwrap().iter().enumerate() {
                    let now = action["nowMs"].as_i64().unwrap();
                    let op = action["op"].as_str().unwrap();
                    let (result, lines) = capture(|| match op {
                        "pass" => {
                            run_daily_db_optimize(&db, &data, &zone, now);
                            Value::Null
                        }
                        "phase2" => {
                            run_startup_backups(&db, &data, now, &zone);
                            Value::Null
                        }
                        "retention" => {
                            apply_retention_policy(&data, now, &zone);
                            Value::Null
                        }
                        "optimizeReadonly" => {
                            let o = db.read_main(|c| Ok(optimize_database(c, "main"))).unwrap();
                            json!({
                                "ok": o.ok,
                                "steps": o.steps.iter().map(|s| {
                                    let mut m = Map::new();
                                    m.insert("name".into(), Value::from(s.name));
                                    m.insert("ok".into(), Value::from(s.ok));
                                    if let Some(e) = &s.error {
                                        m.insert("error".into(), Value::from(e.as_str()));
                                    }
                                    Value::Object(m)
                                }).collect::<Vec<_>>(),
                            })
                        }
                        other => panic!("op {other}"),
                    });
                    let mut v4 = v4_lines(&action["lines"]);
                    let mut v5 = v5_lines(lines, &base_text);
                    // DEGRADED_OPTIMIZE_ERROR_TEXT: the key compared, the
                    // value pinned to exactly the recorded pair.
                    for (side, lines, want) in [
                        ("v4", &mut v4, DEGRADED_OPTIMIZE_ERROR_TEXT.0),
                        ("v5", &mut v5, DEGRADED_OPTIMIZE_ERROR_TEXT.1),
                    ] {
                        for (_, m, f) in lines.iter_mut() {
                            if m == "Database optimize failed; will retry on next launch" {
                                for (k, v) in f.iter_mut() {
                                    if k == "error" {
                                        assert_eq!(
                                            v.as_str(),
                                            Some(want),
                                            "DEGRADED_OPTIMIZE_ERROR_TEXT ({side}, {name})"
                                        );
                                        *v = Value::from("<degraded>");
                                    }
                                }
                            }
                        }
                    }
                    for (side, lines) in [("v4", &v4), ("v5", &v5)] {
                        assert!(
                            !lines
                                .iter()
                                .any(|(_, m, _)| m == WAL_CHECKPOINT_UNDER_TRUNCATE),
                            "WAL_CHECKPOINT_UNDER_TRUNCATE ({side}, {name})"
                        );
                    }
                    let (r4, r5) = (render(&v4), render(&v5));
                    if r4 != r5 {
                        failures.push(format!(
                            "{name} action {i} ({op}) lines\n  v4: {r4:#?}\n  v5: {r5:#?}"
                        ));
                    }
                    if result != action["result"] {
                        failures.push(format!(
                            "{name} action {i} ({op}) result: v4 {} v5 {result}",
                            action["result"]
                        ));
                    }
                }
                drop(db);

                let got = outcome(base, row, &planted, &recipe.pepper);
                let (d4, d5) = (
                    row["dbs"].as_object().unwrap(),
                    got["dbs"].as_object().unwrap(),
                );
                if d4.keys().collect::<Vec<_>>() != d5.keys().collect::<Vec<_>>() {
                    failures.push(format!("{name} dbs keys: v4 {d4:?} v5 {d5:?}"));
                }
                for (d, v4) in d4 {
                    if let Some(v5) = d5.get(d) {
                        compare_db(name, d, v4, v5, &mut failures);
                    }
                }
                for key in ["stateAfter", "backupsAfter"] {
                    if got[key] != row[key] {
                        failures.push(format!(
                            "{name} {key}:\n  v4: {}\n  v5: {}",
                            row[key], got[key]
                        ));
                    }
                }
                compared += 1;
            }
            other => panic!("row kind {other}"),
        }
    }
    eprintln!(
        "{expect_tz}: {compared} rows compared, {} failures",
        failures.len()
    );
    assert!(failures.is_empty(), "{expect_tz}:\n{}", failures.join("\n"));
}

#[test]
fn daily_db_optimize_matches_v4_under_utc() {
    let Some(rows) = load("QT_ORACLE_DAILY_DB_OPTIMIZE") else {
        return;
    };
    run_zone(&rows, "UTC");
}

#[test]
fn daily_db_optimize_matches_v4_under_chicago() {
    let Some(rows) = load("QT_ORACLE_DAILY_DB_OPTIMIZE_CHICAGO") else {
        return;
    };
    run_zone(&rows, "America/Chicago");
}
