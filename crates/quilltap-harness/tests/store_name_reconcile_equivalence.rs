//! P4.D274 unit 4 — the headline tier-2 differential: v4's REAL
//! `reconcileStoreNames` (`lib/mount-index/reconcile-store-names.ts`,
//! `ed8b15b50`, bug 186) against `quilltap_core::db::store_name_reconcile`,
//! scenario by scenario of `harness/oracle/fixtures/store-name-reconcile-
//! spec.json`: the BEFORE / AFTER `doc_mount_points` dumps (`updatedAt` →
//! `<now>` iff it moved), the result object (or the error text), and the
//! captured lines in order (level, message, fields in v4's key order).
//!
//! **The ping-pong proof (cross-run).** On an instance v4 and v5 share, the two
//! must plan the SAME renames or each boot undoes the other. So:
//!   - stage 1 also runs v5's reconcile over each v4 AFTER pair
//!     (`QT_SNR_V4_AFTER_DIR`) and asserts it renames NOTHING; and writes v5's
//!     own AFTER pairs to `QT_SNR_V5_AFTER_DIR`;
//!   - stage 2 (`QT_ORACLE_STORE_NAME_RECONCILE_CROSS`) reads v4's reconcile
//!     over those v5 pairs and asserts `renamed: []` for every scenario —
//!     except the `RECONCILE_READ_FAILURE_STRICT` rows, where v5 skipped and v4
//!     replays exactly its own stage-1 plan (v5 lags v4; it never undoes it).
//!
//! **`RECONCILE_READ_FAILURE_STRICT`** (R-A arm 2), both ways in
//! [`READ_FAILURE_STRICT`]: when a whole-query read fails, v4's `_findAll`
//! answers `[]` — a failed characters read RETIRES every vault-shaped
//! `character` store — while v5 logs v4's same ERROR, then its own skip WARN,
//! answers `ReadFailed` and writes nothing. A v4 convergence trips the table.
//!
//! Regenerate + run (the pinned v4 worktree, Node 24, TZ=UTC; four stages):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-store-name-reconcile ; rm -f /tmp/oracle-store-name-reconcile.ndjson /tmp/oracle-store-name-reconcile-cross.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile $N/npx tsx $V5W/harness/oracle/fixtures/build-store-name-reconcile-fixture.ts
//!   TZ=UTC QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_OUT_DIR=/tmp/qt-store-name-reconcile/v4-after $N/npx tsx $V5W/harness/oracle/cases/store-name-reconcile.ts > /tmp/oracle-store-name-reconcile.ndjson
//!   cd $V5W
//!   QT_ORACLE_STORE_NAME_RECONCILE=/tmp/oracle-store-name-reconcile.ndjson QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_V4_AFTER_DIR=/tmp/qt-store-name-reconcile/v4-after QT_SNR_V5_AFTER_DIR=/tmp/qt-store-name-reconcile/v5-after cargo test -p quilltap-harness --test store_name_reconcile_equivalence -- --nocapture
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_CROSS_DIR=/tmp/qt-store-name-reconcile/v5-after $N/npx tsx $V5W/harness/oracle/cases/store-name-reconcile.ts > /tmp/oracle-store-name-reconcile-cross.ndjson
//!   cd $V5W
//!   QT_ORACLE_STORE_NAME_RECONCILE=/tmp/oracle-store-name-reconcile.ndjson QT_ORACLE_STORE_NAME_RECONCILE_CROSS=/tmp/oracle-store-name-reconcile-cross.ndjson QT_FIXTURE_SNR_DIR=/tmp/qt-store-name-reconcile QT_SNR_V4_AFTER_DIR=/tmp/qt-store-name-reconcile/v4-after QT_SNR_V5_AFTER_DIR=/tmp/qt-store-name-reconcile/v5-after cargo test -p quilltap-harness --test store_name_reconcile_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::store_name_reconcile::{reconcile_store_names, StoreNameReconcileResult};
use quilltap_core::db::Writer;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    pepper: String,
    #[serde(rename = "userId")]
    user_id: String,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    trigger: String,
    plant: Option<String>,
    stores: Vec<StoreRow>,
    characters: Vec<CharRow>,
}

#[derive(Deserialize)]
struct StoreRow {
    id: String,
    name: String,
    #[serde(rename = "storeType")]
    store_type: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
}

