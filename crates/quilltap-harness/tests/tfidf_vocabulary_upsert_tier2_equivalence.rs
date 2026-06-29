//! Tier-2 differential test: the `tfidf_vocabulary` repo's `upsertByProfileId`
//! method (Phase-2), in the MINTED-VALUES (remap) form.
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-tfidf-vocabulary-upsert-fixture.ts), run the
//! SAME sequence of `upsertByProfileId` ops from the committed spec, dump the
//! `tfidf_vocabularies` table canonically (sorted by the natural key
//! `profileId`), and assert the post-op state is equivalent under normalization.
//!
//! ⚠️ REMAP (minted-values) normalization. v4's `upsertByProfileId`:
//!   - find by `profileId`; if found -> `update(existing.id, FULL data)`
//!     (mints `updatedAt`, preserves `createdAt`);
//!   - else -> `create(data)` (mints `id` + `createdAt` + `updatedAt`).
//!
//! So neither side can be pinned: the CREATE branch mints a random `id` and
//! wall-clock timestamps, the UPDATE branch mints `updatedAt`. The two dumps are
//! reconciled by normalizing ONLY the legitimately nondeterministic fields:
//!   - **id remap.** Rows are dumped in natural-key (`profileId`) order — identical
//!     on both sides because `profileId` is an input. Walking that order, each
//!     `id` value gets a first-seen canonical token (`ID_0`, `ID_1`, …). Op 2
//!     CREATEs a new profileId and op 3 UPDATEs that same profileId, so the remap
//!     verifies an update landed on the row whose id the port minted.
//!   - **timestamps.** `createdAt` / `updatedAt` -> a `<ts>` placeholder. NOTE:
//!     unlike `folders_remap`, the `createdAt == updatedAt` create-invariant is
//!     NOT asserted — an UPDATE leaves `createdAt` (the original) older than the
//!     freshly minted `updatedAt`.
//!
//! Marshaling banked: plain-string JSON-text columns (`vocabulary`/`idf`), the two
//! REAL number columns (`avgDocLength` fractional/integer, `vocabularySize`), and
//! the boolean `includeBigrams` (set both true and false across the ops).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-tv-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-tfidf-vocabulary-upsert-fixture.ts
//!   QT_FIXTURE_TFIDF_VOCABULARY_UPSERT=/tmp/qt-tv-upsert-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/tfidf-vocabulary-upsert-tier2.ts \
//!     > /tmp/oracle-tv-upsert.ndjson
//! Run:
//!   QT_ORACLE_TFIDF_VOCABULARY_UPSERT=/tmp/oracle-tv-upsert.ndjson \
//!   QT_FIXTURE_TFIDF_VOCABULARY_UPSERT=/tmp/qt-tv-upsert-fixture.db \
//!     cargo test -p quilltap-harness --test tfidf_vocabulary_upsert_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::tfidf_vocabulary::TvCreate;
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

/// The upsert payload — v4's `data: Omit<TfidfVocabulary,'id'|timestamps>`, which
/// INCLUDES `profileId` (the lookup key). Maps directly onto `TvCreate`.
#[derive(Deserialize)]
struct UpsertData {
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

/// Columns that hold a generated id (the PK; this repo has no FK-to-generated-id).
const ID_COLUMNS: &[&str] = &["id"];
/// Columns that hold a wall-clock timestamp minted at create/update time.
const TS_COLUMNS: &[&str] = &["createdAt", "updatedAt"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/tfidf-vocabulary-upsert-tier2.json")
}

/// Normalize a `{ table, columns, rows }` dump in place: first-seen `id` remap
/// over the rows in their given (`profileId`) order, then timestamp
/// placeholdering of `createdAt`/`updatedAt`. NO `createdAt == updatedAt`
/// assertion — an upsert UPDATE leaves them legitimately different.
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
fn tfidf_vocabulary_upsert_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TFIDF_VOCABULARY_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TFIDF_VOCABULARY_UPSERT to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_TFIDF_VOCABULARY_UPSERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_TFIDF_VOCABULARY_UPSERT to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!("qt-tv-upsert-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME upsert op sequence through the Rust port, minting our own
    // ids/timestamps (the repo handles find-or-create + minting internally).
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.tfidf_vocabulary();
        for op in &spec.ops {
            let Op::Upsert { data } = op;
            repo.upsert_by_profile_id(&TvCreate {
                profile_id: data.profile_id.clone(),
                user_id: data.user_id.clone(),
                vocabulary: data.vocabulary.clone(),
                idf: data.idf.clone(),
                avg_doc_length: data.avg_doc_length,
                vocabulary_size: data.vocabulary_size,
                include_bigrams: data.include_bigrams,
                fitted_at: data.fitted_at.clone(),
            })
            .expect("tfidf_vocabulary.upsert_by_profile_id");
        }
    }

    let mut got = writer
        .dump_table_json("tfidf_vocabularies", "profileId")
        .expect("dump tfidf_vocabularies");

    let _ = std::fs::remove_file(&work);

    // One normalization (id remap + timestamp placeholder), applied to both dumps.
    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

    // Structural diff: table + columns + rows must match (ignore the oracle's
    // "case" label). Both sides are sorted by profileId.
    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "remapped row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    // Sanity: the expected final population is 4 rows — 2 seed (one UPDATEd via
    // op 1, one untouched) + 2 CREATEd (op 2 then UPDATEd by op 3; op 4).
    let rows = got["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 4, "expected four final rows");
    // Guard against a no-op normalization: ids must be tokens, not raw UUIDs.
    let m = rows[0].as_object().unwrap();
    assert!(
        m["id"].as_str().unwrap().starts_with("ID_"),
        "id was not remapped"
    );

    eprintln!(
        "OK: tfidf_vocabulary upsert tier-2 matched oracle ({} rows).",
        rows.len()
    );
}
