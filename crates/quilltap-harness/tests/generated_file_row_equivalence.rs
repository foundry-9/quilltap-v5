//! Tier-2 differential for the generated `files` row (P4.D283, v4 `9ecebf986`):
//! `quilltap_core::files::generated_file_row::create_generated_file_row` vs v4's
//! REAL `createGeneratedFileRow` (`lib/files/generated-file-row.ts`) — the ONE
//! row every stored picture gets (the avatar job, the Lantern, the tool, a
//! wardrobe item's picture, the avatar preview), with the bridge's stored type
//! and size, the measured dimensions, the caller's id when given, and NO label
//! in `description` (bug 132).
//!
//! Per case both sides take a FRESH copy of the wardrobe-item-image-generation
//! fixture's MAIN db, mint the row from the corpus's input
//! (`harness/oracle/fixtures/generated-file-row.json`) and compare the `files`
//! row by id (ISO timestamps and a minted id placeholdered) plus the
//! `[GeneratedFileRow] Created image file row` DEBUG (v4's bag in order; the
//! `linkedTo` array through the `…Json` file-layer convention). The helper runs
//! on the caller's thread here (inside a job it runs on the writer thread,
//! invisible to the capture rig — this family is where its line is pinned).
//!
//! Regenerate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-gfr-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/generated-file-row.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/generated-file-row.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WIIG_MAIN=/tmp/qt-gfr-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-gfr-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-image-generation-fixture.ts
//!   QT_FIXTURE_GFR_MAIN=/tmp/qt-gfr-main.db QT_ORACLE_OUT=/tmp/oracle-generated-file-row.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/generated-file-row\.test\.ts$"
//!
//! Run:
//!   QT_ORACLE_GENERATED_FILE_ROW=/tmp/oracle-generated-file-row.ndjson \
//!   QT_FIXTURE_GFR_MAIN=/tmp/qt-gfr-main.db \
//!     cargo test -p quilltap-harness --test generated_file_row_equivalence

mod wardrobe_images_support;

use std::collections::HashMap;
use std::path::Path;

use quilltap_core::db::Writer;
use quilltap_core::files::generated_file_row::{
    create_generated_file_row, GeneratedFileRowInput, GenerationRecord, StoredImageBytes,
};
use quilltap_core::test_support::captured_with;
use serde_json::{json, Value};
use wardrobe_images_support::{oracle_lines, rust_lines};

const LOG_PREFIXES: [&str; 1] = ["[GeneratedFileRow]"];
const UUID_RE: &str = r"[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const ISO_RE: &str = r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z";

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// A row/line normalizer: every ISO timestamp → `<ts>`; a MINTED id (the one
/// case without a pinned id) → `<minted>`.
fn normalize(s: &str, minted: Option<&str>) -> String {
    let mut out = regex::Regex::new(ISO_RE)
        .unwrap()
        .replace_all(s, "<ts>")
        .into_owned();
    if let Some(m) = minted {
        out = out.replace(m, "<minted>");
    }
    out
}