#[derive(Deserialize)]
struct CharRow {
    id: String,
    name: String,
    #[serde(rename = "characterDocumentMountPointId")]
    vault: Option<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "archivedAt")]
    archived_at: Option<String>,
}

/// `RECONCILE_READ_FAILURE_STRICT`, both ways: `(scenario, v4's rename count,
/// v4's first line, v5's lines)`. v4 answers the failed read with `[]` and
/// plans on; v5 logs v4's ERROR and skips.
const READ_FAILURE_STRICT: &[(&str, usize, &str, &[&str])] = &[
    (
        "e2-characters-read-fails",
        2,
        "ERROR Error finding all entities collection=characters error=malformed JSON",
        &[
            "ERROR Error finding all entities collection=characters error=malformed JSON",
            "WARN Store-name reconcile skipped: a store or character read failed module=mount-index:reconcile-store-names trigger=boot collection=characters error=malformed JSON",
        ],
    ),
    (
        "e2-stores-read-fails",
        0,
        "ERROR Error finding all entities collection=doc_mount_points error=malformed JSON",
        &[
            "ERROR Error finding all entities collection=doc_mount_points error=malformed JSON",
            "WARN Store-name reconcile skipped: a store or character read failed module=mount-index:reconcile-store-names trigger=boot collection=doc_mount_points error=malformed JSON",
        ],
    ),
];

fn spec() -> Spec {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/store-name-reconcile-spec.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("read spec")).expect("parse spec")
}

/// v4's captured line → `LEVEL message k=v …` (key order kept).
fn render_v4_line(l: &Value) -> String {
    let mut out = format!(
        "{} {}",
        l["level"].as_str().unwrap().to_uppercase(),
        l["message"].as_str().unwrap()
    );
    for (k, v) in l["fields"].as_object().unwrap() {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push_str(&format!(" {k}={rendered}"));
    }
    out
}

/// v5's captured line with the tracing target dropped.
fn drop_target(line: &str) -> String {
    let (level, rest) = line.split_once(' ').unwrap();
    let (_target, rest) = rest.split_once(' ').unwrap();
    format!("{level} {rest}")
}

fn dump_stores(mount: &Connection, planted: &HashMap<String, String>) -> Value {
    let mut stmt = mount
        .prepare(
            "SELECT id, name, storeType, createdAt, updatedAt FROM doc_mount_points ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            let id: String = r.get(0)?;
            let updated: String = r.get(4)?;
            let moved = planted.get(&id) != Some(&updated);
            Ok(json!({
                "id": id,
                "name": r.get::<_, String>(1)?,
                "storeType": r.get::<_, Option<String>>(2)?,
                "createdAt": r.get::<_, String>(3)?,
                "updatedAt": if moved { json!("<now>") } else { json!(updated) },
            }))
        })
        .unwrap();
    Value::Array(rows.map(|r| r.unwrap()).collect())
}

fn result_json(r: &StoreNameReconcileResult) -> Value {
    json!({
        "renamed": r.renamed.iter().map(|x| x.to_json()).collect::<Vec<_>>(),
        "skippedReason": r.skipped_reason.map(|s| s.as_str()),
    })
}

struct Run {
    before: Value,
    after: Value,
    result: Value,
    error: Value,
    lines: Vec<String>,
}

