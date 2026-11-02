//! Tier-2 differential test: the `roleplay_templates` repo (Phase-2).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-roleplay-templates-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `roleplay_templates` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! THE HEADLINE: the first array-of-objects JSON column (`renderingPatterns`) and
//! a nullable JSON-object column (`dialogueDetection`). Both store compact JSON
//! whose object key order is the Zod schema field order; the Rust port models the
//! elements with typed serde structs in schema order, so the stored text matches
//! v4 byte-for-byte (including omitted optional fields).
//!
//! P4.D341 (v4 `778b97b2b`, the narration codec): `narrationDelimiters` is now
//! JSON text on every write and decoded on every read. The corpus plants
//! pre-codec / legacy cells RAW (`rawCells`: a SQL-NULL narration, the TEXT
//! `null`, a raw `+`, a raw built-in `*`, a NULL `markdownAsterisks`, an
//! out-of-order `unmarkedStyle`), creates an unmarked-prose template (`null`
//! narration + an `unmarkedStyle` + the three new pattern scopes with an EMPTY
//! `className`), and updates rows WITHOUT naming the narration (the merged write
//! ENCODES the stored value). After the ops, `reads` compares v4's REAL
//! `findById` (`decoded()`) against v5's `find_full_json_by_id`, byte for byte.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-rt-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-roleplay-templates-fixture.ts
//!   QT_FIXTURE_RT=/tmp/qt-rt-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/roleplay-templates-tier2.ts \
//!     > /tmp/oracle-rt.ndjson
//! Run:
//!   QT_ORACLE_ROLEPLAY_TEMPLATES=/tmp/oracle-rt.ndjson \
//!   QT_FIXTURE_ROLEPLAY_TEMPLATES=/tmp/qt-rt-fixture.db \
//!     cargo test -p quilltap-harness --test roleplay_templates_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::roleplay_templates::{
    find_full_json_by_id, CreateOptions, DialogueDetection, RenderingPattern, RtCreate, RtUpdate,
    StringOrPair, TemplateDelimiter, UnmarkedStyle,
};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

/// The committed fixture spec — the single source driving both ports.
#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
    #[serde(default)]
    reads: Option<Reads>,
}

#[derive(Deserialize)]
struct Reads {
    ids: Vec<String>,
}

/// Absent → `None`; present (`null` included) → `Some(..)` — v4's
/// `'narrationDelimiters' in data` tri-state.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

fn default_true() -> bool {
    true
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
    user_id: Option<String>,
    name: String,
    description: Option<String>,
    #[serde(rename = "systemPrompt")]
    system_prompt: String,
    #[serde(rename = "isBuiltIn")]
    is_built_in: bool,
    tags: Vec<String>,
    delimiters: Vec<TemplateDelimiter>,
    #[serde(rename = "renderingPatterns")]
    rendering_patterns: Vec<RenderingPattern>,
    #[serde(rename = "dialogueDetection")]
    dialogue_detection: Option<DialogueDetection>,
    #[serde(rename = "narrationDelimiters")]
    narration_delimiters: Option<StringOrPair>,
    #[serde(rename = "unmarkedStyle", default)]
    unmarked_style: Option<UnmarkedStyle>,
    #[serde(rename = "markdownAsterisks", default = "default_true")]
    markdown_asterisks: bool,
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
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "systemPrompt")]
    system_prompt: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    delimiters: Option<Vec<TemplateDelimiter>>,
    #[serde(default, rename = "renderingPatterns")]
    rendering_patterns: Option<Vec<RenderingPattern>>,
    #[serde(default, rename = "dialogueDetection")]
    dialogue_detection: Option<DialogueDetection>,
    #[serde(default, rename = "narrationDelimiters", deserialize_with = "present")]
    narration_delimiters: Option<Option<StringOrPair>>,
    #[serde(default, rename = "unmarkedStyle", deserialize_with = "present")]
    unmarked_style: Option<Option<UnmarkedStyle>>,
    #[serde(default, rename = "markdownAsterisks")]
    markdown_asterisks: Option<bool>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/roleplay-templates-tier2.json")
}

#[test]
fn roleplay_templates_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_ROLEPLAY_TEMPLATES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ROLEPLAY_TEMPLATES to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_ROLEPLAY_TEMPLATES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_ROLEPLAY_TEMPLATES to the seed fixture .db (see header)."
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
    let oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    // Work on a fresh copy of the seed fixture so the shared file stays pristine.
    let scratch = tempfile::Builder::new()
        .prefix("qt-rt-rust-")
        .tempdir()
        .expect("tempdir");
    let work = scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.roleplay_templates();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => repo
                    .create(
                        &RtCreate {
                            user_id: data.user_id.clone(),
                            name: data.name.clone(),
                            description: data.description.clone(),
                            system_prompt: data.system_prompt.clone(),
                            is_built_in: data.is_built_in,
                            tags: data.tags.clone(),
                            delimiters: data.delimiters.clone(),
                            rendering_patterns: data.rendering_patterns.clone(),
                            dialogue_detection: data.dialogue_detection.clone(),
                            narration_delimiters: data.narration_delimiters.clone(),
                            unmarked_style: data.unmarked_style.clone(),
                            markdown_asterisks: data.markdown_asterisks,
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("roleplay_templates.create"),
                Op::Update { id, data } => {
                    let found = repo
                        .update(
                            id,
                            &RtUpdate {
                                name: data.name.clone(),
                                description: data.description.clone(),
                                system_prompt: data.system_prompt.clone(),
                                tags: data.tags.clone(),
                                delimiters: data.delimiters.clone(),
                                rendering_patterns: data.rendering_patterns.clone(),
                                dialogue_detection: data.dialogue_detection.clone(),
                                narration_delimiters: data.narration_delimiters.clone(),
                                unmarked_style: data.unmarked_style.clone(),
                                markdown_asterisks: data.markdown_asterisks,
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("roleplay_templates.update");
                    assert!(found, "update target {id} not found in fixture");
                }
                Op::Delete { id } => {
                    let found = repo.delete(id).expect("roleplay_templates.delete");
                    assert!(found, "delete target {id} not found in fixture");
                }
            }
        }
    }

    let got = writer
        .dump_table_json("roleplay_templates", "id")
        .expect("dump roleplay_templates");

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
    eprintln!("OK: roleplay_templates tier-2 matched oracle ({n} rows).");

    // P4.D341: the decoded reads, byte for byte (key order included — a
    // `serde_json::Map` `==` is order-blind under preserve_order).
    let want_reads = oracle["reads"].as_array().expect("oracle reads[]");
    let ids = spec.reads.as_ref().map(|r| r.ids.as_slice()).unwrap_or(&[]);
    assert_eq!(want_reads.len(), ids.len(), "read count");
    assert!(!ids.is_empty(), "no reads in the corpus");
    let mut diverged = Vec::new();
    for (id, want) in ids.iter().zip(want_reads) {
        assert_eq!(want["id"].as_str(), Some(id.as_str()), "read order");
        let got = find_full_json_by_id(writer.connection(), id)
            .expect("find_full_json_by_id")
            .map(|v| serde_json::to_string(&v).expect("serialize read"));
        let want = want["json"].as_str().map(str::to_string);
        if got != want {
            diverged.push(format!("  {id}\n    rust:   {got:?}\n    oracle: {want:?}"));
        }
    }
    assert!(
        diverged.is_empty(),
        "{} decoded read(s) diverged:\n{}",
        diverged.len(),
        diverged.join("\n")
    );
    eprintln!(
        "OK: roleplay_templates decoded reads matched oracle ({} ids).",
        ids.len()
    );
}
