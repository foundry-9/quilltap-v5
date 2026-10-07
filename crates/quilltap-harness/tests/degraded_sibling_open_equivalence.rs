//! P4.159 (dogfood #150) tier-1 differential — the DEGRADED sibling open:
//! `quilltap_core::db::runtime::Db::open` and the structural pass's degraded
//! arm against v4's REAL dedicated-database clients, integrity checks and
//! PHASE 3.1 pass (`harness/oracle/cases/degraded-sibling-open.test.ts`).
//!
//! Row kinds:
//!   - `open` — per partition (`mountIndex` / `llmLogs`) × plant (`garbage`,
//!     `sound`, `integrity`): v4's `getMountIndexSQLiteClient` /
//!     `getLLMLogsSQLiteClient` + `run*IntegrityCheck` over the planted file.
//!     The row's recipe is rebuilt byte-for-byte here (the garbage pattern; the
//!     sound build's SQL through a writable open; the integrity plant's
//!     overwritten page), the plant placed as that sibling of a sound main, and
//!     `Db::open` run under the thread-scoped capture. Compared: every
//!     `module=database:*` line — count, order, level, message, keys IN ORDER,
//!     values (the `error` bytes and the `quick_check` `result` verbatim; the
//!     plant's path normalized to `<path>`; v4's root `service` /
//!     `environment` keys dropped — v5 renders no root context) — and the
//!     partition's state (`Degraded` iff v4's `is*Degraded()`).
//!   - `pass` — v4's REAL `verifyStructuralTables` over the REAL container
//!     with one or both dedicated partitions degraded the way the client
//!     leaves them (the other fresh): the problems in order and every line,
//!     replayed through v5's `verify_structural_tables` with
//!     `TableRead::PartitionDegraded` for the degraded partition(s) and a
//!     sound read for the rest. The target token is not compared (v4 has
//!     none; v5's pass logs at `quilltap::boot`, the DEBUG at `quilltap::db`).
//!
//! Red-first (P4.159's lane record): on unported `main` this binary did not
//! compile (no `PartitionState`, no `partition_state`, no
//! `TableRead::PartitionDegraded`); with those names stubbed to today's
//! behaviour, every garbage row ERRORED in `Db::open` where v4 answers
//! degraded, and every pass row recorded ZERO problems.
//!
//! Generate the oracle (Node 24, from the v4 checkout — or the pinned
//! worktree; STAGE the case outside `.claude/`, v4's jest ignores it):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-oracle-stage-degraded-sibling-open
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases
//!   cp $V5W/harness/oracle/cases/degraded-sibling-open.test.ts $STAGE/harness/oracle/cases/
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-degraded-sibling-open.ndjson \
//!     PATH=$N:$PATH npx jest --silent --watchman=false --testTimeout=240000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "degraded-sibling-open\.test\.ts$"
//! Run:
//!   QT_ORACLE_DEGRADED_SIBLING_OPEN=/tmp/oracle-degraded-sibling-open.ndjson \
//!     cargo test -p quilltap-harness --test degraded_sibling_open_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated
//! discipline.

use std::path::Path;

use quilltap_core::db::runtime::{Db, DbPaths, PartitionState};
use quilltap_core::db::table_shape::{
    verify_structural_tables, EnsureFailures, Partition, TableRead,
};
use quilltap_core::db::Writer;
use quilltap_core::test_support::captured_with;
use serde_json::Value;

/// Any valid base64 keys a fresh encrypted file; the bytes the comparison
/// reads (SQLite's messages, `quick_check`'s result) do not depend on it.
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn rows() -> Option<Vec<Value>> {
    let Ok(path) = std::env::var("QT_ORACLE_DEGRADED_SIBLING_OPEN") else {
        eprintln!("SKIP: QT_ORACLE_DEGRADED_SIBLING_OPEN unset");
        return None;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    // 6 open rows + 3 pass rows: a truncated regen must not pass vacuously.
    assert_eq!(rows.len(), 9, "oracle row count");
    Some(rows)
}

fn of_kind<'a>(rows: &'a [Value], kind: &str) -> Vec<&'a Value> {
    rows.iter().filter(|r| r["kind"] == kind).collect()
}

/// A v4 field value as the capture renders it: strings bare, numbers and
/// booleans by their JSON text.
fn render_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// v4's line as `LEVEL message k=v …` (the root `service` / `environment`
/// keys dropped).
fn v4_line(line: &Value) -> String {
    let mut out = format!(
        "{} {}",
        line["level"].as_str().unwrap().to_uppercase(),
        line["message"].as_str().unwrap()
    );
    for (k, v) in line["fields"].as_object().unwrap() {
        if k == "service" || k == "environment" {
            continue;
        }
        out.push_str(&format!(" {k}={}", render_value(v)));
    }
    out
}

/// v5's captured line as `LEVEL message k=v …` (the target token dropped).
fn v5_line(line: &str) -> String {
    let (level, rest) = line.split_once(' ').unwrap();
    let (_target, rest) = rest.split_once(' ').unwrap();
    format!("{level} {rest}")
}

fn garbage(recipe: &Value) -> Vec<u8> {
    let g = &recipe["garbage"];
    let (len, mul, add) = (
        g["len"].as_u64().unwrap() as usize,
        g["mul"].as_u64().unwrap() as usize,
        g["add"].as_u64().unwrap() as usize,
    );
    (0..len).map(|i| ((i * mul + add) & 255) as u8).collect()
}

