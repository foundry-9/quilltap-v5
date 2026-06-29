//! Tier-2 differential test: the `tfidf_vocabulary` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-tfidf-vocabulary-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `tfidf_vocabularies` table canonically (sorted by id), and assert the post-op
//! state is identical.
//!
//! ⚠️ MINTED `updatedAt` — single-column placeholder normalization. Unlike
//! `folders`/`tags`/`image_profiles` (zero-normalization, every field pinned),
//! v4's `TfidfVocabularyRepository` OVERRIDES the base create/update and sets
//! `updatedAt = getCurrentTimestamp()` unconditionally — `options.updatedAt` /
//! patch `updatedAt` are ignored. The Rust port mints `updatedAt` the same way
//! (`clock::now_iso`). So `id`, `createdAt`, and every payload column are pinned
//! and diffed EXACTLY; only `updatedAt` is collapsed to a `<ts>` placeholder on
//! both sides (the minted-timestamp form from `folders_remap`, here over one
//! column with no id remap — ids are pinned). `createdAt` is honored on create
//! and preserved on update, so it stays exact and is asserted as-is.
//!
//! This banks PLAIN-STRING columns that hold JSON text (`vocabulary`, `idf` —
//! z.string(), stored as-is, never re-stringified) and two REAL number columns
//! (`avgDocLength` bare z.number(); `vocabularySize` z.number().int().positive(),
//! min only -> still REAL). A fractional `avgDocLength` dumps as a float;
//! integer-valued REALs collapse to JSON integers via js_number_to_json.
//! `includeBigrams` is a boolean (-> INTEGER 0/1).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-tv-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tfidf-vocabulary-fixture.ts
//!   QT_FIXTURE_TV=/tmp/qt-tv-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tfidf-vocabulary-tier2.ts \
//!     > /tmp/oracle-tv.ndjson
//! Run:
//!   QT_ORACLE_TFIDF_VOCABULARY=/tmp/oracle-tv.ndjson \
//!   QT_FIXTURE_TFIDF_VOCABULARY=/tmp/qt-tv-fixture.db \
//!     cargo test -p quilltap-harness --test tfidf_vocabulary_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::tfidf_vocabulary::{CreateOptions, TvCreate, TvUpdate};
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
    #[serde(rename = "profileId")]
    profile_id: String,
    #[serde(rename = "userId")]
    user_id: String,
    vocabulary: String,
    idf: String,
    #[serde(rename = "avgDocLength")]
    avg_doc_length: f64,
    #[serde(rename = "vocabularySize")]
    vocabulary_size: f64,
    #[serde(rename = "includeBigrams")]
    include_bigrams: bool,
    #[serde(rename = "fittedAt")]
    fitted_at: String,
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
    #[serde(default, rename = "profileId")]
    profile_id: Option<String>,
    #[serde(default, rename = "userId")]
    user_id: Option<String>,
    #[serde(default)]
    vocabulary: Option<String>,
    #[serde(default)]
    idf: Option<String>,
    #[serde(default, rename = "avgDocLength")]
    avg_doc_length: Option<f64>,
    #[serde(default, rename = "vocabularySize")]
    vocabulary_size: Option<f64>,
    #[serde(default, rename = "includeBigrams")]
    include_bigrams: Option<bool>,
    #[serde(default, rename = "fittedAt")]
    fitted_at: Option<String>,
}

/// The minted (nondeterministic) column — placeholdered on both sides.
const TS_COLUMNS: &[&str] = &["updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/tfidf-vocabulary-tier2.json")
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
fn tfidf_vocabulary_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TFIDF_VOCABULARY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TFIDF_VOCABULARY to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_TFIDF_VOCABULARY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_TFIDF_VOCABULARY to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-tv-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.tfidf_vocabulary();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => {
                    repo.create(
                        &TvCreate {
                            profile_id: data.profile_id.clone(),
                            user_id: data.user_id.clone(),
                            vocabulary: data.vocabulary.clone(),
                            idf: data.idf.clone(),
                            avg_doc_length: data.avg_doc_length,
                            vocabulary_size: data.vocabulary_size,
                            include_bigrams: data.include_bigrams,
                            fitted_at: data.fitted_at.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                        },
                    )
                    .expect("tfidf_vocabulary.create");
                }
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &TvUpdate {
                                profile_id: data.profile_id.clone(),
                                user_id: data.user_id.clone(),
                                vocabulary: data.vocabulary.clone(),
                                idf: data.idf.clone(),
                                avg_doc_length: data.avg_doc_length,
                                vocabulary_size: data.vocabulary_size,
                                include_bigrams: data.include_bigrams,
                                fitted_at: data.fitted_at.clone(),
                            },
                        )
                        .expect("tfidf_vocabulary.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("tfidf_vocabulary.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let mut got = writer
        .dump_table_json("tfidf_vocabularies", "id")
        .expect("dump tfidf_vocabularies");

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
    eprintln!("OK: tfidf_vocabulary tier-2 matched oracle ({n} rows).");
}
