//! Tier-2 differential test: the `provider_models` repo UPSERT (minted-values /
//! remap) path — the deferred `upsertModel` method.
//!
//! The earlier `provider_models` tier-2 case pinned every id and timestamp (the
//! zero-normalization form, scope create/update/delete). `upsertModel` mints its
//! own values — a fresh UUID + createdAt on a create branch, a fresh updatedAt on
//! every upsert — so nothing can be pinned. Both v4 and the Rust port mint their
//! own (different) ids + clocks; the raw dumps are reconciled by normalizing only
//! the legitimately nondeterministic fields, then structural-diffing the rest:
//!
//!   - **id remap.** Rows are dumped in natural-key (`modelId`) order — identical
//!     on both sides because every final row's `modelId` is a distinct input.
//!     Walking that order, each `id` gets a first-seen canonical token (`ID_0`,
//!     `ID_1`, …). (There is no FK between provider_models rows, so `id` is the
//!     only id column.)
//!   - **timestamps.** `createdAt` / `updatedAt` → a `<ts>` placeholder. The
//!     `createdAt == updatedAt` invariant is DROPPED here: an upsert that hits the
//!     UPDATE branch mints a fresh `updatedAt` while preserving the original
//!     `createdAt`, so the two legitimately differ.
//!
//! The predicate under test is `findByProviderAndModelId`'s baseUrl handling:
//! v4 constrains baseUrl ONLY when it is truthy. Op 1 upserts a null-baseUrl
//! payload against the null-baseUrl `gpt-4o` seed row and MUST update it (not
//! create a duplicate) — the dump has exactly four rows (gpt-4o, claude-opus-4,
//! local-llama, text-embedding-3-small), proving no duplicate was minted.
//!
//! The SAME normalization runs over both dumps (one implementation, here), so the
//! remap is provably consistent — the oracle stays a raw emitter.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-pm-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-provider-models-upsert-fixture.ts
//!   QT_FIXTURE_PROVIDER_MODELS_UPSERT=/tmp/qt-pm-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/provider-models-upsert-tier2.ts \
//!     > /tmp/oracle-pm-upsert.ndjson
//! Run:
//!   QT_ORACLE_PROVIDER_MODELS_UPSERT=/tmp/oracle-pm-upsert.ndjson \
//!   QT_FIXTURE_PROVIDER_MODELS_UPSERT=/tmp/qt-pm-upsert-fixture.db \
//!     cargo test -p quilltap-harness --test provider_models_upsert_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::provider_models::PmCreate;
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
    Upsert { data: UpsertData },
}

#[derive(Deserialize)]
struct UpsertData {
    provider: String,
    #[serde(rename = "modelId")]
    model_id: String,
    #[serde(rename = "modelType")]
    model_type: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(rename = "contextWindow")]
    context_window: Option<f64>,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: Option<f64>,
    deprecated: bool,
    experimental: bool,
}

/// Columns that hold a generated id (only the PK here — no FK between rows).
const ID_COLUMNS: &[&str] = &["id"];
/// Columns that hold a wall-clock timestamp minted at create/upsert time.
const TS_COLUMNS: &[&str] = &["createdAt", "updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/provider-models-upsert-tier2.json")
}

/// Normalize a `{ table, columns, rows }` dump in place: first-seen id remap over
/// the rows in their given (modelId) order, then timestamp placeholdering. The
/// `createdAt == updatedAt` invariant is intentionally NOT asserted (an upsert
/// UPDATE mints a fresh updatedAt while preserving createdAt).
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
fn provider_models_upsert_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PROVIDER_MODELS_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PROVIDER_MODELS_UPSERT to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_PROVIDER_MODELS_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_PROVIDER_MODELS_UPSERT to the seed fixture .db (see header)."
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
    let work = std::env::temp_dir().join(format!("qt-pm-upsert-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME upsert sequence through the Rust port, minting our own
    // ids/timestamps on the create/upsert branches.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.provider_models();
        for op in &spec.ops {
            let Op::Upsert { data } = op;
            repo.upsert_model(&PmCreate {
                provider: data.provider.clone(),
                model_id: data.model_id.clone(),
                model_type: data.model_type.clone(),
                display_name: data.display_name.clone(),
                base_url: data.base_url.clone(),
                context_window: data.context_window,
                max_output_tokens: data.max_output_tokens,
                deprecated: data.deprecated,
                experimental: data.experimental,
            })
            .unwrap_or_else(|e| panic!("upsert_model {} failed: {e}", data.model_id));
        }
    }

    let mut got = writer
        .dump_table_json("provider_models", "modelId")
        .expect("dump provider_models");

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

    // Sanity: four distinct final rows (the null-baseUrl upsert UPDATED the seed
    // gpt-4o row instead of minting a duplicate).
    let rows = got["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        4,
        "expected four final rows (no duplicate from the null-baseUrl upsert)"
    );
    let model_ids: Vec<&str> = rows.iter().filter_map(|r| r["modelId"].as_str()).collect();
    assert_eq!(
        model_ids,
        vec![
            "claude-opus-4",
            "gpt-4o",
            "local-llama",
            "text-embedding-3-small"
        ],
        "unexpected modelId set / order"
    );
    // Guard against a no-op normalization: ids must be tokens, not raw UUIDs.
    let m: &Map<String, Value> = rows[0].as_object().unwrap();
    assert!(
        m["id"].as_str().unwrap().starts_with("ID_"),
        "id was not remapped"
    );

    eprintln!(
        "OK: provider_models upsert remap tier-2 matched oracle ({} rows).",
        rows.len()
    );
}