/// Copy the pair under `work`, plant the scenario when `plant`, reconcile on
/// the caller thread (so `captured_with` sees every line), and dump.
fn run_pair(
    spec: &Spec,
    sc: &Scenario,
    main_src: &Path,
    mount_src: &Path,
    work: &Path,
    plant: bool,
) -> (Run, PathBuf, PathBuf) {
    std::fs::create_dir_all(work).unwrap();
    let main_path = work.join(format!("{}-main.db", sc.name));
    let mount_path = work.join(format!("{}-mount.db", sc.name));
    std::fs::copy(main_src, &main_path).unwrap();
    std::fs::copy(mount_src, &mount_path).unwrap();
    let degraded = sc.plant.as_deref() == Some("mount-index-degraded");

    let main = Writer::open_writable(&main_path, &spec.pepper).expect("open main");
    let mount =
        (!degraded).then(|| Writer::open_writable(&mount_path, &spec.pepper).expect("open mount"));
    let mut planted = HashMap::new();
    if plant {
        if let Some(m) = &mount {
            for s in &sc.stores {
                m.connection()
                    .execute(
                        "INSERT INTO doc_mount_points (id, name, storeType, createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?4)",
                        params![s.id, s.name, s.store_type, s.created_at],
                    )
                    .unwrap();
            }
        }
        for c in &sc.characters {
            main.connection()
                .execute(
                    "INSERT INTO characters (id, userId, name, characterDocumentMountPointId, archivedAt, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                    params![c.id, spec.user_id, c.name, c.vault, c.archived_at, c.created_at],
                )
                .unwrap();
        }
        let boom = "ADD COLUMN qt_boom TEXT GENERATED ALWAYS AS (json('not json')) VIRTUAL";
        match sc.plant.as_deref() {
            Some("characters-read-fails") => main
                .connection()
                .execute_batch(&format!("ALTER TABLE characters {boom}"))
                .unwrap(),
            Some("stores-read-fails") => mount
                .as_ref()
                .unwrap()
                .connection()
                .execute_batch(&format!("ALTER TABLE doc_mount_points {boom}"))
                .unwrap(),
            _ => {}
        }
    }
    if let Some(m) = &mount {
        let mut stmt = m
            .connection()
            .prepare("SELECT id, updatedAt FROM doc_mount_points")
            .unwrap();
        for r in stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
        {
            let (id, u) = r.unwrap();
            planted.insert(id, u);
        }
    }

    let before = mount
        .as_ref()
        .map(|m| dump_stores(m.connection(), &planted))
        .unwrap_or(json!([]));
    let (outcome, lines) = quilltap_core::test_support::captured_with(|| {
        reconcile_store_names(
            main.connection(),
            mount.as_ref().map(|m| m.connection()),
            &sc.trigger,
        )
    });
    let after = mount
        .as_ref()
        .map(|m| dump_stores(m.connection(), &planted))
        .unwrap_or(json!([]));
    let (result, error) = match outcome {
        Ok(r) => (result_json(&r), Value::Null),
        Err(e) => (
            Value::Null,
            json!(quilltap_core::db::fallback::error_text(&e)),
        ),
    };
    drop(main);
    drop(mount);
    (
        Run {
            before,
            after,
            result,
            error,
            lines: lines.iter().map(|l| drop_target(l)).collect(),
        },
        main_path,
        mount_path,
    )
}

fn oracle_lines(path: &str) -> HashMap<String, Value> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    assert!(!text.trim().is_empty(), "the oracle file {path} is EMPTY");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("oracle line");
            (v["scenario"].as_str().unwrap().to_string(), v)
        })
        .collect()
}

