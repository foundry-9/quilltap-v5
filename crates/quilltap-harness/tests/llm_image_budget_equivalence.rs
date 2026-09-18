//! Tier-1 differential: v4's REAL `shrinkImageForLlmTransport`
//! (`lib/files/llm-image-budget.ts`, NEW at `bcd7e4852`, bug 151) against
//! `quilltap_core::files::llm_image_budget::shrink_image_for_llm_transport`.
//!
//! ## Why a scripted encoder is the right instrument here
//!
//! The function is a DECISION over a pixel op, and the two implementations'
//! pixel ops cannot be byte-compared: v4 encodes through sharp, v5 through
//! libwebp, and D19 says the operation is ported while encoded byte parity is
//! neither required nor reachable. A differential over real encoders would
//! compare nothing but the encoders.
//!
//! So both sides run the **same per-case script** below a deterministic
//! encoder — v4 through a `jest.doMock('sharp')` built by
//! `harness/oracle/lib/shrink-script.ts`, v5 through
//! [`quilltap_harness::scripted_transcoder::ScriptedTranscoder`] — and what the
//! diff proves is the decision: the arm order, the ceiling, which rungs are
//! asked for and in what order, which encode becomes `best`, when a grown
//! encode is discarded, and what a throw does to a partial ladder. Because the
//! scripts agree, the RESULT BYTES agree, so `buffer` is a real comparand.
//! The real encoders are proven separately against their own contracts: v4's by
//! `__tests__/unit/lib/files/llm-image-budget.test.ts`, v5's by
//! `HostImageCodec`'s tests, which assert v4's own 683x1024 / 1024x683.
//!
//! ## Comparands
//!
//! `wasShrunk`, `mimeType`, `originalSize`, `finalSize`, `width`, `height`, the
//! result BYTES (as a sha256 over the whole buffer, plus the verbatim base64
//! for buffers at or under 4096 bytes — four cases carry ~400 KB originals,
//! because the ceiling is 500 KiB of base64 and nothing smaller can exceed it,
//! and their base64 would add megabytes to a file both sides read whole), the
//! recorded `(maxEdge, quality)` ladder, and **the log line** — level, message
//! and every field of v4's bag, which is the only way `ceiling` and the
//! `${w}x${h}` dimension strings become comparands instead of transcriptions.
//!
//! ## The two recorded narrowings
//!
//! 1. **Field CASE.** v4's bags are camelCase (`originalSize`); v5's `tracing`
//!    fields are snake_case (`original_size`), which is this repo's standing
//!    convention for every ported log bag (`provider_failover.rs`'s
//!    `chat_id`/`response_length` against v4's `chatId`/`responseLength`) and
//!    reaches `combined.log` verbatim, since the P4.49 file layer inserts field
//!    names unchanged. Pre-existing and repo-wide; [`SNAKE`] maps them here
//!    rather than inventing a one-off camelCase site.
//! 2. **An ABSENT field.** `JSON.stringify` drops an `undefined` bag value, so
//!    v4's `provider_absent` bag has no `provider` key at all; `tracing` has no
//!    conditional field, so v5 renders it EMPTY. Asserted by name on that row
//!    (see `expect_absent_renders_empty`). Unreachable from production: both
//!    loaders gate the whole call on a provider being present and always have a
//!    filename.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout; stage the case OUTSIDE
//! any `.claude/` path — v4's jest ignores `/\.claude/`). While v4 HEAD is past
//! the oracle baseline this needs a PINNED worktree; that is the sweep driver's
//! `--v4`, never a path in this header.
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; TMPO=/tmp/qt-oracle-run-llmib
//!   cd ~/source/quilltap-server
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
//!   cp "$V5W/harness/oracle/cases/llm-image-budget.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/llm-image-budget.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/lib/shrink-script.ts" "$TMPO/lib/"
//!   QT_ORACLE_OUT=/tmp/oracle-llm-image-budget.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- cases/llm-image-budget
//! Run:
//!   QT_ORACLE_LLM_IMAGE_BUDGET=/tmp/oracle-llm-image-budget.ndjson \
//!     cargo test -p quilltap-harness --test llm_image_budget_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use quilltap_core::files::llm_image_budget::shrink_image_for_llm_transport;
use quilltap_harness::scripted_transcoder::{
    encoded_bytes, original_bytes, RecordedCall, ScriptMetadata, ScriptedTranscoder, ShrinkScript,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The camelCase-to-snake_case field map for the log-bag comparand — narrowing
/// (1) in the module docs. Every key v4 puts in either bag appears here; a new
/// key v4 adds fails loudly rather than being silently skipped.
const SNAKE: &[(&str, &str)] = &[
    ("module", "module"),
    ("filename", "filename"),
    ("provider", "provider"),
    ("mimeType", "mime_type"),
    ("error", "error"),
    ("originalSize", "original_size"),
    ("finalSize", "final_size"),
    ("originalDimensions", "original_dimensions"),
    ("finalDimensions", "final_dimensions"),
    ("ceiling", "ceiling"),
];

// ===========================================================================
// The corpus
// ===========================================================================

#[derive(Deserialize)]
struct Spec {
    cases: Vec<Case>,
}

#[derive(Deserialize, Clone)]
struct Case {
    label: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    provider: Option<String>,
    filename: Option<String>,
    #[serde(rename = "originalLen")]
    original_len: usize,
    script: CaseScript,
    /// A DECLARED ladder divergence: the rungs v5 asks for where they differ
    /// from v4's, with the reason. Only the `undecodable` row carries one (v4
    /// throws at the probe and never reaches the ladder; v5's probe cannot
    /// throw). Asserted to genuinely differ, so a converging mechanism retires
    /// the override loudly instead of rotting.
    #[serde(rename = "v5Calls", default)]
    v5_calls: Option<Vec<CaseCall>>,
    #[serde(rename = "v5CallsWhy", default)]
    v5_calls_why: Option<String>,
}

#[derive(Deserialize, Clone)]
struct CaseCall {
    #[serde(rename = "maxEdge")]
    max_edge: i64,
    quality: i64,
}

#[derive(Deserialize, Clone)]
struct CaseScript {
    /// `{"width": n|null, "height": n|null}` or the string `"throws"`.
    metadata: Value,
    #[serde(rename = "metadataThrowMessage", default)]
    metadata_throw_message: Option<String>,
    #[serde(rename = "final")]
    final_dims: Dims,
    steps: Vec<Step>,
}

#[derive(Deserialize, Clone)]
struct Dims {
    width: Option<i64>,
    height: Option<i64>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum Step {
    Ok { len: usize },
    Throw { throw: String },
}

impl CaseScript {
    fn to_script(&self) -> ShrinkScript {
        let metadata = if self.metadata.as_str() == Some("throws") {
            ScriptMetadata::Throws(
                self.metadata_throw_message
                    .clone()
                    .unwrap_or_else(|| "scripted metadata failure".to_string()),
            )
        } else {
            ScriptMetadata::Dims(
                self.metadata.get("width").and_then(Value::as_i64),
                self.metadata.get("height").and_then(Value::as_i64),
            )
        };
        ShrinkScript {
            metadata,
            final_dims: (self.final_dims.width, self.final_dims.height),
            steps: self
                .steps
                .iter()
                .map(|s| match s {
                    Step::Ok { len } => Ok(*len),
                    Step::Throw { throw } => Err(throw.clone()),
                })
                .collect(),
        }
    }
}

// ===========================================================================
// The oracle rows
// ===========================================================================

#[derive(Deserialize)]
struct OracleRow {
    label: String,
    result: OracleResult,
    calls: Vec<CaseCall>,
    log: Option<OracleLog>,
}

#[derive(Deserialize)]
struct OracleResult {
    #[serde(rename = "wasShrunk")]
    was_shrunk: bool,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "originalSize")]
    original_size: usize,
    #[serde(rename = "finalSize")]
    final_size: usize,
    width: Option<i64>,
    height: Option<i64>,
    #[serde(rename = "bufferSha256")]
    buffer_sha256: String,
    #[serde(rename = "bufferLen")]
    buffer_len: usize,
    #[serde(rename = "bufferBase64")]
    buffer_base64: Option<String>,
}

