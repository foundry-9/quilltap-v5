//! Tier-2 differential test: the `doc_mount_points` repo — the **widest
//! mount-index sibling-DB repo** of Phase 2 (enums, a boolean, two JSON arrays,
//! three REAL-int counters, nullable strings/timestamp).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture, which is the
//! mount-index sibling DB (`quilltap-mount-index.db`), not the main DB. The Rust
//! `Writer` is partition-agnostic — `open_writable` opens that file by path
//! exactly as it opens a main DB — so this test is shaped identically to the
//! main-DB tier-2 tests; only the fixture differs. Both run the SAME
//! create + update + delete op sequence from the committed spec, dump the
//! `doc_mount_points` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides → zero normalization.
//!
//! P4.D274 (v4 `ed8b15b50`, bug 186): the oracle's SECOND line is the name
//! chokepoint — `fixtures/doc-mount-points-name-gate.json`'s steps through the
//! same repository on the same working copy, after the committed ops. Per step
//! the error text, the captured lines (the gate's WARN; the Zod drop lines of
//! a non-uuid planted holder, which is NOT a holder, so its namesake's INSERT
//! hits the UNIQUE index instead) and every `(id, name)` must match v4's. v4's
//! table init ensures the name index before any op; the Rust side mirrors it
//! with `ensure_mount_point_name_unique_index` on the copy.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-dmp-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-doc-mount-points-fixture.ts
//!   QT_FIXTURE_DOC_MOUNT_POINTS=/tmp/qt-dmp-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/doc-mount-points-tier2.ts \
//!     > /tmp/oracle-dmp.ndjson
//! Run:
//!   QT_ORACLE_DOC_MOUNT_POINTS=/tmp/oracle-dmp.ndjson \
//!   QT_FIXTURE_DOC_MOUNT_POINTS=/tmp/qt-dmp-fixture.db \
//!     cargo test -p quilltap-harness --test doc_mount_points_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_points::{CreateOptions, DmpCreate, DmpUpdate};
use quilltap_core::db::mount_index_case_repair::ensure_mount_point_name_unique_index;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The committed fixture spec — the single source driving both ports.
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        data: CreateData,
        options: CreateOpts,
    },
    #[serde(rename = "update")]
    Update { id: String, data: UpdateData },
    #[serde(rename = "delete")]
    Delete { id: String },
}

#[derive(Deserialize)]
struct CreateData {
    name: String,
    #[serde(rename = "basePath")]
    base_path: String,
    #[serde(rename = "mountType")]
    mount_type: String,
    #[serde(rename = "storeType")]
    store_type: String,
    #[serde(rename = "includePatterns")]
    include_patterns: Vec<String>,
    #[serde(rename = "excludePatterns")]
    exclude_patterns: Vec<String>,
    enabled: bool,
    #[serde(rename = "lastScannedAt")]
    last_scanned_at: Option<String>,
    #[serde(rename = "scanStatus")]
    scan_status: String,
    #[serde(rename = "lastScanError")]
    last_scan_error: Option<String>,
    #[serde(rename = "conversionStatus")]
    conversion_status: String,
    #[serde(rename = "conversionError")]
    conversion_error: Option<String>,
    #[serde(rename = "fileCount")]
    file_count: f64,
    #[serde(rename = "chunkCount")]
    chunk_count: f64,
    #[serde(rename = "totalSizeBytes")]
    total_size_bytes: f64,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Default, Deserialize)]
struct UpdateData {
    name: Option<String>,
    #[serde(rename = "basePath")]
    base_path: Option<String>,
    #[serde(rename = "mountType")]
    mount_type: Option<String>,
    #[serde(rename = "storeType")]
    store_type: Option<String>,
    #[serde(rename = "includePatterns")]
    include_patterns: Option<Vec<String>>,
    #[serde(rename = "excludePatterns")]
    exclude_patterns: Option<Vec<String>>,
    enabled: Option<bool>,
    #[serde(rename = "lastScannedAt")]
    last_scanned_at: Option<String>,
    #[serde(rename = "scanStatus")]
    scan_status: Option<String>,
    #[serde(rename = "lastScanError")]
    last_scan_error: Option<String>,
    #[serde(rename = "conversionStatus")]
    conversion_status: Option<String>,
    #[serde(rename = "conversionError")]
    conversion_error: Option<String>,
    #[serde(rename = "fileCount")]
    file_count: Option<f64>,
    #[serde(rename = "chunkCount")]
    chunk_count: Option<f64>,
    #[serde(rename = "totalSizeBytes")]
    total_size_bytes: Option<f64>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/doc-mount-points-tier2.json")
}

