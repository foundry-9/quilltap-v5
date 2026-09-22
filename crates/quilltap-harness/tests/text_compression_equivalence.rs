//! Tier-1 differential: the Rust compressed-text codec must agree with v4's
//! REAL `lib/database/text-compression.ts`, case by case, on every corpus row.
//!
//! v5 is a PEER WRITER of the same synced database, so agreement here is not
//! "round-trips correctly" — it is **byte identity**. A v5-written cell and a
//! v4-written cell of the same text must be the same bytes, or a synced
//! instance grows a mixed corpus neither side can account for.
//!
//! ## The parity measurement (P4.D203 tier-1 item 1)
//!
//! Measured 2026-09-21 over this corpus: the `brotli` crate 8.0.4 at
//! `quality: 5`, `size_hint: raw.len()` and the default `lgwin` is
//! **byte-identical** to Node 24.13.1's bundled brotli 1.2.0
//! `brotliCompressSync(raw, { QUALITY: 5, SIZE_HINT: raw.length })` on every
//! row — decision, stored length and bytes. So this family is a BYTE pin and
//! the harness tier-2 differ needs **no decode normalizer**: a compressed cell
//! hexes identically on both sides (`harness/oracle/lib/tier2.ts` `canonValue`
//! and `quilltap_core::db::cell_to_json` agree already).
//!
//! ⚠ The one trap: calling `flush()` on the `CompressorWriter` before dropping
//! it emits an empty meta-block, adding 1–3 bytes to every payload — parity
//! breaks while every round-trip still passes. See the codec module's note.
//!
//! ## Regenerating
//!
//! The `v5Blobs` section of the corpus holds blobs **v5 produced**, so v4's
//! real decoder can be run over them (the cross-decode direction). It is
//! rewritten by the `#[ignore]`d regenerator in this file, which must run
//! BEFORE the oracle:
//!
//! When the CORPUS changes, run the `#[ignore]`d regenerator first (it rewrites
//! the committed `v5Blobs`, so it is deliberately outside the sweep's block):
//! `cargo test -p quilltap-harness --test text_compression_equivalence --
//! --ignored regenerate_v5_blobs --nocapture`, from the v5 checkout.
//!
//! Regenerate the oracle from the v4 CHECKOUT (Node 24): a `/tmp` pin never
//! survives the round that made it (the sweep driver's `stale_v4_pin_path`
//! refusal) — when the baseline is behind v4 HEAD the driver's `--v4 <pin>`
//! supplies the pin, not this header. `@/lib` resolves through the cwd's
//! tsconfig, so the `cd` is load-bearing.
//!
//! ```bash
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   npx tsx "$V5W/harness/oracle/cases/text-compression.ts" \
//!     "$V5W/harness/oracle/fixtures/text-compression.json" \
//!     > /tmp/oracle-text-compression.ndjson
//!   QT_ORACLE_TEXT_COMPRESSION=/tmp/oracle-text-compression.ndjson \
//!     cargo test -p quilltap-harness --test text_compression_equivalence -- --nocapture
//! ```
//!
//! With `QT_ORACLE_TEXT_COMPRESSION` unset the differential SKIPs (and says
//! so); the corpus self-checks below always run.

use std::collections::BTreeMap;
use std::path::PathBuf;

use quilltap_core::db::text_compression::{
    blob_to_text, is_compressed_text_blob, text_to_blob, TextCell,
};
use rusqlite::types::ValueRef;
use serde::Deserialize;
use serde_json::Value;

const ORACLE_VAR: &str = "QT_ORACLE_TEXT_COMPRESSION";

#[derive(Deserialize)]
struct Corpus {
    texts: Vec<TextRow>,
    #[serde(rename = "decodeShapes")]
    decode_shapes: Vec<ShapeRow>,
    #[serde(rename = "v5Blobs")]
    v5_blobs: Vec<V5BlobRow>,
}

#[derive(Deserialize, Clone)]
struct TextRow {
    label: String,
    text: String,
}

#[derive(Deserialize, Clone)]
struct ShapeRow {
    label: String,
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    hex: Option<String>,
    #[serde(default)]
    value: Option<f64>,
}

#[derive(Deserialize, Clone)]
struct V5BlobRow {
    label: String,
    hex: String,
}

fn fixture_path() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/quilltap-harness.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/text-compression.json")
}

fn load_corpus() -> Corpus {
    let raw = std::fs::read_to_string(fixture_path()).expect("the committed corpus must exist");
    serde_json::from_str(&raw).expect("the corpus must parse")
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("corpus hex must parse"))
        .collect()
}