#[derive(Deserialize)]
struct OracleLog {
    level: String,
    message: String,
    bag: serde_json::Map<String, Value>,
}

// ===========================================================================
// Helpers
// ===========================================================================

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/llm-image-budget.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// v4's bag value as the bytes `tracing` renders for the same value: numbers
/// and strings both go through `Display`, so this is `to_string()` without
/// JSON's quotes.
fn rendered(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ===========================================================================
// The test
// ===========================================================================

#[test]
fn llm_image_budget_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_LLM_IMAGE_BUDGET") else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let mut rows: HashMap<String, OracleRow> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: OracleRow = serde_json::from_str(line).expect("oracle line parses");
        rows.insert(row.label.clone(), row);
    }

    // A stale-oracle floor: the corpus and the oracle must describe the same
    // set, and the set must not shrink below what this port was written against
    // (the order's count guard).
    assert!(
        rows.len() >= 12,
        "the oracle carries {} cases; the family needs at least 12 — regenerate it",
        rows.len()
    );
    assert_eq!(
        rows.len(),
        spec.cases.len(),
        "the oracle has {} cases and the corpus {} — one side is stale",
        rows.len(),
        spec.cases.len()
    );

    for c in &spec.cases {
        let want = rows
            .get(&c.label)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.label));

        let original = original_bytes(c.original_len);
        let transcoder =
            ScriptedTranscoder::new().with_script(original.clone(), c.script.to_script());

        let got = shrink_image_for_llm_transport(
            &transcoder,
            &original,
            &c.mime_type,
            c.provider.as_deref(),
            c.filename.as_deref(),
        );

        // --- the result fields ---
        assert_eq!(
            got.was_shrunk, want.result.was_shrunk,
            "[{}] wasShrunk",
            c.label
        );
        assert_eq!(
            got.mime_type, want.result.mime_type,
            "[{}] mimeType",
            c.label
        );
        assert_eq!(
            got.original_size, want.result.original_size,
            "[{}] originalSize",
            c.label
        );
        assert_eq!(
            got.final_size, want.result.final_size,
            "[{}] finalSize",
            c.label
        );
        assert_eq!(got.width, want.result.width, "[{}] width", c.label);
        assert_eq!(got.height, want.result.height, "[{}] height", c.label);

        // --- the result BYTES ---
        assert_eq!(
            got.buffer.len(),
            want.result.buffer_len,
            "[{}] buffer length",
            c.label
        );
        assert_eq!(
            sha256_hex(&got.buffer),
            want.result.buffer_sha256,
            "[{}] buffer bytes (sha256)",
            c.label
        );
        if let Some(b64) = &want.result.buffer_base64 {
            assert_eq!(
                base64::engine::general_purpose::STANDARD.encode(&got.buffer),
                *b64,
                "[{}] buffer bytes (base64)",
                c.label
            );
        }
        // v4's identity contract on an unchanged arm (`result.buffer === buffer`,
        // asserted oracle-side where identity is observable): v5 clones, so the
        // comparand is byte equality with the INPUT.
        if !got.was_shrunk {
            assert_eq!(
                got.buffer, original,
                "[{}] an unchanged result must carry the input bytes",
                c.label
            );
            assert_eq!(
                got.final_size, got.original_size,
                "[{}] an unchanged result's sizes must agree",
                c.label
            );
            assert_eq!(
                (got.width, got.height),
                (None, None),
                "[{}] an unchanged result omits width/height",
                c.label
            );
        }

        // --- the ladder ---
        let want_calls: Vec<RecordedCall> = match &c.v5_calls {
            Some(overridden) => {
                let v4: Vec<RecordedCall> = want
                    .calls
                    .iter()
                    .map(|c| RecordedCall {
                        max_edge: c.max_edge,
                        quality: c.quality,
                    })
                    .collect();
                let v5: Vec<RecordedCall> = overridden
                    .iter()
                    .map(|c| RecordedCall {
                        max_edge: c.max_edge,
                        quality: c.quality,
                    })
                    .collect();
                // The override must still be EARNING its keep: if v4 grew a
                // probe that cannot throw, or v5 gained a fallible one, the two
                // ladders converge and this row should be a plain equality.
                assert_ne!(
                    v4,
                    v5,
                    "[{}] declares a v5Calls override ({}) but v4 now asks for the SAME \
                     ladder — the mechanism converged; retire the override",
                    c.label,
                    c.v5_calls_why.as_deref().unwrap_or("no reason recorded")
                );
                assert!(
                    c.v5_calls_why.is_some(),
                    "[{}] a v5Calls override needs its reason in the corpus",
                    c.label
                );
                v5
            }
            None => want
                .calls
                .iter()
                .map(|c| RecordedCall {
                    max_edge: c.max_edge,
                    quality: c.quality,
                })
                .collect(),
        };
        assert_eq!(
            transcoder.calls(),
            want_calls,
            "[{}] the ladder v5 walked",
            c.label
        );
        // Whatever the rungs, the box is always v4's: the seam's `max_edge`.
        for call in transcoder.calls() {
            assert_eq!(call.max_edge, 1024, "[{}] max edge", c.label);
        }

        // --- the log line ---
        let (_, lines) = quilltap_core::test_support::captured_with(|| {
            let t2 = ScriptedTranscoder::new().with_script(original.clone(), c.script.to_script());
            shrink_image_for_llm_transport(
                &t2,
                &original,
                &c.mime_type,
                c.provider.as_deref(),
                c.filename.as_deref(),
            )
        });
        match &want.log {
            None => assert!(
                lines.is_empty(),
                "[{}] v4 logged nothing; v5 logged {lines:?}",
                c.label
            ),
            Some(log) => {
                assert_eq!(
                    lines.len(),
                    1,
                    "[{}] expected one line, got {lines:?}",
                    c.label
                );
                let line = &lines[0];
                let want_level = log.level.to_uppercase();
                assert!(
                    line.starts_with(&format!("{want_level} ")),
                    "[{}] level: want {want_level}, line {line}",
                    c.label
                );
                assert!(
                    line.contains(&log.message),
                    "[{}] message {:?} missing from {line}",
                    c.label,
                    log.message
                );
                for (k, v) in &log.bag {
                    let snake = SNAKE
                        .iter()
                        .find(|(camel, _)| camel == k)
                        .map(|(_, s)| *s)
                        .unwrap_or_else(|| {
                            panic!(
                                "[{}] v4's bag carries an unmapped key {k:?} — add it to SNAKE \
                                 and port the field",
                                c.label
                            )
                        });
                    assert!(
                        line.contains(&format!("{snake}={}", rendered(v))),
                        "[{}] bag {k}={} missing from {line}",
                        c.label,
                        rendered(v)
                    );
                }
                // Narrowing (2): a key v4 DROPPED (an `undefined` value) is
                // rendered empty by v5 rather than omitted. Asserted by name
                // where it happens, so it can never be a silent difference.
                for absent in ["provider", "filename"] {
                    if !log.bag.contains_key(absent) {
                        assert!(
                            line.contains(&format!("{absent}= ")),
                            "[{}] v4 dropped {absent} (undefined); v5 must render it EMPTY — \
                             the recorded narrowing. line: {line}",
                            c.label
                        );
                    }
                }
            }
        }
    }

    // The byte generators, against the same fixed probes the TS half asserts —
    // a drift in either silently voids every `bufferSha256` above.
    assert_eq!(original_bytes(5), vec![13u8, 20, 27, 34, 41]);
    assert_eq!(encoded_bytes(78, 4), vec![78u8, 79, 80, 81]);

    eprintln!(
        "OK: llm-image-budget differential matched the oracle across all {} cases.",
        spec.cases.len()
    );
}
