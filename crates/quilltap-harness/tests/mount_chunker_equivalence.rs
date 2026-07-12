//! P4.6v tier-1 differential: the mount-index pure leaves — `chunkDocument` /
//! `estimateTokens` (chunker), `normaliseRelativePath` / `detectNativeText` /
//! `mimeForExtension` (path-utils), and `fileOpStatus` (file-op-status) — exact
//! against v4's REAL `lib/mount-index/` code.
//!
//! Generate the oracle (from the v4 checkout):
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/mount-chunker.ts \
//!     > /tmp/oracle-mount-chunker.ndjson
//! Run:
//!   QT_ORACLE_MOUNT_CHUNKER=/tmp/oracle-mount-chunker.ndjson \
//!     cargo test -p quilltap-harness --test mount_chunker_equivalence

use quilltap_core::services::mount_index::chunker::{
    chunk_document, estimate_tokens, ChunkOptions,
};
use quilltap_core::services::mount_index::file_op_status::file_op_status_for_code;
use quilltap_core::services::mount_index::path_utils::{
    detect_native_text, mime_for_extension, normalise_relative_path,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct WireChunk {
    content: String,
    #[serde(rename = "tokenCount")]
    token_count: i64,
    #[serde(rename = "chunkIndex")]
    chunk_index: i64,
    #[serde(rename = "headingContext")]
    heading_context: Option<String>,
}

#[derive(Deserialize)]
struct WireOpts {
    #[serde(rename = "targetMinTokens")]
    target_min_tokens: Option<i64>,
    #[serde(rename = "targetMaxTokens")]
    target_max_tokens: Option<i64>,
    #[serde(rename = "overlapTokens")]
    overlap_tokens: Option<i64>,
}

#[test]
fn mount_chunker_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MOUNT_CHUNKER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MOUNT_CHUNKER to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 6]; // chunk, estimate, normalise, nativeText, mime, status
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        match kind {
            "chunk" => {
                let input = row["text"].as_str().unwrap();
                let opts = match row.get("options") {
                    Some(Value::Object(_)) => {
                        let w: WireOpts = serde_json::from_value(row["options"].clone()).unwrap();
                        ChunkOptions {
                            target_min_tokens: w.target_min_tokens,
                            target_max_tokens: w.target_max_tokens,
                            overlap_tokens: w.overlap_tokens,
                        }
                    }
                    _ => ChunkOptions::default(),
                };
                let expected: Vec<WireChunk> = serde_json::from_value(row["out"].clone()).unwrap();
                let got = chunk_document(input, opts);
                assert_eq!(got.len(), expected.len(), "chunk '{id}' length");
                for (i, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
                    assert_eq!(g.content, e.content, "chunk '{id}'[{i}] content");
                    assert_eq!(g.token_count, e.token_count, "chunk '{id}'[{i}] tokenCount");
                    assert_eq!(g.chunk_index, e.chunk_index, "chunk '{id}'[{i}] chunkIndex");
                    assert_eq!(
                        g.heading_context, e.heading_context,
                        "chunk '{id}'[{i}] headingContext"
                    );
                }
                counts[0] += 1;
            }
            "estimate" => {
                let out = row["out"].as_i64().unwrap();
                assert_eq!(
                    estimate_tokens(row["text"].as_str().unwrap()),
                    out,
                    "estimate '{id}'"
                );
                counts[1] += 1;
            }
            "normalise" => {
                let input = row["input"].as_str().unwrap();
                let got = normalise_relative_path(input);
                let out = &row["out"];
                if let Some(ok) = out.get("ok") {
                    assert_eq!(
                        got.as_ref().map(|s| s.as_str()).ok(),
                        ok.as_str(),
                        "normalise '{id}' ok"
                    );
                } else {
                    let code = out["err"].as_str().unwrap();
                    assert_eq!(
                        got.as_ref().err().map(|e| e.code.as_str()),
                        Some(code),
                        "normalise '{id}' err"
                    );
                }
                counts[2] += 1;
            }
            "nativeText" => {
                let p = row["path"].as_str().unwrap();
                let got = detect_native_text(p).map(|t| t.as_str().to_string());
                let expected = row["out"].as_str().map(str::to_string);
                assert_eq!(got, expected, "nativeText '{id}'");
                counts[3] += 1;
            }
            "mime" => {
                let p = row["path"].as_str().unwrap();
                assert_eq!(
                    mime_for_extension(p),
                    row["out"].as_str().unwrap(),
                    "mime '{id}'"
                );
                counts[4] += 1;
            }
            "status" => {
                let source = row["source"].as_str().unwrap();
                let code = row["code"].as_str().unwrap();
                let arg = if source == "Error" { None } else { Some(code) };
                assert_eq!(
                    file_op_status_for_code(arg) as i64,
                    row["out"].as_i64().unwrap(),
                    "status '{id}'"
                );
                counts[5] += 1;
            }
            other => panic!("unknown row kind {other}"),
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: mount-chunker matched oracle (counts {counts:?}).");
}
