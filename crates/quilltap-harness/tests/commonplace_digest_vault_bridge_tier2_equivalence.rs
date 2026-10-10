//! P4.D271 tier-2 differential: the Commonplace digest vault bridge (v4
//! `writeCommonplaceDigestsToVault`, `lib/file-storage/commonplace-digest-vault-
//! bridge.ts`, byte-identical at the `01a83539d` pin) against
//! `quilltap_core::services::commonplace_digest_vault_bridge`, over a COPY of
//! the consolidation fixture pair (`build-consolidation-fixture.ts`).
//!
//! The fourteen calls (`harness/oracle/fixtures/commonplace-bridge-calls.json`)
//! run in order on ONE copy on each side: first write (self + other + an empty
//! file), the unchanged re-run, one changed file, a subject named `Self`, a
//! per-file failure (the fixture's mount-index trigger plant), a missing /
//! archived / store-type-mismatch / FILESYSTEM / vaultless / dangling-vault
//! holder, the stale pre-existing file overwritten, an empty list, a list of
//! only empty files. Comparands per call: the result object and every captured
//! line (level, message, every context field in v4's order — the
//! `Document absent from database-backed store` debug with v4's `module:
//! 'service'`, and v4's `Failed to resolve character vault store` WARN with
//! its `context` key and its overlay error text). At the end: the
//! `Commonplace/` links + files + documents, folders and chunks by SQL
//! projections carrying no ids or clocks.
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout or the
//! round's pin — the CASES run from a /tmp mirror because jest ignores
//! `.claude/` paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; M=/tmp/qt-consolidation-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $V5W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-consolidation-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-consolidation-fixture.ts
//!   rm -f /tmp/qt-consolidation-bridge.ndjson
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
//!   QT_ORACLE_OUT=/tmp/qt-consolidation-bridge.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$M/cases" -- commonplace-digest-vault-bridge-tier2
//!   cd $V5W
//!   QT_ORACLE_CONSOLIDATION_BRIDGE=/tmp/qt-consolidation-bridge.ndjson \
//!   QT_FIXTURE_CONSOLIDATION_MAIN=/tmp/qt-consolidation-main.db \
//!   QT_FIXTURE_CONSOLIDATION_MOUNT=/tmp/qt-consolidation-mount.db \
//!     cargo test -p quilltap-harness --test commonplace_digest_vault_bridge_tier2_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::commonplace_file::CommonplaceDigestEntry;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::commonplace_digest_vault_bridge::{
    write_commonplace_digests_to_vault, CommonplaceMirrorFile, WriteCommonplaceDigestsInput,
};
use quilltap_core::test_support::global_capture;
use serde_json::Value;

const TARGET: &str = "quilltap_core::services::commonplace_digest_vault_bridge";
const CAPTURED: &[&str] = &[
    "Skipping Commonplace mirror: holder not found",
    "Skipping Commonplace mirror for archived character",
    "Skipping Commonplace mirror: holder has no character vault",
    "Wrote Commonplace digest mirror",
    "Failed to write a Commonplace digest mirror",
    "Commonplace mirror failed",
    "Commonplace mirror complete",
    "Document absent from database-backed store",
    "Failed to resolve character vault store",
];

/// Every 36-char UUID not in the committed spec text → `<id>`: the vault mount
/// point ids are minted by each fixture build, so a rebuilt pair (each
/// consolidation family's recipe rebuilds it) must not move a comparand.
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

fn input(v: &Value) -> WriteCommonplaceDigestsInput {
    WriteCommonplaceDigestsInput {
        holder_character_id: v["holderCharacterId"].as_str().unwrap().to_string(),
        updated_at: v["updatedAt"].as_str().unwrap().to_string(),
        files: v["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| CommonplaceMirrorFile {
                subject_character_id: f["subjectCharacterId"].as_str().unwrap().to_string(),
                subject_name: f["subjectName"].as_str().unwrap().to_string(),
                is_self: f["isSelf"].as_bool().unwrap(),
                digests: f["digests"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|d| CommonplaceDigestEntry {
                        content: d["content"].as_str().unwrap().to_string(),
                        kind: d["kind"].as_str().unwrap().to_string(),
                        occurred_at: d["occurredAt"].as_str().map(str::to_string),
                        reinforced_importance: d["reinforcedImportance"].as_f64().unwrap(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// One oracle line as `global_capture` renders a v5 event.
fn render_oracle_line(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let mut out = format!("{level} {TARGET} {}", line["message"].as_str().unwrap());
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

fn is_captured(line: &str) -> bool {
    line.split_once(' ')
        .and_then(|(_, rest)| rest.split_once(' '))
        .is_some_and(|(_, msg)| {
            CAPTURED.iter().any(|m| {
                msg.strip_prefix(m)
                    .is_some_and(|t| t.is_empty() || t.starts_with(' '))
            })
        })
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

#[tokio::test(flavor = "current_thread")]
async fn commonplace_digest_vault_bridge_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_CONSOLIDATION_BRIDGE"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MAIN"),
        std::env::var("QT_FIXTURE_CONSOLIDATION_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_CONSOLIDATION_BRIDGE + QT_FIXTURE_CONSOLIDATION_MAIN / _MOUNT (see header)."
        );
        return;
    };
    global_capture::install();
    let spec_text =
        std::fs::read_to_string(fixtures_dir().join("consolidation-tier3.json")).unwrap();
    let spec: Value = serde_json::from_str(&spec_text).unwrap();
    let calls: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("commonplace-bridge-calls.json")).unwrap(),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    let scratch = tempfile::Builder::new()
        .prefix("qt-consolidation-bridge-rust-")
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
    let calls = calls["calls"].as_array().unwrap();
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
        let (result, captured) = global_capture::capture_async(write_commonplace_digests_to_vault(
            &db,
            &input(&call["input"]),
        ))
        .await;
        if result.to_json() != want["result"] {
            failures.push(format!(
                "{name}: result v5 {} v4 {}",
                result.to_json(),
                want["result"]
            ));
        }
        let got: Vec<String> = captured
            .into_iter()
            .filter(|l| is_captured(l))
            .map(|l| mask_minted(&l, &spec_text))
            .collect();
        let expected: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| mask_minted(&render_oracle_line(l), &spec_text))
            .collect();
        lines_seen += expected.len();
        if got != expected {
            failures.push(format!("{name}: lines\n  v5 {got:#?}\n  v4 {expected:#?}"));
        }
    }
    for p in calls_projections(&fixtures_dir()) {
        let want = oracle
            .iter()
            .find(|o| o["kind"] == "projection" && o["name"] == p.0.as_str())
            .unwrap_or_else(|| panic!("oracle has no projection {}", p.0));
        let got: Value =
            serde_json::from_str(&mask_minted(&projection(&db, &p.1).to_string(), &spec_text))
                .unwrap();
        let want_rows: Value =
            serde_json::from_str(&mask_minted(&want["rows"].to_string(), &spec_text)).unwrap();
        if !cells_eq(&got, &want_rows) {
            failures.push(format!(
                "projection {}:\n  v5 {got}\n  v4 {}",
                p.0, want["rows"]
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
        "OK: commonplace digest vault bridge matched oracle ({} calls, {lines_seen} lines, 3 projections).",
        calls.len()
    );
}

fn calls_projections(dir: &Path) -> Vec<(String, String)> {
    let v: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("commonplace-bridge-calls.json")).unwrap(),
    )
    .unwrap();
    v["projections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap().to_string(),
                p["sql"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}