/// v5's stored form for one corpus text, as `(decision, stored_bytes, hex)` —
/// the exact triple the oracle's `encode` row carries.
fn v5_encode(text: &str) -> (&'static str, usize, String) {
    match text_to_blob(text) {
        TextCell::Text(s) => ("string", s.len(), to_hex(s.as_bytes())),
        TextCell::Blob(b) => ("blob", b.len(), to_hex(&b)),
    }
}

/// Decode one corpus shape the way a SQLite cell of that shape would decode.
fn v5_decode_shape(shape: &ShapeRow) -> (bool, Option<String>) {
    match shape.kind.as_str() {
        // Both spell a NULL cell; better-sqlite3 hands v4 `null` for one and
        // `undefined` for an absent column, and v4's first arm covers both.
        "null" | "undefined" => (false, blob_to_text(ValueRef::Null)),
        "string" => {
            let t = shape.text.clone().unwrap_or_default();
            (false, blob_to_text(ValueRef::Text(t.as_bytes())))
        }
        "blob" => {
            let b = from_hex(shape.hex.as_deref().unwrap_or(""));
            (
                is_compressed_text_blob(&b),
                blob_to_text(ValueRef::Blob(&b)),
            )
        }
        "number" => {
            let v = shape.value.expect("a number shape carries a value");
            // v4's `String(value)` arm. An integral value arrives as an
            // INTEGER cell, a fractional one as REAL.
            let cell = if v.fract() == 0.0 && v.abs() < 9e15 {
                ValueRef::Integer(v as i64)
            } else {
                ValueRef::Real(v)
            };
            (false, blob_to_text(cell))
        }
        other => panic!("unknown corpus shape kind {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The corpus self-checks — always run, oracle or no oracle.
// ---------------------------------------------------------------------------

#[test]
fn the_committed_v5_blobs_are_still_what_v5_writes() {
    // The byte pin on v5's own side: if the codec ever changes what it emits,
    // this reddens BEFORE the oracle is consulted, and names the row.
    let corpus = load_corpus();
    assert!(
        !corpus.v5_blobs.is_empty(),
        "v5Blobs is empty — run the `regenerate_v5_blobs` ignored test"
    );
    let by_label: BTreeMap<&str, &TextRow> =
        corpus.texts.iter().map(|t| (t.label.as_str(), t)).collect();
    for row in &corpus.v5_blobs {
        let text = by_label
            .get(row.label.as_str())
            .unwrap_or_else(|| panic!("v5Blobs row {:?} has no matching text", row.label));
        let (_, _, hex) = v5_encode(&text.text);
        assert_eq!(
            hex, row.hex,
            "the committed v5 blob for {:?} is not what text_to_blob now writes",
            row.label
        );
    }
}

#[test]
fn every_corpus_text_round_trips_through_v5() {
    let corpus = load_corpus();
    for row in &corpus.texts {
        let stored = text_to_blob(&row.text);
        let got = match &stored {
            TextCell::Text(s) => blob_to_text(ValueRef::Text(s.as_bytes())),
            TextCell::Blob(b) => blob_to_text(ValueRef::Blob(b)),
        };
        assert_eq!(
            got.as_deref(),
            Some(row.text.as_str()),
            "round trip failed for {:?}",
            row.label
        );
        assert!(
            stored.stored_len() <= row.text.len().max(1),
            "the stored form of {:?} is larger than the plain string",
            row.label
        );
    }
}

// ---------------------------------------------------------------------------
// The differential.
// ---------------------------------------------------------------------------

#[test]
fn matches_the_v4_oracle_row_for_row() {
    let Ok(path) = std::env::var(ORACLE_VAR) else {
        println!("SKIP: {ORACLE_VAR} unset — set it to the oracle NDJSON to run the differential");
        return;
    };
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read the oracle at {path}: {e}"));
    let rows: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each oracle line is JSON"))
        .collect();
    assert!(!rows.is_empty(), "the oracle NDJSON at {path} is EMPTY");

    let corpus = load_corpus();
    let texts: BTreeMap<&str, &TextRow> =
        corpus.texts.iter().map(|t| (t.label.as_str(), t)).collect();
    let shapes: BTreeMap<&str, &ShapeRow> = corpus
        .decode_shapes
        .iter()
        .map(|s| (s.label.as_str(), s))
        .collect();
    let v5_blobs: BTreeMap<&str, &V5BlobRow> = corpus
        .v5_blobs
        .iter()
        .map(|b| (b.label.as_str(), b))
        .collect();

    let mut failures: Vec<String> = Vec::new();
    let mut seen = (0usize, 0usize, 0usize);

    for row in &rows {
        let kind = row["kind"].as_str().expect("every row has a kind");
        let label = row["label"]
            .as_str()
            .expect("every row has a label")
            .to_string();
        let mut check = |field: &str, got: String, want: &Value| {
            let want_s = match want {
                Value::Null => "<null>".to_string(),
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if got != want_s {
                failures.push(format!(
                    "[{kind}] {label}: {field}\n  v5: {got}\n  v4: {want_s}"
                ));
            }
        };

        match kind {
            "encode" => {
                seen.0 += 1;
                let text = texts
                    .get(label.as_str())
                    .unwrap_or_else(|| panic!("oracle encode row {label:?} is not in the corpus"));
                let (decision, stored_bytes, hex) = v5_encode(&text.text);
                check("decision", decision.to_string(), &row["decision"]);
                check("storedBytes", stored_bytes.to_string(), &row["storedBytes"]);
                check("storedHex", hex, &row["storedHex"]);
                // v4 records its own round trip; hold v5 to the same string.
                let got = match text_to_blob(&text.text) {
                    TextCell::Text(s) => blob_to_text(ValueRef::Text(s.as_bytes())),
                    TextCell::Blob(b) => blob_to_text(ValueRef::Blob(&b)),
                };
                check(
                    "roundTrip",
                    got.unwrap_or_else(|| "<null>".into()),
                    &row["roundTrip"],
                );
            }
            "crossDecode" => {
                seen.1 += 1;
                // v4's REAL decoder over v5's bytes. The oracle's `text` is
                // what v4 read; it must be the corpus text v5 encoded.
                let blob = v5_blobs.get(label.as_str()).unwrap_or_else(|| {
                    panic!("oracle crossDecode row {label:?} is not in v5Blobs")
                });
                let text = texts
                    .get(label.as_str())
                    .unwrap_or_else(|| panic!("crossDecode row {label:?} has no corpus text"));
                check("text", text.text.clone(), &row["text"]);
                check(
                    "isCompressed",
                    is_compressed_text_blob(&from_hex(&blob.hex)).to_string(),
                    &row["isCompressed"],
                );
            }
            "decode" => {
                seen.2 += 1;
                let shape = shapes
                    .get(label.as_str())
                    .unwrap_or_else(|| panic!("oracle decode row {label:?} is not in the corpus"));
                let (is_compressed, text) = v5_decode_shape(shape);
                check(
                    "isCompressed",
                    is_compressed.to_string(),
                    &row["isCompressed"],
                );
                check(
                    "text",
                    text.unwrap_or_else(|| "<null>".into()),
                    &row["text"],
                );
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    println!(
        "text_compression_equivalence: {} encode / {} crossDecode / {} decode rows compared",
        seen.0, seen.1, seen.2
    );
    assert_eq!(
        seen.0,
        corpus.texts.len(),
        "the oracle did not cover every corpus text"
    );
    assert_eq!(
        seen.1,
        corpus.v5_blobs.len(),
        "the oracle did not cover every v5 blob"
    );
    assert_eq!(
        seen.2,
        corpus.decode_shapes.len(),
        "the oracle did not cover every decode shape"
    );

    if !failures.is_empty() {
        panic!(
            "{} divergence(s) against v4's real text-compression.ts:\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

// ---------------------------------------------------------------------------
// The fixture step.
// ---------------------------------------------------------------------------

/// Rewrite the corpus's `v5Blobs` section from v5's own encoder, so the oracle
/// can run v4's REAL `blobToText` over bytes v5 produced. A fixture step, not a
/// check — it writes a committed file and asserts nothing.
#[test]
#[ignore = "a fixture step, not a check — see the module docs"]
fn regenerate_v5_blobs() {
    let path = fixture_path();
    let raw = std::fs::read_to_string(&path).expect("the corpus must exist");
    let mut doc: Value = serde_json::from_str(&raw).expect("the corpus must parse");
    let texts: Vec<TextRow> =
        serde_json::from_value(doc["texts"].clone()).expect("texts must parse");

    let blobs: Vec<Value> = texts
        .iter()
        .filter_map(|t| match text_to_blob(&t.text) {
            // Only the rows that actually COMPRESS: a `string` decision stores
            // plain UTF-8, which the `decode` rows already cover.
            TextCell::Blob(b) => Some(serde_json::json!({
                "label": t.label,
                "hex": to_hex(&b),
            })),
            TextCell::Text(_) => None,
        })
        .collect();

    let n = blobs.len();
    doc["v5Blobs"] = Value::Array(blobs);
    let mut out = serde_json::to_string_pretty(&doc).expect("the corpus must serialize");
    out.push('\n');
    std::fs::write(&path, out).expect("the corpus must be writable");
    println!(
        "regenerate_v5_blobs: wrote {n} v5 blobs to {}",
        path.display()
    );
}