fn normalize_value(v: &Value, minted: Option<&str>) -> Value {
    match v {
        Value::String(s) => Value::String(normalize(s, minted)),
        Value::Array(a) => Value::Array(a.iter().map(|x| normalize_value(x, minted)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), normalize_value(x, minted)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn generated_file_row_matches_oracle() {
    let (Ok(oracle_path), Ok(main_fixture)) = (
        std::env::var("QT_ORACLE_GENERATED_FILE_ROW"),
        std::env::var("QT_FIXTURE_GFR_MAIN"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_GENERATED_FILE_ROW + QT_FIXTURE_GFR_MAIN (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/generated-file-row.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let user_id = spec["userId"].as_str().unwrap().to_string();
    let now = spec["now"].as_str().unwrap().to_string();
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    let cases = spec["cases"].as_array().unwrap();
    assert_eq!(oracle.len(), cases.len(), "oracle case count != corpus");
    let uuid_re = regex::Regex::new(&format!("^{UUID_RE}$")).unwrap();

    let mut failures: Vec<String> = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let want = &oracle[name];
        let input = &case["input"];
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-gfr-rust-{name}-"))
            .tempdir()
            .unwrap();
        let mw = scratch.path().join("main.db");
        std::fs::copy(&main_fixture, &mw).unwrap();
        let main = Writer::open_writable(&mw, &pepper).unwrap();

        let stored = StoredImageBytes {
            storage_key: input["stored"]["storageKey"].as_str().unwrap().to_string(),
            stored_mime_type: input["stored"]["storedMimeType"]
                .as_str()
                .unwrap()
                .to_string(),
            size_bytes: input["stored"]["sizeBytes"].as_i64().unwrap(),
        };
        let generation = input.get("generation").map(|g| GenerationRecord {
            prompt: g["prompt"].as_str().map(str::to_string),
            model: g["model"].as_str().map(str::to_string),
            revised_prompt: g["revisedPrompt"].as_str().map(str::to_string),
        });
        let pinned_id = input["id"].as_str();
        let (result, lines) = captured_with(|| {
            create_generated_file_row(
                main.connection(),
                &GeneratedFileRowInput {
                    user_id: &user_id,
                    id: pinned_id,
                    sha256: input["sha256"].as_str().unwrap(),
                    original_filename: input["originalFilename"].as_str().unwrap(),
                    stored: &stored,
                    width: input.get("width").and_then(Value::as_f64),
                    height: input.get("height").and_then(Value::as_f64),
                    linked_to: strings(input.get("linkedTo")),
                    tags: strings(input.get("tags")),
                    source: input.get("source").and_then(Value::as_str),
                    generation: generation.as_ref(),
                    generation_key: input.get("generationKey").and_then(Value::as_str),
                    description: input.get("description").and_then(Value::as_str),
                    project_id: input.get("projectId").and_then(Value::as_str),
                    folder_path: input.get("folderPath").and_then(Value::as_str),
                    now_iso: &now,
                },
            )
        });
        let (returned_id, threw) = match &result {
            Ok(row) => (row.id.clone(), Value::Null),
            Err(e) => (String::new(), json!(e.to_string())),
        };
        let minted =
            (pinned_id.is_none() && !returned_id.is_empty()).then_some(returned_id.as_str());
        let row = {
            let dumped = wardrobe_images_support::dump(main.connection(), "files");
            dumped["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"].as_str() == Some(returned_id.as_str()))
                .cloned()
                .unwrap_or(Value::Null)
        };
        let got = json!({
            "returnedId": if pinned_id.is_some() { json!(returned_id) } else if returned_id.is_empty() { json!("") } else { json!("<minted>") },
            "mintedIdIsUuid": if pinned_id.is_some() { Value::Null } else { json!(uuid_re.is_match(&returned_id)) },
            "threw": threw,
            "row": normalize_value(&row, minted),
            "logs": rust_lines(&lines, &LOG_PREFIXES).iter().map(|l| normalize(l, minted)).collect::<Vec<_>>(),
        });
        let want_minted = want["returnedId"].as_str() == Some("<minted>");
        let want_rows_id = want["row"]["id"].as_str().map(str::to_string);
        let want_s = json!({
            "returnedId": want["returnedId"],
            "mintedIdIsUuid": want["mintedIdIsUuid"],
            "threw": want["threw"],
            "row": normalize_value(&want["row"], if want_minted { want_rows_id.as_deref() } else { None }),
            "logs": oracle_lines(&want["logs"]).iter().map(|l| normalize(l, if want_minted { want_rows_id.as_deref() } else { None })).collect::<Vec<_>>(),
        });
        for part in ["returnedId", "mintedIdIsUuid", "threw", "row", "logs"] {
            if got[part] != want_s[part] {
                failures.push(format!(
                    "{name}: {part} diverged\n  rust:   {}\n  oracle: {}",
                    got[part], want_s[part]
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