#[test]
fn doc_mount_points_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_DOC_MOUNT_POINTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_DOC_MOUNT_POINTS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_DOC_MOUNT_POINTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_DOC_MOUNT_POINTS to the seed fixture .db (see test header)."
            );
            return;
        }
    };

    // Parse the committed spec (pepper + op sequence) — same file the oracle used.
    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    // Parse the oracle's expected post-op dump (one NDJSON object).
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut oracle_lines = oracle_text.lines().filter(|l| !l.trim().is_empty());
    let oracle: Value = serde_json::from_str(oracle_lines.next().expect("oracle line 1"))
        .expect("parse oracle dump");
    let gate_oracle: Value = serde_json::from_str(
        oracle_lines
            .next()
            .expect("oracle line 2 (the P4.D274 name gate — regenerate at the pin)"),
    )
    .expect("parse gate oracle");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work_dir = tempfile::Builder::new()
        .prefix("qt-dmp-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_dir.path().join("dmp-rust.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port. The Writer opens the
    // mount-index fixture by path — no special "mount-index writer" needed.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    // v4's table init (`onTableEnsured`) ensures the name index first.
    ensure_mount_point_name_unique_index(writer.connection()).expect("name index ensure");
    {
        let repo = writer.doc_mount_points();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => repo
                    .create(
                        &DmpCreate {
                            name: data.name.clone(),
                            base_path: data.base_path.clone(),
                            mount_type: data.mount_type.clone(),
                            store_type: data.store_type.clone(),
                            include_patterns: data.include_patterns.clone(),
                            exclude_patterns: data.exclude_patterns.clone(),
                            enabled: data.enabled,
                            last_scanned_at: data.last_scanned_at.clone(),
                            scan_status: data.scan_status.clone(),
                            last_scan_error: data.last_scan_error.clone(),
                            conversion_status: data.conversion_status.clone(),
                            conversion_error: data.conversion_error.clone(),
                            file_count: data.file_count,
                            chunk_count: data.chunk_count,
                            total_size_bytes: data.total_size_bytes,
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("doc_mount_points.create"),
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &DmpUpdate {
                                name: data.name.clone(),
                                base_path: data.base_path.clone(),
                                mount_type: data.mount_type.clone(),
                                store_type: data.store_type.clone(),
                                include_patterns: data.include_patterns.clone(),
                                exclude_patterns: data.exclude_patterns.clone(),
                                enabled: data.enabled,
                                last_scanned_at: data.last_scanned_at.clone(),
                                scan_status: data.scan_status.clone(),
                                last_scan_error: data.last_scan_error.clone(),
                                conversion_status: data.conversion_status.clone(),
                                conversion_error: data.conversion_error.clone(),
                                file_count: data.file_count,
                                chunk_count: data.chunk_count,
                                total_size_bytes: data.total_size_bytes,
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("doc_mount_points.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let removed = repo
                        .delete_row_only(id)
                        .expect("doc_mount_points.delete_row_only");
                    assert!(removed, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("doc_mount_points", "id")
        .expect("dump doc_mount_points");

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");

    let steps = run_name_gate(&writer);
    let want = gate_oracle["steps"].as_array().expect("gate steps");
    assert_eq!(steps.len(), want.len(), "gate step count");
    for (got, want) in steps.iter().zip(want) {
        assert_eq!(
            serde_json::to_string(got).unwrap(),
            serde_json::to_string(&normalize_gate_step(want)).unwrap(),
            "name-gate step {} diverged",
            want["label"]
        );
    }
    eprintln!(
        "OK: doc_mount_points tier-2 matched oracle ({n} rows; {} name-gate steps).",
        steps.len()
    );
}

/// The committed name-gate spec (P4.D274).
#[derive(Deserialize)]
struct GateSpec {
    stamp: String,
    base: CreateData,
    steps: Vec<GateStep>,
}

#[derive(Deserialize)]
struct GateStep {
    label: String,
    kind: String,
    id: String,
    name: Option<String>,
}

/// v4's step → the compared shape: lines rendered `level message k=v …` (v4's
/// key order; a JSON `null` as `null`).
fn normalize_gate_step(step: &Value) -> Value {
    let lines: Vec<Value> = step["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
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
            Value::String(out)
        })
        .collect();
    serde_json::json!({
        "label": step["label"],
        "error": step["error"],
        "lines": lines,
        "names": step["names"],
    })
}

fn run_name_gate(writer: &Writer) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/doc-mount-points-name-gate.json");
    let gate: GateSpec =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read gate spec")).unwrap();
    let repo = writer.doc_mount_points();
    let mut out = Vec::new();
    for step in &gate.steps {
        let (error, lines) =
            quilltap_core::test_support::captured_with(|| match step.kind.as_str() {
                "create" => repo
                    .create(
                        &DmpCreate {
                            name: step.name.clone().unwrap(),
                            base_path: gate.base.base_path.clone(),
                            mount_type: gate.base.mount_type.clone(),
                            store_type: gate.base.store_type.clone(),
                            include_patterns: gate.base.include_patterns.clone(),
                            exclude_patterns: gate.base.exclude_patterns.clone(),
                            enabled: gate.base.enabled,
                            last_scanned_at: gate.base.last_scanned_at.clone(),
                            scan_status: gate.base.scan_status.clone(),
                            last_scan_error: gate.base.last_scan_error.clone(),
                            conversion_status: gate.base.conversion_status.clone(),
                            conversion_error: gate.base.conversion_error.clone(),
                            file_count: gate.base.file_count,
                            chunk_count: gate.base.chunk_count,
                            total_size_bytes: gate.base.total_size_bytes,
                        },
                        &CreateOptions {
                            id: step.id.clone(),
                            created_at: gate.stamp.clone(),
                            updated_at: gate.stamp.clone(),
                        },
                    )
                    .err(),
                "update" => repo
                    .update(
                        &step.id,
                        &DmpUpdate {
                            name: step.name.clone(),
                            scan_status: step.name.is_none().then(|| "idle".to_string()),
                            updated_at: gate.stamp.clone(),
                            ..Default::default()
                        },
                    )
                    .err(),
                _ => writer
                    .connection()
                    .execute(
                        "INSERT INTO doc_mount_points (id, name, basePath, mountType, storeType, \
                       includePatterns, excludePatterns, enabled, scanStatus, conversionStatus, \
                       fileCount, chunkCount, totalSizeBytes, createdAt, updatedAt) \
                     VALUES (?1, ?2, '', 'database', 'documents', '[]', '[]', 1, 'idle', 'idle', \
                       0, 0, 0, ?3, ?3)",
                        rusqlite::params![step.id, step.name, gate.stamp],
                    )
                    .err()
                    .map(Into::into),
            });
        let names = {
            let mut stmt = writer
                .connection()
                .prepare("SELECT id, name FROM doc_mount_points ORDER BY id")
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok(serde_json::json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)? }))
                })
                .unwrap();
            rows.map(|r| r.unwrap()).collect::<Vec<_>>()
        };
        let lines: Vec<Value> = lines
            .iter()
            .map(|l| {
                let (level, rest) = l.split_once(' ').unwrap();
                let (_target, rest) = rest.split_once(' ').unwrap();
                Value::String(format!("{level} {rest}"))
            })
            .collect();
        out.push(serde_json::json!({
            "label": step.label,
            "error": error.map(|e| quilltap_core::db::fallback::error_text(&e)),
            "lines": lines,
            "names": names,
        }));
    }
    out
}
