//! Tier-2 differential test: the `embedding_status` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-embedding-status-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `embedding_status` table canonically (sorted by id), and assert the post-op
//! state is identical.
//!
//! ⚠️ MINTED `updatedAt` — single-column placeholder normalization. Like
//! `tfidf_vocabulary` (and unlike `folders`/`tags`/`image_profiles`, which pin
//! every field), v4's `EmbeddingStatusRepository` OVERRIDES the base
//! create/update and sets `updatedAt = getCurrentTimestamp()` unconditionally —
//! `options.updatedAt` / patch `updatedAt` are ignored. The Rust port mints
//! `updatedAt` the same way (`clock::now_iso`). So `id`, `createdAt`, and every
//! payload column are pinned and diffed EXACTLY; only `updatedAt` is collapsed to
//! a `<ts>` placeholder on both sides. `createdAt` is honored on create and
//! preserved on update, so it stays exact and is asserted as-is.
//!
//! This banks an all-TEXT marshaling surface: UUID columns (userId, entityId,
//! profileId) as TEXT, two enum columns as TEXT (entityType, status), and two
//! NULLABLE TEXT columns (embeddedAt, error). No booleans, numbers, JSON, or BLOB.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-es-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-embedding-status-fixture.ts
//!   QT_FIXTURE_ES=/tmp/qt-es-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/embedding-status-tier2.ts \
//!     > /tmp/oracle-es.ndjson
//! Run:
//!   QT_ORACLE_EMBEDDING_STATUS=/tmp/oracle-es.ndjson \
//!   QT_FIXTURE_EMBEDDING_STATUS=/tmp/qt-es-fixture.db \
//!     cargo test -p quilltap-harness --test embedding_status_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::embedding_status::{CreateOptions, EsCreate, EsUpdate};
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
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "entityType")]
    entity_type: String,
    #[serde(rename = "entityId")]
    entity_id: String,
    #[serde(rename = "profileId")]
    profile_id: String,
    status: String,
    #[serde(default, rename = "embeddedAt")]
    embedded_at: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Create options carry ONLY id + createdAt — v4's override ignores
/// `options.updatedAt`, so the spec omits it.
#[derive(Deserialize)]
struct CreateOpts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

/// Update patch — note NO `updatedAt`: v4 mints it (overwrites any caller value).
#[derive(Deserialize)]
struct UpdateData {
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default, rename = "entityType")]
    entity_type: Option<String>,
    #[serde(default, rename = "entityId")]
    entity_id: Option<String>,
    #[serde(default, rename = "profileId")]
    profile_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default, rename = "embeddedAt")]
    embedded_at: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// The minted (nondeterministic) column — placeholdered on both sides.
const TS_COLUMNS: &[&str] = &["updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/embedding-status-tier2.json")
}

/// Collapse the minted `updatedAt` column to a `<ts>` placeholder, in place, on a
/// `{ table, columns, rows }` dump. Every other field (ids, createdAt, payload)
/// is left exact so the structural diff still catches real divergence.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

#[test]
fn embedding_status_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_EMBEDDING_STATUS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_EMBEDDING_STATUS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_EMBEDDING_STATUS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_EMBEDDING_STATUS to the seed fixture .db (see test header)."
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
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let work = std::env::temp_dir().join(format!("qt-es-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.embedding_status();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &EsCreate {
                            user_id: data.user_id.clone(),
                            entity_type: data.entity_type.clone(),
                            entity_id: data.entity_id.clone(),
                            profile_id: data.profile_id.clone(),
                            status: data.status.clone(),
                            embedded_at: data.embedded_at.clone(),
                            error: data.error.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                        },
                    )
                    .expect("embedding_status.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &EsUpdate {
                                user_id: data.user_id.clone(),
                                entity_type: data.entity_type.clone(),
                                entity_id: data.entity_id.clone(),
                                profile_id: data.profile_id.clone(),
                                status: data.status.clone(),
                                embedded_at: data.embedded_at.clone(),
                                error: data.error.clone(),
                            },
                        )
                        .expect("embedding_status.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("embedding_status.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let mut got = writer
        .dump_table_json("embedding_status", "id")
        .expect("dump embedding_status");

    let _ = std::fs::remove_file(&work);

    // One normalization (placeholder the minted updatedAt), applied to both dumps.
    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

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
    eprintln!("OK: embedding_status tier-2 matched oracle ({n} rows).");
}
