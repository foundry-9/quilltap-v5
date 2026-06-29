//! Tier-2 differential test #3: the `text_replacement_rules` repo (Phase-2,
//! repo #3 after `folders` and `tags`).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-text-replacement-rules-fixture.ts), run the
//! SAME create / update / delete op sequence from the committed spec, dump the
//! `text_replacement_rules` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! Beyond `tags` this exercises a real INTEGER number column (`sortOrder`), two
//! boolean columns (`caseSensitive`, `enabled`), and — the headline —
//! CONFLICT DETECTION. Two ops are flagged `expectThrow`: both sides assert the
//! op returns a [`TrrError::Conflict`] (the Rust analogue of v4's
//! `TextReplacementRuleConflictError`) and is rejected, so the conflict logic is
//! proven independently on each side AND by the final-state dump (a port lacking
//! the check would have written a row, diverging the state).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-trr-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-text-replacement-rules-fixture.ts
//!   QT_FIXTURE_TRR=/tmp/qt-trr-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/text-replacement-rules-tier2.ts \
//!     > /tmp/oracle-trr.ndjson
//! Run:
//!   QT_ORACLE_TRR=/tmp/oracle-trr.ndjson \
//!   QT_FIXTURE_TRR=/tmp/qt-trr-fixture.db \
//!     cargo test -p quilltap-harness --test text_replacement_rules_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::text_replacement_rules::{CreateOptions, TrrCreate, TrrError, TrrUpdate};
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
        #[serde(default, rename = "expectThrow")]
        expect_throw: bool,
        data: CreateData,
        options: CreateOpts,
    },
    #[serde(rename = "update")]
    Update {
        #[serde(default, rename = "expectThrow")]
        expect_throw: bool,
        id: String,
        data: UpdateData,
    },
    #[serde(rename = "delete")]
    Delete { id: String },
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "fromText")]
    from_text: String,
    #[serde(rename = "toText")]
    to_text: String,
    #[serde(rename = "caseSensitive")]
    case_sensitive: bool,
    enabled: bool,
    #[serde(rename = "sortOrder")]
    sort_order: i64,
}

#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
struct UpdateData {
    #[serde(default, rename = "fromText")]
    from_text: Option<String>,
    #[serde(default, rename = "toText")]
    to_text: Option<String>,
    #[serde(default, rename = "caseSensitive")]
    case_sensitive: Option<bool>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default, rename = "sortOrder")]
    sort_order: Option<i64>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/text-replacement-rules-tier2.json")
}

#[test]
fn text_replacement_rules_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TRR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TRR to the oracle NDJSON (see test header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_TRR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_TRR to the seed fixture .db (see test header).");
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
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work = std::env::temp_dir().join(format!("qt-trr-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.text_replacement_rules();
        for op in &spec.ops {
            match op {
                Op::Create {
                    expect_throw,
                    data,
                    options,
                } => {
                    let res = repo.create(
                        &TrrCreate {
                            from_text: data.from_text.clone(),
                            to_text: data.to_text.clone(),
                            case_sensitive: data.case_sensitive,
                            enabled: data.enabled,
                            sort_order: data.sort_order,
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    );
                    assert_outcome(res.map(|_| true), *expect_throw, "create", &options.id);
                }
                Op::Update {
                    expect_throw,
                    id,
                    data,
                } => {
                    let res = repo.update(
                        id,
                        &TrrUpdate {
                            from_text: data.from_text.clone(),
                            to_text: data.to_text.clone(),
                            case_sensitive: data.case_sensitive,
                            enabled: data.enabled,
                            sort_order: data.sort_order,
                            updated_at: data.updated_at.clone(),
                        },
                    );
                    if !*expect_throw {
                        let found = res.expect("trr.update");
                        assert!(found, "update target {id} not found in fixture");
                    } else {
                        assert_outcome(res, true, "update", id);
                    }
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("trr.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("text_replacement_rules", "id")
        .expect("dump text_replacement_rules");

    let _ = std::fs::remove_file(&work);

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label). assert_eq on serde_json::Value is order-independent for
    // object keys and exact for the row arrays (both sides sorted by id).
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: text_replacement_rules tier-2 matched oracle ({n} rows).");
}

/// Assert an op's outcome against its `expectThrow` flag: a flagged op must
/// return `Err(TrrError::Conflict { .. })`; an unflagged op must succeed.
fn assert_outcome(res: Result<bool, TrrError>, expect_throw: bool, kind: &str, id: &str) {
    match (expect_throw, res) {
        (true, Err(TrrError::Conflict { .. })) => {}
        (true, Err(other)) => {
            panic!("expectThrow {kind} {id} returned the wrong error: {other}")
        }
        (true, Ok(_)) => panic!("expectThrow {kind} {id} did NOT conflict"),
        (false, Ok(_)) => {}
        (false, Err(e)) => panic!("{kind} {id} unexpectedly failed: {e}"),
    }
}
