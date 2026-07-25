//! P4.9G4 `.qtap` IMPORT-READ differential: replays the oracle's OWN `.qtap`
//! bytes (emitted as `qtapBase64`, so both sides provably read identical input)
//! through the Rust reader (`services::quilltap_import::ndjson::
//! load_qtap_from_upload`) and the Rust `preview_import`, diffing:
//!
//!   - the assembled `{manifest, data}` — **exact**, key order included
//!   - the `previewImport` body — **exact**, key order included
//!   - the reader's throwing arms — v4's error strings, verbatim
//!
//! Nothing here writes: the preview is v4's count-only pass. The import EXECUTE
//! half is this lane's named deferral (see the order's status header).
//!
//! Generate the oracle (see the .test.ts header), then run:
//!   QT_ORACLE_SYSTEM_IMPORT=/tmp/oracle-system-import.ndjson \
//!     cargo test -p quilltap-harness --test system_import_equivalence -- --nocapture

use std::path::PathBuf;

use base64::Engine;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::quilltap_import::ndjson::load_qtap_from_upload;
use quilltap_core::services::quilltap_import::preview::preview_import;
use quilltap_core::services::quilltap_import::QuilltapExport;
use serde_json::{Map, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// Oracle cases where v5 deliberately does NOT match v4, each one ruled and
/// recorded. Exactly one entry: v4's sparse-array `every` finalizes a
/// multi-chunk blob on its first chunk, so v4 cannot re-import a document-store
/// blob larger than its own 3 MB `BLOB_CHUNK_BYTES`. v5 requires every chunk
/// (reader-only; the writer stays byte-identical). Ruled 2026-07-24 — the
/// rationale is in `ndjson.rs` and the status log.
const EXPECTED_DIVERGENCES: &[&str] = &["throw_ndjson_truncated_blob"];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-sysimport-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("system-data-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("system-data-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

fn decode(v: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(v.as_str().expect("qtapBase64 is a string"))
        .expect("qtapBase64 decodes")
}

/// The assembled export as one `{manifest, data}` object, so the diff covers
/// both halves and their key order in a single comparison.
fn as_value(export: &QuilltapExport) -> Value {
    let mut m = Map::new();
    m.insert("manifest".into(), export.manifest.clone());
    m.insert("data".into(), export.data.clone());
    Value::Object(m)
}

/// Mask the JSON-ENGINE's own parse message inside v4's wrapper. v4 embeds V8's
/// text (`Unexpected token 'o', "not json" is not valid JSON`) where serde emits
/// its own (`expected ident at line 1 column 2`) — the standing
/// "`is not valid JSON:` wording seam", out of contract for every ported
/// surface. v4's WRAPPER (`Invalid NDJSON on line N: … (near: X)` /
/// `Invalid JSON: …`) is fully under test; only the engine's sentence is masked.
fn mask_parser_text(msg: &str) -> String {
    if let Some(rest) = msg.strip_prefix("Invalid JSON: ") {
        let _ = rest;
        return "Invalid JSON: <PARSER>".to_string();
    }
    if let Some(head) = msg.find("Invalid NDJSON on ") {
        // Keep everything up to the first `: ` after the line number, and the
        // trailing ` (near: …)` excerpt — mask only what lies between.
        let after = &msg[head..];
        if let (Some(colon), Some(near)) = (after.find(": "), after.rfind(" (near: ")) {
            if colon + 2 <= near {
                return format!("{}: <PARSER>{}", &after[..colon], &after[near..]);
            }
        }
    }
    msg.to_string()
}

#[test]
fn system_import_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SYSTEM_IMPORT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SYSTEM_IMPORT to the oracle NDJSON (see test header).");
            return;
        }
    };
    let lines: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("parse oracle line"))
        .collect();

    let meta = lines
        .iter()
        .find(|l| l["name"] == "_meta")
        .expect("oracle carries _meta");
    let user = meta["userId"].as_str().unwrap().to_string();

    let db = fresh_db("reads");
    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;

    for exp in lines.iter().filter(|l| l["name"] != "_meta") {
        let name = exp["name"].as_str().unwrap();
        match exp["kind"].as_str().unwrap_or("") {
            // Export bytes → reader → preview, both diffed exactly.
            "roundtrip" | "read" | "preview" => {
                let export = if exp["kind"] == "preview" {
                    // The hand-built payload arm: no `.qtap` bytes, just the object.
                    let p = &exp["payload"];
                    QuilltapExport {
                        manifest: p["manifest"].clone(),
                        data: p["data"].clone(),
                    }
                } else {
                    match load_qtap_from_upload(&decode(&exp["qtapBase64"])) {
                        Ok(e) => e,
                        Err(msg) => {
                            failed.push(format!("{name}: rust reader threw: {msg}"));
                            continue;
                        }
                    }
                };

                if let Some(expected) = exp.get("assembled").filter(|v| !v.is_null()) {
                    let got = as_value(&export);
                    let mut want = Map::new();
                    want.insert("manifest".into(), expected["manifest"].clone());
                    want.insert("data".into(), expected["data"].clone());
                    let want = Value::Object(want);
                    if got != want {
                        failed.push(format!(
                            "{name}: assembled export differs\n  rust:   {got}\n  oracle: {want}"
                        ));
                        continue;
                    }
                }

                if let Some(expected) = exp.get("preview").filter(|v| !v.is_null()) {
                    let got = db
                        .read_main(|main| {
                            db.read_mount_index(|mount| preview_import(main, mount, &user, &export))
                        })
                        .expect("preview read");
                    if got != *expected {
                        failed.push(format!(
                            "{name}: preview differs\n  rust:   {got}\n  oracle: {expected}"
                        ));
                        continue;
                    }
                }
            }
            "throw" => {
                let got = load_qtap_from_upload(&decode(&exp["qtapBase64"]));
                match exp["error"].as_str() {
                    // A `null` error means v4 did NOT throw. The ONLY such case is
                    // the short multi-chunk blob, where v4's sparse-array `every`
                    // finalizes on the first chunk and silently truncates the blob
                    // — the RULED divergence (see the note in `ndjson.rs`). v5 must
                    // throw here, with v4's own end-of-stream truncation message,
                    // so the divergence stays asserted in both directions rather
                    // than merely tolerated.
                    None => {
                        if !EXPECTED_DIVERGENCES.contains(&name) {
                            failed.push(format!(
                                "{name}: oracle did not throw and the case is not a \
                                 recorded divergence — add it to EXPECTED_DIVERGENCES \
                                 with a ruling, or fix the reader"
                            ));
                            continue;
                        }
                        match got {
                            Err(msg)
                                if msg.starts_with(
                                    "NDJSON export truncated: 1 blob(s) missing chunks — ",
                                ) => {}
                            other => {
                                failed.push(format!(
                                    "{name}: expected the ruled truncation divergence, got {other:?}"
                                ));
                                continue;
                            }
                        }
                    }
                    Some(expected) => match got {
                        Err(msg) if mask_parser_text(&msg) == mask_parser_text(expected) => {}
                        other => {
                            failed.push(format!(
                                "{name}: expected throw {expected:?}, got {other:?}"
                            ));
                            continue;
                        }
                    },
                }
            }
            other => {
                failed.push(format!("{name}: unknown oracle kind {other}"));
                continue;
            }
        }
        ran += 1;
        eprintln!("OK {name}");
    }

    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed.join("\n\n")
    );
    assert_eq!(ran, 19, "expected 19 cases to run, ran {ran}");
}
