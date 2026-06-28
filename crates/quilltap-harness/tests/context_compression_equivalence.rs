//! Tier-1 differential test #6: context-compression sizing (pure subset).
//!
//! Covers shouldApplyCompression / shouldApplyBudgetCompression /
//! splitMessagesForCompression / buildCompressedHistoryBlock. The Rust side
//! reconstructs the same inputs from each row and compares: bools exact, the
//! split exact (message structure + order), the history block exact string
//! (incl. the empty/undefined → null branch).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/context-compression.ts \
//!     > /tmp/oracle-context-compression.ndjson
//! Run:
//!   QT_ORACLE_CONTEXT_COMPRESSION=/tmp/oracle-context-compression.ndjson cargo test -p quilltap-harness

use quilltap_core::context_compression::{
    build_compressed_history_block, should_apply_budget_compression, should_apply_compression,
    split_messages_for_compression, CompressibleMessage, CompressionSettings,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "shouldApply")]
    ShouldApply {
        id: String,
        #[serde(rename = "messageCount")]
        message_count: i64,
        enabled: bool,
        #[serde(rename = "windowSize")]
        window_size: i64,
        bypass: bool,
        out: bool,
    },
    #[serde(rename = "budget")]
    Budget {
        id: String,
        total: i64,
        #[serde(rename = "maxAvailable")]
        max_available: i64,
        enabled: bool,
        bypass: bool,
        out: bool,
    },
    #[serde(rename = "split")]
    Split {
        id: String,
        messages: Vec<CompressibleMessage>,
        #[serde(rename = "windowSize")]
        window_size: i64,
        out: SplitOut,
    },
    #[serde(rename = "block")]
    Block {
        id: String,
        input: Option<String>,
        out: Option<String>,
    },
}

#[derive(Deserialize)]
struct SplitOut {
    #[serde(rename = "messagesToCompress")]
    messages_to_compress: Vec<CompressibleMessage>,
    #[serde(rename = "windowMessages")]
    window_messages: Vec<CompressibleMessage>,
}

#[test]
fn context_compression_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CONTEXT_COMPRESSION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_CONTEXT_COMPRESSION to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 4];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::ShouldApply {
                id,
                message_count,
                enabled,
                window_size,
                bypass,
                out,
            } => {
                let settings = CompressionSettings {
                    enabled,
                    window_size,
                };
                assert_eq!(
                    should_apply_compression(message_count, &settings, bypass),
                    out,
                    "shouldApply '{id}'"
                );
                counts[0] += 1;
            }
            OracleRow::Budget {
                id,
                total,
                max_available,
                enabled,
                bypass,
                out,
            } => {
                // window_size is irrelevant to the budget predicate; use a placeholder.
                let settings = CompressionSettings {
                    enabled,
                    window_size: 5,
                };
                assert_eq!(
                    should_apply_budget_compression(total, max_available, &settings, bypass),
                    out,
                    "budget '{id}'"
                );
                counts[1] += 1;
            }
            OracleRow::Split {
                id,
                messages,
                window_size,
                out,
            } => {
                let got = split_messages_for_compression(&messages, window_size);
                assert_eq!(
                    got.messages_to_compress, out.messages_to_compress,
                    "split '{id}' compress"
                );
                assert_eq!(
                    got.window_messages, out.window_messages,
                    "split '{id}' window"
                );
                counts[2] += 1;
            }
            OracleRow::Block { id, input, out } => {
                let got = build_compressed_history_block(input.as_deref());
                assert_eq!(got, out, "block '{id}'");
                counts[3] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!(
        "OK: context-compression matched oracle ({} shouldApply, {} budget, {} split, {} block).",
        counts[0], counts[1], counts[2], counts[3]
    );
}
