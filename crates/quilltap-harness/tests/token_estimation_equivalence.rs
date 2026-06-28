//! Tier-1 differential test #11 (Wave 1 / B3): character-based token estimation.
//!
//! Covers estimateTokens / countMessageTokens / countMessagesTokens /
//! truncateToTokenLimit / getContextUsagePercent / getContextWarningLevel, all
//! on the default 3.5 chars-per-token path. Counts and strings exact.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/token-estimation.ts \
//!     > /tmp/oracle-token-estimation.ndjson
//! Run:
//!   QT_ORACLE_TOKEN_ESTIMATION=/tmp/oracle-token-estimation.ndjson cargo test -p quilltap-harness

use quilltap_core::token_estimation::{
    count_message_tokens, count_messages_tokens, estimate_tokens, get_context_usage_percent,
    get_context_warning_level, truncate_to_token_limit, DEFAULT_CHARS_PER_TOKEN,
};
use serde::Deserialize;

const CPT: f64 = DEFAULT_CHARS_PER_TOKEN;

#[derive(Deserialize)]
struct WireMsg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "estimate")]
    Estimate { id: String, text: String, out: i64 },
    #[serde(rename = "message")]
    Message {
        id: String,
        role: String,
        content: String,
        out: i64,
    },
    #[serde(rename = "conversation")]
    Conversation {
        id: String,
        messages: Vec<WireMsg>,
        out: i64,
    },
    #[serde(rename = "truncate")]
    Truncate {
        id: String,
        text: String,
        #[serde(rename = "maxTokens")]
        max_tokens: i64,
        suffix: String,
        out: String,
    },
    #[serde(rename = "usage")]
    Usage {
        id: String,
        #[serde(rename = "usedTokens")]
        used_tokens: i64,
        #[serde(rename = "contextLimit")]
        context_limit: i64,
        out: i64,
    },
    #[serde(rename = "warning")]
    Warning {
        id: String,
        #[serde(rename = "usedTokens")]
        used_tokens: i64,
        #[serde(rename = "contextLimit")]
        context_limit: i64,
        out: String,
    },
}

#[test]
fn token_estimation_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_TOKEN_ESTIMATION") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TOKEN_ESTIMATION to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 6];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Estimate { id, text, out } => {
                assert_eq!(estimate_tokens(&text, CPT), out, "estimate '{id}'");
                counts[0] += 1;
            }
            OracleRow::Message {
                id,
                role,
                content,
                out,
            } => {
                assert_eq!(
                    count_message_tokens(&role, &content, CPT),
                    out,
                    "message '{id}'"
                );
                counts[1] += 1;
            }
            OracleRow::Conversation { id, messages, out } => {
                let msgs: Vec<(String, String)> =
                    messages.into_iter().map(|m| (m.role, m.content)).collect();
                assert_eq!(
                    count_messages_tokens(&msgs, CPT),
                    out,
                    "conversation '{id}'"
                );
                counts[2] += 1;
            }
            OracleRow::Truncate {
                id,
                text,
                max_tokens,
                suffix,
                out,
            } => {
                assert_eq!(
                    truncate_to_token_limit(&text, max_tokens, CPT, &suffix),
                    out,
                    "truncate '{id}'"
                );
                counts[3] += 1;
            }
            OracleRow::Usage {
                id,
                used_tokens,
                context_limit,
                out,
            } => {
                assert_eq!(
                    get_context_usage_percent(used_tokens, context_limit),
                    out,
                    "usage '{id}'"
                );
                counts[4] += 1;
            }
            OracleRow::Warning {
                id,
                used_tokens,
                context_limit,
                out,
            } => {
                assert_eq!(
                    get_context_warning_level(used_tokens, context_limit),
                    out,
                    "warning '{id}'"
                );
                counts[5] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: token-estimation matched oracle (counts {counts:?}).");
}
