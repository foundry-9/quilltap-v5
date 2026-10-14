//! P4.D288 (C1-7, L-3) tier-2 differential: the Commonplace mirror written
//! from committed digests — v4 `mirrorCommonplaceForSubjects`
//! (`lib/memory/commonplace-mirror.ts:111-200`) with bug 208's emptied-bucket
//! removal in the bridge, at the `83334b9a5` pin — against
//! `quilltap_core::services::commonplace_mirror::mirror_commonplace_for_subjects`,
//! over a COPY of the consolidation fixture pair (`build-consolidation-
//! fixture.ts`).
//!
//! The calls (`harness/oracle/fixtures/commonplace-mirror-calls.json`) run in
//! order on ONE copy on each side: planted digests mirrored (a subject listed
//! twice; self + other), the unchanged re-run, an empty subject list, a
//! subject that no longer exists, a missing holder, a bucket emptied (its file
//! REMOVED) and emptied again (`skipped`), the 200-digest read cap over 205
//! planted digests, and two retirements whose announcement
//! (`memory_retire::announce_retirement`, after the commit — R-AF) refreshes
//! the removed digest's bucket, the second removing the file. Comparands per
//! call: the answer and every captured line (level, message, every context
//! field in v4's order; v5's target stripped — the lines come from three
//! modules). At the end: the `Commonplace/` links + files + documents,
//! folders and chunks by the bridge family's SQL projections.
//!
//! Generate the fixture + oracle output (Node 24, from the round's pin — the
//! CASES run from a /tmp mirror because jest ignores `.claude/` paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
//!   rm -f /tmp/qt-commonplace-mirror.ndjson
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
//!   QT_ORACLE_OUT=/tmp/qt-commonplace-mirror.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- commonplace-mirror-tier2
//!   cd $V5W
//!   QT_ORACLE_COMMONPLACE_MIRROR=/tmp/qt-commonplace-mirror.ndjson \
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
//!     cargo test -p quilltap-harness --test commonplace_mirror_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::commonplace_mirror::mirror_commonplace_for_subjects;
use quilltap_core::services::memory_retire::{retire_and_announce, RetireOptions, RetireReason};
use quilltap_core::test_support::global_capture;
use serde_json::Value;

const CAPTURED: &[&str] = &[
    "Commonplace mirror skipped: holder not found",
    "Commonplace mirror skips a bucket whose subject no longer exists",
    "Commonplace mirror written from committed digests",
    "Commonplace mirror failed (the digests are committed regardless)",
    "Refreshed Commonplace mirror after retiring a digest",
    "Skipping Commonplace mirror: holder not found",
    "Skipping Commonplace mirror for archived character",
    "Skipping Commonplace mirror: holder has no character vault",
    "Wrote Commonplace digest mirror",
    "Failed to write a Commonplace digest mirror",
    "Removed Commonplace mirror for an emptied bucket",
    "Failed to remove an emptied Commonplace mirror",
    "Commonplace mirror failed",
    "Commonplace mirror complete",
    "Document absent from database-backed store",
    "Failed to resolve character vault store",
];

/// Every 36-char UUID not in the committed spec texts → `<id>` (the vault
/// mount point ids are minted by each fixture build).
fn mask_minted(s: &str, known: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        if i + 36 <= s.len() && s.is_char_boundary(i) && s.is_char_boundary(i + 36) {
            let cand = &s[i..i + 36];
            let is_uuid = cand.bytes().enumerate().all(|(j, c)| match j {
                8 | 13 | 18 | 23 => c == b'-',
                _ => c.is_ascii_hexdigit(),
            });
            if is_uuid {
                out.push_str(if known.contains(cand) { cand } else { "<id>" });
                i += 36;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

/// One oracle line as `LEVEL message k=v …`.
fn render_oracle_line(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let mut out = format!("{level} {}", line["message"].as_str().unwrap());
    if let Some(ctx) = line["context"].as_object() {
        for (k, v) in ctx {
            let rendered = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push_str(&format!(" {k}={rendered}"));
        }
    }
    out
}

/// A v5 line minus its target, when its message is one of [`CAPTURED`].
fn captured_line(line: &str) -> Option<String> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, msg) = rest.split_once(' ')?;
    CAPTURED
        .iter()
        .any(|m| {
            msg.strip_prefix(m)
                .is_some_and(|t| t.is_empty() || t.starts_with(' '))
        })
        .then(|| format!("{level} {msg}"))
}

/// Rows compared cell by cell; a REAL v5 reads as `37.0` where v4's
/// better-sqlite3 hands back `37`.
fn cells_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| cells_eq(p, q))
        }
        _ => a == b,
    }
}

