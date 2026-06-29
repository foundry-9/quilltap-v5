//! Tier-2 differential test: the `character_plugin_data` repo's `upsert` method
//! (Phase-2, MINTED-VALUES / remap path).
//!
//! Structural DB diff in the minted-values form. Both sides start from the SAME
//! seed fixture (built by
//! harness/oracle/fixtures/build-character-plugin-data-upsert-fixture.ts), run
//! the SAME `upsert(characterId, pluginName, data)` op sequence from the
//! committed spec, dump the `character_plugin_data` table, and assert the post-op
//! state is identical AFTER normalization.
//!
//! Unlike the pinned `character_plugin_data_tier2_equivalence`, `upsert` mints
//! its own id (create branch) and `now` (both branches) internally, so nothing
//! can be pinned. The two raw dumps cannot match byte-for-byte; they are
//! reconciled by normalizing only the legitimately-nondeterministic fields, then
//! structural-diffing the rest:
//!
//!   - **id remap.** Rows are dumped in natural-key (`pluginName`) order —
//!     identical on both sides because pluginNames are inputs, and the corpus
//!     gives every FINAL row a distinct pluginName. Walking that order, each `id`
//!     gets a first-seen canonical token (`ID_0`, `ID_1`, …). There is no FK
//!     column here (unlike `folders.parentFolderId`), so `id` is the only id
//!     column remapped.
//!   - **timestamps.** `createdAt` / `updatedAt` → a `<ts>` placeholder. Note the
//!     `createdAt == updatedAt` create-invariant is NOT asserted here: an UPDATE
//!     branch of `upsert` mints a fresh `updatedAt` while preserving the seed (or
//!     create-time) `createdAt`, so the two legitimately diverge.
//!
//! The SAME normalization runs over both dumps (one implementation, here), so the
//! remap is provably consistent — the oracle stays a raw emitter.
//!
//! The corpus exercises BOTH upsert branches: an UPDATE of a seed row, a CREATE
//! of a new pair, an UPDATE of a freshly-minted-id row, and a second CREATE. The
//! `data` corpus is constrained to `{}` / single-key objects so v4's
//! insertion-order `JSON.stringify` and Rust's key-sorting `serde_json::Value`
//! serialize byte-identically (the multi-key key-order seam is a tracked
//! deferral — see character_plugin_data.rs).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-cpd-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-character-plugin-data-upsert-fixture.ts
//!   QT_FIXTURE_CHARACTER_PLUGIN_DATA_UPSERT=/tmp/qt-cpd-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/character-plugin-data-upsert-tier2.ts \
//!     > /tmp/oracle-cpd-upsert.ndjson
//! Run:
//!   QT_ORACLE_CHARACTER_PLUGIN_DATA_UPSERT=/tmp/oracle-cpd-upsert.ndjson \
//!   QT_FIXTURE_CHARACTER_PLUGIN_DATA_UPSERT=/tmp/qt-cpd-upsert-fixture.db \
//!     cargo test -p quilltap-harness --test character_plugin_data_upsert_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{Map, Value};

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
    Upsert {
        #[serde(rename = "characterId")]
        character_id: String,
        #[serde(rename = "pluginName")]
        plugin_name: String,
        data: Value,
    },
}

/// Columns that hold a generated id (only the PK here — no FK to a generated id).
const ID_COLUMNS: &[&str] = &["id"];
/// Columns that hold a wall-clock timestamp minted at create/update time.
const TS_COLUMNS: &[&str] = &["createdAt", "updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/character-plugin-data-upsert-tier2.json")
}

/// Normalize a `{ table, columns, rows }` dump in place: first-seen id remap over
/// the rows in their given order (`pluginName`), then timestamp placeholdering.
///
/// Unlike the folders-remap normalizer, this does NOT assert `createdAt ==
/// updatedAt` — `upsert`'s update branch mints a fresh `updatedAt` while keeping
/// the original `createdAt`, so the two legitimately differ on updated rows.
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
fn character_plugin_data_upsert_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHARACTER_PLUGIN_DATA_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CHARACTER_PLUGIN_DATA_UPSERT to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHARACTER_PLUGIN_DATA_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CHARACTER_PLUGIN_DATA_UPSERT to the seed fixture .db (see header)."
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
    let work = std::env::temp_dir().join(format!("qt-cpd-upsert-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME upsert sequence through the Rust port, minting our own
    // ids/timestamps.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.character_plugin_data();
        for op in &spec.ops {
            let Op::Upsert {
                character_id,
                plugin_name,
                data,
            } = op;
            repo.upsert(character_id, plugin_name, data.clone())
                .expect("character_plugin_data.upsert");
        }
    }

    let mut got = writer
        .dump_table_json("character_plugin_data", "pluginName")
        .expect("dump character_plugin_data");

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

    // Sanity: four distinct final rows (two seed pairs — one updated in place,
    // one untouched — plus two created pairs), and the id remap actually fired.
    let rows = got["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 4, "expected four final rows");
    let first: &Map<String, Value> = rows[0].as_object().unwrap();
    assert!(
        first["id"].as_str().unwrap().starts_with("ID_"),
        "id was not remapped"
    );

    eprintln!(
        "OK: character_plugin_data upsert tier-2 matched oracle ({} rows).",
        rows.len()
    );
}
