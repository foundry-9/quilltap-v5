//! Tier-2 differential test #4: the `prompt_templates` repo (Phase-2, repo #4
//! after `folders`, `tags`, `text_replacement_rules`).
//!
//! Structural DB diff. Both sides start from the SAME seed fixture (built by
//! harness/oracle/fixtures/build-prompt-templates-fixture.ts), run the SAME
//! create / update / delete op sequence from the committed spec, dump the
//! `prompt_templates` table canonically, and assert the post-op state is
//! identical. Ids and timestamps are pinned on both sides, so the dumps must
//! match with zero normalization.
//!
//! Beyond `text_replacement_rules` this exercises the first JSON ARRAY column
//! (`tags` -> compact JSON text), several nullable string columns, and the
//! built-in READ-ONLY GUARD. Two ops are flagged `expectNoop`: an update and a
//! delete that both target the built-in seed row. Both sides assert the op
//! reported not-modified (Rust: `Ok(false)`; oracle: `update -> null` /
//! `delete -> false`), AND the final-state dump confirms the built-in row was
//! left byte-identical (a port missing the guard would have changed/removed it).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-prompt-templates-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-prompt-templates-fixture.ts
//!   QT_FIXTURE_PROMPT_TEMPLATES=/tmp/qt-prompt-templates-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/prompt-templates-tier2.ts \
//!     > /tmp/oracle-prompt-templates.ndjson
//! Run:
//!   QT_ORACLE_PROMPT_TEMPLATES=/tmp/oracle-prompt-templates.ndjson \
//!   QT_FIXTURE_PROMPT_TEMPLATES=/tmp/qt-prompt-templates-fixture.db \
//!     cargo test -p quilltap-harness --test prompt_templates_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::prompt_templates::{
    CreateOptions, PromptTemplatesRepository, PtCreate, PtUpdate,
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
    Update {
        #[serde(default, rename = "expectNoop")]
        expect_noop: bool,
        id: String,
        data: UpdateData,
    },
    #[serde(rename = "delete")]
    Delete {
        #[serde(default, rename = "expectNoop")]
        expect_noop: bool,
        id: String,
    },
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "userId")]
    user_id: Option<String>,
    name: String,
    content: String,
    description: Option<String>,
    #[serde(rename = "isBuiltIn")]
    is_built_in: bool,
    category: Option<String>,
    #[serde(rename = "modelHint")]
    model_hint: Option<String>,
    tags: Vec<String>,
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
    content: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default, rename = "modelHint")]
    model_hint: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/prompt-templates-tier2.json")
}

#[test]
fn prompt_templates_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PROMPT_TEMPLATES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_PROMPT_TEMPLATES to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_PROMPT_TEMPLATES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_PROMPT_TEMPLATES to the seed fixture .db (see test header)."
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
    let work = std::env::temp_dir().join(format!(
        "qt-prompt-templates-rust-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo: PromptTemplatesRepository = writer.prompt_templates();
        for op in &spec.ops {
            match op {
                Op::Create { data, options } => repo
                    .create(
                        &PtCreate {
                            user_id: data.user_id.clone(),
                            name: data.name.clone(),
                            content: data.content.clone(),
                            description: data.description.clone(),
                            is_built_in: data.is_built_in,
                            category: data.category.clone(),
                            model_hint: data.model_hint.clone(),
                            tags: data.tags.clone(),
                        },
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("prompt_templates.create"),
                Op::Update {
                    expect_noop,
                    id,
                    data,
                } => {
                    let modified = repo
                        .update(
                            id,
                            &PtUpdate {
                                name: data.name.clone(),
                                content: data.content.clone(),
                                description: data.description.clone(),
                                category: data.category.clone(),
                                model_hint: data.model_hint.clone(),
                                tags: data.tags.clone(),
                                updated_at: data.updated_at.clone(),
                            },
                        )
                        .expect("prompt_templates.update");
                    if *expect_noop {
                        assert!(!modified, "expectNoop update {id} modified a built-in row");
                    } else {
                        assert!(modified, "update target {id} not found / not modified");
                    }
                }
                Op::Delete { expect_noop, id } => {
                    let removed = repo.delete(id).expect("prompt_templates.delete");
                    if *expect_noop {
                        assert!(!removed, "expectNoop delete {id} removed a built-in row");
                    } else {
                        assert!(removed, "delete target {id} not found");
                    }
                }
            }
        }
    }

    let got = writer
        .dump_table_json("prompt_templates", "id")
        .expect("dump prompt_templates");

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
    eprintln!("OK: prompt_templates tier-2 matched oracle ({n} rows).");
}