fn projection(db: &Db, sql: &str) -> Value {
    let sql = sql.to_string();
    db.read_mount_index(move |conn| {
        let mut stmt = conn.prepare(&sql)?;
        let n = stmt.column_count();
        let rows = stmt.query_map([], |row| {
            let mut cells = Vec::with_capacity(n);
            for i in 0..n {
                let v: rusqlite::types::Value = row.get(i)?;
                cells.push(match v {
                    rusqlite::types::Value::Null => Value::Null,
                    rusqlite::types::Value::Integer(i) => Value::from(i),
                    rusqlite::types::Value::Real(f) => Value::from(f),
                    rusqlite::types::Value::Text(s) => Value::from(s),
                    rusqlite::types::Value::Blob(b) => Value::from(format!("blob:{}", b.len())),
                });
            }
            Ok(Value::Array(cells))
        })?;
        Ok(Value::Array(rows.collect::<Result<Vec<_>, _>>()?))
    })
    .unwrap_or_else(|e| panic!("projection {e}"))
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn commonplace_mirror_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_COMMONPLACE_MIRROR"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MAIN"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_COMMONPLACE_MIRROR + QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT (see header)."
        );
        return;
    };
    global_capture::install();
    let spec_text =
        std::fs::read_to_string(fixtures_dir().join("consolidation-tier3.json")).unwrap();
    let calls_text =
        std::fs::read_to_string(fixtures_dir().join("commonplace-mirror-calls.json")).unwrap();
    let known = format!("{spec_text}\n{calls_text}");
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let calls_spec: Value = serde_json::from_str(&calls_text).unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    let scratch = tempfile::Builder::new()
        .prefix("qt-commonplace-mirror-rust-")
        .tempdir()
        .unwrap();
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(&fixture_main, &main).unwrap();
    std::fs::copy(&fixture_mount, &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        spec["testPepperBase64"].as_str().unwrap(),
    )
    .expect("open the fixture copies");

    let oracle_calls: Vec<&Value> = oracle.iter().filter(|o| o["kind"] == "call").collect();
    let calls = calls_spec["calls"].as_array().unwrap();
    assert_eq!(
        calls.len(),
        oracle_calls.len(),
        "call count — regenerate the oracle"
    );
    let mut failures = Vec::new();
    let mut lines_seen = 0usize;
    for (call, want) in calls.iter().zip(oracle_calls) {
        let name = call["name"].as_str().unwrap();
        assert_eq!(want["name"], name, "call order");
        let (result, captured) = global_capture::capture_async(async {
            match call["kind"].as_str().unwrap() {
                "sql" => {
                    let statements = strings(&call["sql"]);
                    db.write(move |w| {
                        for sql in &statements {
                            w.main().connection().execute_batch(sql)?;
                        }
                        Ok(())
                    })
                    .await
                    .expect("plant");
                    Value::Null
                }
                "mirror" => Value::from(
                    mirror_commonplace_for_subjects(
                        &db,
                        call["holderId"].as_str().unwrap(),
                        &strings(&call["subjects"]),
                        call["updatedAt"].as_str(),
                    )
                    .await,
                ),
                "retire" => Value::from(
                    retire_and_announce(
                        &db,
                        strings(&call["ids"]),
                        RetireOptions::new(RetireReason::ItemDelete),
                    )
                    .await
                    .expect("retire")
                    .retired,
                ),
                other => panic!("unknown call kind {other}"),
            }
        })
        .await;
        if result != want["result"] {
            failures.push(format!("{name}: result v5 {result} v4 {}", want["result"]));
        }
        let got: Vec<String> = captured
            .iter()
            .filter_map(|l| captured_line(l))
            .map(|l| mask_minted(&l, &known))
            .collect();
        let expected: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| mask_minted(&render_oracle_line(l), &known))
            .collect();
        lines_seen += expected.len();
        if got != expected {
            failures.push(format!("{name}: lines\n  v5 {got:#?}\n  v4 {expected:#?}"));
        }
    }
    for p in calls_spec["projections"].as_array().unwrap() {
        let (pname, sql) = (p["name"].as_str().unwrap(), p["sql"].as_str().unwrap());
        let want = oracle
            .iter()
            .find(|o| o["kind"] == "projection" && o["name"] == pname)
            .unwrap_or_else(|| panic!("oracle has no projection {pname}"));
        let got: Value =
            serde_json::from_str(&mask_minted(&projection(&db, sql).to_string(), &known)).unwrap();
        let want_rows: Value =
            serde_json::from_str(&mask_minted(&want["rows"].to_string(), &known)).unwrap();
        if !cells_eq(&got, &want_rows) {
            failures.push(format!(
                "projection {pname}:\n  v5 {got}\n  v4 {}",
                want["rows"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(lines_seen >= 25, "captured lines compared: {lines_seen}");
    eprintln!(
        "OK: commonplace mirror matched oracle ({} calls, {lines_seen} lines, 3 projections).",
        calls.len()
    );
}