#[test]
fn store_name_reconcile_matches_oracle() {
    let (Ok(oracle_path), Ok(base_dir)) = (
        std::env::var("QT_ORACLE_STORE_NAME_RECONCILE"),
        std::env::var("QT_FIXTURE_SNR_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_STORE_NAME_RECONCILE and QT_FIXTURE_SNR_DIR (see header).");
        return;
    };
    let spec = spec();
    let oracle = oracle_lines(&oracle_path);
    let base = PathBuf::from(&base_dir);
    let work = tempfile::Builder::new()
        .prefix("qt-snr-rust-")
        .tempdir()
        .unwrap();
    let v4_after = std::env::var("QT_SNR_V4_AFTER_DIR").ok().map(PathBuf::from);
    let v5_after = std::env::var("QT_SNR_V5_AFTER_DIR").ok().map(PathBuf::from);
    if let Some(d) = &v5_after {
        std::fs::create_dir_all(d).unwrap();
    }

    let mut failures = Vec::new();
    let mut checked = 0usize;
    let mut cross_checked = 0usize;
    for sc in &spec.scenarios {
        let want = oracle
            .get(&sc.name)
            .unwrap_or_else(|| panic!("no oracle line for {}", sc.name));
        let (run, main_after, mount_after) = run_pair(
            &spec,
            sc,
            &base.join("base-main.db"),
            &base.join("base-mount.db"),
            &work.path().join("stage1"),
            true,
        );
        checked += 1;
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4_line)
            .collect();

        if let Some((_, v4_renames, v4_first, v5_lines)) =
            READ_FAILURE_STRICT.iter().find(|(n, ..)| *n == sc.name)
        {
            // v4's side, as recorded.
            assert_eq!(
                want["result"]["renamed"].as_array().map(Vec::len),
                Some(*v4_renames),
                "RECONCILE_READ_FAILURE_STRICT {}: v4's rename count moved — v4 converged? re-measure",
                sc.name
            );
            assert_eq!(
                want_lines.first().map(String::as_str),
                Some(*v4_first),
                "{}",
                sc.name
            );
            // v5's side: v4's ERROR, the skip WARN, nothing written.
            assert_eq!(run.lines, *v5_lines, "{}: v5 lines", sc.name);
            assert_eq!(
                run.result,
                json!({"renamed": [], "skippedReason": "read-failed"}),
                "{}",
                sc.name
            );
            assert_eq!(
                run.before, run.after,
                "{}: v5 wrote on a failed read",
                sc.name
            );
        } else {
            for (label, got, exp) in [
                ("before", &run.before, &want["before"]),
                ("after", &run.after, &want["after"]),
                ("result", &run.result, &want["result"]),
                ("error", &run.error, &want["error"]),
            ] {
                if serde_json::to_string(got).unwrap() != serde_json::to_string(exp).unwrap() {
                    failures.push(format!("{} {label}:\n  v5 {got}\n  v4 {exp}", sc.name));
                }
            }
            if run.lines != want_lines {
                failures.push(format!(
                    "{} lines:\n  v5 {:#?}\n  v4 {:#?}",
                    sc.name, run.lines, want_lines
                ));
            }
        }

        if let Some(d) = &v5_after {
            if sc.plant.as_deref() != Some("mount-index-degraded") {
                std::fs::copy(&main_after, d.join(format!("{}-main.db", sc.name))).unwrap();
                std::fs::copy(&mount_after, d.join(format!("{}-mount.db", sc.name))).unwrap();
            }
        }

        // Cross-run, v4 → v5: v5 over v4's reconciled pair renames nothing.
        if let Some(d) = &v4_after {
            let m = d.join(format!("{}-main.db", sc.name));
            if m.exists() {
                let (cross, ..) = run_pair(
                    &spec,
                    sc,
                    &m,
                    &d.join(format!("{}-mount.db", sc.name)),
                    &work.path().join("cross"),
                    false,
                );
                cross_checked += 1;
                let renamed = cross.result["renamed"]
                    .as_array()
                    .map(Vec::len)
                    .unwrap_or(0);
                if renamed != 0 || cross.before != cross.after {
                    failures.push(format!(
                        "{} cross v4→v5: v5 renamed {renamed} on v4's reconciled pair: {}",
                        sc.name, cross.result
                    ));
                }
            }
        }
    }

    // Stage 2, v5 → v4: v4 over v5's reconciled pairs renames nothing.
    let mut v4_cross = 0usize;
    if let Ok(cross_path) = std::env::var("QT_ORACLE_STORE_NAME_RECONCILE_CROSS") {
        for (name, v) in oracle_lines(&cross_path) {
            v4_cross += 1;
            let renamed = v["result"]["renamed"].as_array().map(Vec::len).unwrap_or(0);
            if READ_FAILURE_STRICT.iter().any(|(n, ..)| *n == name) {
                // v5 skipped where v4 planned, so v4 makes exactly its own
                // stage-1 renames over v5's (untouched) pair: v5 LAGS v4, it
                // never undoes v4's names — the divergence's safety argument.
                assert_eq!(
                    v["result"]["renamed"], oracle[&name]["result"]["renamed"],
                    "RECONCILE_READ_FAILURE_STRICT {name}: v4 over v5's pair must replay its own plan"
                );
                continue;
            }
            if renamed != 0 || v["before"] != v["after"] {
                failures.push(format!(
                    "{name} cross v5→v4: v4 renamed {renamed}: {}",
                    v["result"]
                ));
            }
        }
        assert!(v4_cross > 0, "the cross oracle is empty");
    }

    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(checked, 11, "scenario count");
    eprintln!(
        "OK: store-name reconcile matched oracle ({checked} scenarios; {} RECONCILE_READ_FAILURE_STRICT both ways; cross v4→v5 {cross_checked}, v5→v4 {v4_cross}).",
        READ_FAILURE_STRICT.len()
    );
}
