//! Tier-2 differential test: `help_docs.upsertByPath` — the deferred path-keyed
//! upsert on the help-docs repo, in the MINTED-VALUES (remap) form.
//!
//! The pinned `help_docs` tier-2 case (help_docs_tier2_equivalence.rs) pins every
//! id and timestamp. `upsertByPath` cannot: it mints its own id + timestamps on
//! the create branch and mints `updatedAt` on the update branch. So both v4 and
//! the Rust port independently mint values and the raw dumps cannot match. They
//! are reconciled by normalizing only the nondeterministic fields, then
//! structural-diffing the rest:
//!
//!   - **id remap.** Rows are dumped in natural-key (`path`) order — identical on
//!     both sides because paths are inputs. Walking that order, each `id` gets a
//!     first-seen canonical token (`ID_0`, `ID_1`, …).
//!   - **timestamps.** `createdAt` / `updatedAt` → a `<ts>` placeholder. Unlike
//!     the folders-remap case, there is NO `createdAt == updatedAt` assertion: an
//!     upsert-update mints a new `updatedAt` while preserving the original
//!     `createdAt`, so they legitimately differ on the updated rows.
//!
//! Everything else is compared exactly — in particular the `embedding` column
//! (lowercase hex on both sides, or null). That is what proves the banked
//! behavior: the seed row `help/aurora.md` carries a non-null embedding and op 1
//! upserts onto it (a text-only update), so its embedding hex must survive
//! unchanged; the created rows (`help/pascal.md`, `help/carina.md`) must have a
//! NULL embedding. Both checks ride the exact row diff.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-hd-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-help-docs-upsert-fixture.ts
//!   QT_FIXTURE_HELP_DOCS_UPSERT=/tmp/qt-hd-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/help-docs-upsert-tier2.ts \
//!     > /tmp/oracle-hd-upsert.ndjson
//! Run:
//!   QT_ORACLE_HELP_DOCS_UPSERT=/tmp/oracle-hd-upsert.ndjson \
//!   QT_FIXTURE_HELP_DOCS_UPSERT=/tmp/qt-hd-upsert-fixture.db \
//!     cargo test -p quilltap-harness --test help_docs_upsert_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::help_docs::HdUpsert;
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
    #[serde(rename = "upsert")]
    Upsert { data: UpsertData },
}

#[derive(Deserialize)]
struct UpsertData {
    title: String,
    path: String,
    url: String,
    content: String,
    #[serde(rename = "contentHash")]
    content_hash: String,
}

/// Columns that hold a generated id.
const ID_COLUMNS: &[&str] = &["id"];
/// Columns that hold a wall-clock timestamp minted at create/upsert time.
const TS_COLUMNS: &[&str] = &["createdAt", "updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/help-docs-upsert-tier2.json")
}

/// Normalize a `{ table, columns, rows }` dump in place: first-seen id remap over
/// the rows in their given (`path`) order, then timestamp placeholdering. Unlike
/// the folders-remap case there is no `createdAt == updatedAt` invariant — an
/// upsert-update preserves `createdAt` while minting a new `updatedAt`.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));

    let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));

        for col in ID_COLUMNS {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

#[test]
fn help_docs_upsert_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_HELP_DOCS_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_HELP_DOCS_UPSERT to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_HELP_DOCS_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_HELP_DOCS_UPSERT to the seed fixture .db (see header)."
            );
            return;
        }
    };

    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-hd-upsert-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME upsert sequence through the Rust port, minting our own ids/ts.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.help_docs();
        for op in &spec.ops {
            let Op::Upsert { data } = op;
            repo.upsert_by_path(&HdUpsert {
                title: data.title.clone(),
                path: data.path.clone(),
                url: data.url.clone(),
                content: data.content.clone(),
                content_hash: data.content_hash.clone(),
            })
            .unwrap_or_else(|e| panic!("help_docs.upsert_by_path {}: {e}", data.path));
        }
    }

    let mut got = writer
        .dump_table_json("help_docs", "path")
        .expect("dump help_docs");

    let _ = std::fs::remove_file(&work);

    // One normalization, applied to both dumps.
    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "remapped row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    // Sanity guards.
    let rows = got["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        4,
        "expected four final rows (2 seed + 2 created)"
    );

    // The remap actually fired: ids became ID_ tokens, not raw UUIDs.
    assert!(
        rows[0]["id"].as_str().unwrap().starts_with("ID_"),
        "id was not remapped"
    );

    // The created rows (help/carina.md, help/pascal.md) carry a NULL embedding;
    // the upsert-updated seed row (help/aurora.md) keeps its non-null embedding.
    // (These ride the exact row diff above; assert them explicitly as guards.)
    let by_path = |p: &str| {
        rows.iter()
            .find(|r| r["path"] == Value::String(p.into()))
            .unwrap_or_else(|| panic!("row {p} missing"))
    };
    assert!(
        !by_path("help/aurora.md")["embedding"].is_null(),
        "upsert-updated aurora row should keep its embedding"
    );
    assert!(
        by_path("help/pascal.md")["embedding"].is_null(),
        "created pascal row should have NULL embedding"
    );
    assert!(
        by_path("help/carina.md")["embedding"].is_null(),
        "created carina row should have NULL embedding"
    );

    eprintln!(
        "OK: help_docs upsert tier-2 matched oracle ({} rows).",
        rows.len()
    );
}
