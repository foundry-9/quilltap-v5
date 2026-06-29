//! Tier-2 differential test: the `plugin_config` repo's `upsertForUserPlugin`
//! method (Phase-2), in the MINTED-VALUES (remap) form.
//!
//! `plugin_config` tier-2 so far pinned every id and timestamp (the
//! zero-normalization form). `upsertForUserPlugin` cannot: on the create path it
//! mints a random UUID, and on every path it mints `updatedAt` (the update path
//! preserves createdAt but re-mints updatedAt). Both v4 and the Rust port mint
//! their own (different) values, so the raw dumps cannot match. They are
//! reconciled by normalizing only the legitimately nondeterministic fields, then
//! structural-diffing the rest:
//!
//!   - **id remap.** Rows are dumped in natural-key (`pluginName`) order —
//!     identical on both sides because plugin names are inputs, not generated.
//!     Walking that order, each `id` value gets a first-seen canonical token
//!     (`ID_0`, `ID_1`, …). A minted id thus verifies its row WITHOUT pinning the
//!     literal id.
//!   - **timestamps.** `createdAt` / `updatedAt` → a `<ts>` placeholder. NOTE:
//!     the `createdAt == updatedAt` create-invariant is **NOT** asserted here —
//!     the UPDATE path (merge → `update`) preserves createdAt while re-minting
//!     updatedAt, so the two diverge by design.
//!
//! Everything else (userId, pluginName, config, enabled) is diffed exactly. The
//! SAME normalization runs over both the oracle dump and the Rust dump (one
//! implementation, here), so the remap is provably consistent.
//!
//! v4 `upsertForUserPlugin`: find by (userId, pluginName); if found → MERGE
//! `{ ...existing.config, ...config }` then `update(id, {config: merged})`; else
//! → `create({userId, pluginName, config})`. `enabled` is never set by upsert →
//! SQL NULL on every created/upserted row.
//!
//! OPEN-JSON MERGE CONSTRAINT (tracked deferred seam #5): every stored config —
//! INCLUDING every MERGE result — is `{}` or a SINGLE-KEY object, so v4's
//! insertion-order `JSON.stringify` and Rust's key-sorting `serde_json::Value`
//! serialize byte-identically. The merge ops overwrite the value under the SAME
//! single key (or merge an empty existing with a single-key new), never
//! producing a 2+-key object — close the seam (preserve-insertion-order
//! serializer) before such an op lands.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-pc-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-plugin-config-upsert-fixture.ts
//!   QT_FIXTURE_PLUGIN_CONFIG_UPSERT=/tmp/qt-pc-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/plugin-config-upsert-tier2.ts \
//!     > /tmp/oracle-pc-upsert.ndjson
//! Run:
//!   QT_ORACLE_PLUGIN_CONFIG_UPSERT=/tmp/oracle-pc-upsert.ndjson \
//!   QT_FIXTURE_PLUGIN_CONFIG_UPSERT=/tmp/qt-pc-upsert-fixture.db \
//!     cargo test -p quilltap-harness --test plugin_config_upsert_tier2_equivalence

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
        #[serde(rename = "userId")]
        user_id: String,
        #[serde(rename = "pluginName")]
        plugin_name: String,
        config: Value,
    },
}

/// Columns that hold a generated id (the PK).
const ID_COLUMNS: &[&str] = &["id"];
/// Columns that hold a wall-clock timestamp minted at create/update time.
const TS_COLUMNS: &[&str] = &["createdAt", "updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/plugin-config-upsert-tier2.json")
}

/// Normalize a `{ table, columns, rows }` dump in place: first-seen id remap over
/// the rows in their given (`pluginName`-sorted) order, then timestamp
/// placeholdering. The `createdAt == updatedAt` invariant is intentionally NOT
/// asserted (the upsert UPDATE path re-mints updatedAt while preserving
/// createdAt).
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
fn plugin_config_upsert_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PLUGIN_CONFIG_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PLUGIN_CONFIG_UPSERT to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_PLUGIN_CONFIG_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_PLUGIN_CONFIG_UPSERT to the seed fixture .db (see header)."
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
    let work = std::env::temp_dir().join(format!("qt-pc-upsert-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME upsert sequence through the Rust port, minting our own
    // ids/timestamps.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.plugin_config();
        for op in &spec.ops {
            let Op::Upsert {
                user_id,
                plugin_name,
                config,
            } = op;
            repo.upsert_for_user_plugin(user_id, plugin_name, config)
                .expect("plugin_config.upsert_for_user_plugin");
        }
    }

    let mut got = writer
        .dump_table_json("plugin_configs", "pluginName")
        .expect("dump plugin_configs");

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

    // Sanity: the expected final rows are present and both paths fired.
    let rows = got["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        4,
        "expected four final rows (curl, fresh, search, weather)"
    );
    // Guard against a no-op normalization: ids must be tokens, not raw UUIDs.
    let first: &Map<String, Value> = rows[0].as_object().unwrap();
    assert!(
        first["id"].as_str().unwrap().starts_with("ID_"),
        "id was not remapped"
    );

    eprintln!(
        "OK: plugin_config upsert tier-2 matched oracle ({} rows).",
        rows.len()
    );
}