/// Rebuild the row's plant at `path`.
fn plant(path: &Path, plant: &str, recipe: &Value) {
    if plant == "garbage" {
        std::fs::write(path, garbage(recipe)).unwrap();
        return;
    }
    {
        let w = Writer::open_writable(path, PEPPER).unwrap();
        for sql in recipe["soundSql"].as_array().unwrap() {
            w.connection().execute_batch(sql.as_str().unwrap()).unwrap();
        }
    }
    if plant == "integrity" {
        let page = recipe["page"].as_u64().unwrap() as usize;
        let size = recipe["pageSize"].as_u64().unwrap() as usize;
        let g = garbage(recipe);
        let mut bytes = std::fs::read(path).unwrap();
        let off = (page - 1) * size;
        for i in 0..size {
            bytes[off + i] = g[i % g.len()];
        }
        std::fs::write(path, bytes).unwrap();
    }
}

#[test]
fn every_sibling_open_logs_and_degrades_as_v4s_client() {
    let Some(rows) = rows() else { return };
    let opens = of_kind(&rows, "open");
    assert_eq!(opens.len(), 6, "two partitions x three plants");
    let mut failures = Vec::new();
    for r in &opens {
        let (partition, plant_name) = (
            r["partition"].as_str().unwrap(),
            r["plant"].as_str().unwrap(),
        );
        let label = format!("{partition}/{plant_name}");
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("quilltap.db");
        Writer::open_writable(&main, PEPPER).unwrap();
        let sibling = dir.path().join(format!("{partition}.db"));
        plant(&sibling, plant_name, &r["recipe"]);
        let (paths, which) = match partition {
            "mountIndex" => (
                DbPaths {
                    main,
                    mount_index: Some(sibling.clone()),
                    llm_logs: None,
                },
                Partition::MountIndex,
            ),
            "llmLogs" => (
                DbPaths {
                    main,
                    mount_index: None,
                    llm_logs: Some(sibling.clone()),
                },
                Partition::LlmLogs,
            ),
            other => panic!("unknown partition {other}"),
        };
        let started = std::time::Instant::now();
        let (db, lines) = captured_with(|| Db::open(paths, PEPPER));
        let elapsed = started.elapsed();
        let db = match db {
            Ok(db) => db,
            Err(e) => {
                failures.push(format!(
                    "{label}: Db::open ERRORED where v4 answers degraded: {e}"
                ));
                continue;
            }
        };
        let token = sibling.display().to_string();
        let got: Vec<String> = lines
            .iter()
            .filter(|l| l.contains(" module=database:"))
            .map(|l| v5_line(&l.replace(&token, "<path>")))
            .collect();
        let want: Vec<String> = r["lines"].as_array().unwrap().iter().map(v4_line).collect();
        if got != want {
            failures.push(format!("{label}: lines\n  v5 {got:#?}\n  v4 {want:#?}"));
        }
        let degraded = db.partition_state(which) == PartitionState::Degraded;
        if degraded != r["degraded"].as_bool().unwrap() {
            failures.push(format!(
                "{label}: degraded v5 {degraded} != v4 {}",
                r["degraded"]
            ));
        }
        // R-A: a degraded partition holds neither a read pool nor a writer.
        let readable = match which {
            Partition::MountIndex => db.read_mount_index(|_| Ok(())).is_ok(),
            _ => db.read_llm_logs(|_| Ok(())).is_ok(),
        };
        if readable == degraded {
            failures.push(format!(
                "{label}: readable={readable} but degraded={degraded}"
            ));
        }
        let writable = db
            .write_blocking(move |ws| {
                Ok::<_, quilltap_core::db::DbError>(match which {
                    Partition::MountIndex => ws.mount_index().is_some(),
                    _ => ws.llm_logs().is_some(),
                })
            })
            .unwrap();
        if writable == degraded {
            failures.push(format!(
                "{label}: writer present={writable} but degraded={degraded}"
            ));
        }
        eprintln!("{label}: {} lines, {elapsed:?}", got.len());
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_structural_pass_over_a_degraded_partition_matches_v4s() {
    let Some(rows) = rows() else { return };
    let passes = of_kind(&rows, "pass");
    assert_eq!(passes.len(), 3, "mount index, LLM logs, both");
    let mut failures = Vec::new();
    for r in &passes {
        let degraded: Vec<&str> = r["degraded"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let is_degraded = |p: Partition| degraded.contains(&p.db_target());
        let (problems, lines) = captured_with(|| {
            verify_structural_tables(&EnsureFailures::default(), |t| {
                if is_degraded(t.partition) {
                    TableRead::PartitionDegraded
                } else {
                    TableRead::Read(Ok(None))
                }
            })
        });
        let got: Vec<String> = problems.into_iter().map(|p| p.problem).collect();
        let want: Vec<String> = r["problems"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect();
        if got != want {
            failures.push(format!(
                "{degraded:?}: problems\n  v5 {got:#?}\n  v4 {want:#?}"
            ));
        }
        let got_lines: Vec<String> = lines.iter().map(|l| v5_line(l)).collect();
        let want_lines: Vec<String> = r["lines"].as_array().unwrap().iter().map(v4_line).collect();
        // v4's sound-pass DEBUG (`Structural tables verified`) never fires on
        // a degraded pass, so every v5 line must be one of v4's, in order.
        if got_lines != want_lines {
            failures.push(format!(
                "{degraded:?}: lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
