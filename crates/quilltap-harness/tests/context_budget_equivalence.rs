//! Tier-1 differential test #10 (Wave 1 / B2): context-budget arithmetic.
//!
//! Covers shouldSummarizeConversation / calculateRecentMessageCount /
//! resolveMaxTokens / calculateMaxAvailable / getRecommendedContextAllocation /
//! getSafeInputLimit / hasExtendedContext. Integers exact; the un-floored
//! `recentMessages` fraction within 1e-12. The model-context-limit that v4
//! resolves internally is carried in each window-relative row and injected.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/context-budget.ts \
//!     > /tmp/oracle-context-budget.ndjson
//! Run:
//!   QT_ORACLE_CONTEXT_BUDGET=/tmp/oracle-context-budget.ndjson cargo test -p quilltap-harness

use quilltap_core::context_budget::{
    calculate_max_available, calculate_recent_message_count, get_recommended_context_allocation,
    get_safe_input_limit, has_extended_context, resolve_max_tokens, should_summarize_conversation,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct MaxAvailOut {
    #[serde(rename = "maxAvailable")]
    max_available: i64,
    #[serde(rename = "maxContext")]
    max_context: i64,
    #[serde(rename = "maxTokens")]
    max_tokens: i64,
}

#[derive(Deserialize)]
struct AllocOut {
    #[serde(rename = "totalLimit")]
    total_limit: i64,
    #[serde(rename = "systemPrompt")]
    system_prompt: i64,
    memories: i64,
    knowledge: i64,
    #[serde(rename = "conversationSummary")]
    conversation_summary: i64,
    #[serde(rename = "recentMessages")]
    recent_messages: f64,
    #[serde(rename = "responseReserve")]
    response_reserve: i64,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "summarize")]
    Summarize {
        id: String,
        #[serde(rename = "messageCount")]
        message_count: i64,
        #[serde(rename = "estimatedTokens")]
        estimated_tokens: i64,
        #[serde(rename = "contextLimit")]
        context_limit: i64,
        out: bool,
    },
    #[serde(rename = "recentCount")]
    RecentCount {
        id: String,
        #[serde(rename = "availableTokens")]
        available_tokens: i64,
        #[serde(rename = "averageMessageTokens")]
        average_message_tokens: i64,
        out: i64,
    },
    #[serde(rename = "resolveTokens")]
    ResolveTokens {
        id: String,
        #[serde(rename = "maxTokens")]
        max_tokens: Option<i64>,
        #[serde(rename = "modelClass")]
        model_class: Option<String>,
        out: i64,
    },
    #[serde(rename = "maxAvailable")]
    MaxAvailable {
        id: String,
        #[serde(rename = "modelContextLimit")]
        model_context_limit: i64,
        #[serde(rename = "maxContext")]
        max_context: Option<i64>,
        #[serde(rename = "maxTokens")]
        max_tokens: Option<i64>,
        #[serde(rename = "modelClass")]
        model_class: Option<String>,
        out: MaxAvailOut,
    },
    #[serde(rename = "allocation")]
    Allocation {
        id: String,
        #[serde(rename = "totalLimit")]
        total_limit: i64,
        out: AllocOut,
    },
    #[serde(rename = "safeInput")]
    SafeInput {
        id: String,
        #[serde(rename = "totalLimit")]
        total_limit: i64,
        #[serde(rename = "maxResponseTokens")]
        max_response_tokens: i64,
        out: i64,
    },
    #[serde(rename = "hasExtended")]
    HasExtended {
        id: String,
        #[serde(rename = "totalLimit")]
        total_limit: i64,
        out: bool,
    },
}

#[test]
fn context_budget_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CONTEXT_BUDGET") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CONTEXT_BUDGET to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts = [0usize; 7];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Summarize {
                id,
                message_count,
                estimated_tokens,
                context_limit,
                out,
            } => {
                assert_eq!(
                    should_summarize_conversation(message_count, estimated_tokens, context_limit),
                    out,
                    "summarize '{id}'"
                );
                counts[0] += 1;
            }
            OracleRow::RecentCount {
                id,
                available_tokens,
                average_message_tokens,
                out,
            } => {
                assert_eq!(
                    calculate_recent_message_count(available_tokens, average_message_tokens),
                    out,
                    "recentCount '{id}'"
                );
                counts[1] += 1;
            }
            OracleRow::ResolveTokens {
                id,
                max_tokens,
                model_class,
                out,
            } => {
                assert_eq!(
                    resolve_max_tokens(max_tokens, model_class.as_deref()),
                    out,
                    "resolveTokens '{id}'"
                );
                counts[2] += 1;
            }
            OracleRow::MaxAvailable {
                id,
                model_context_limit,
                max_context,
                max_tokens,
                model_class,
                out,
            } => {
                let got = calculate_max_available(
                    model_context_limit,
                    max_context,
                    max_tokens,
                    model_class.as_deref(),
                );
                assert_eq!(
                    got.max_available, out.max_available,
                    "maxAvailable '{id}' avail"
                );
                assert_eq!(got.max_context, out.max_context, "maxAvailable '{id}' ctx");
                assert_eq!(got.max_tokens, out.max_tokens, "maxAvailable '{id}' tokens");
                counts[3] += 1;
            }
            OracleRow::Allocation {
                id,
                total_limit,
                out,
            } => {
                let got = get_recommended_context_allocation(total_limit);
                assert_eq!(got.total_limit, out.total_limit, "allocation '{id}' total");
                assert_eq!(
                    got.system_prompt, out.system_prompt,
                    "allocation '{id}' system"
                );
                assert_eq!(got.memories, out.memories, "allocation '{id}' memories");
                assert_eq!(got.knowledge, out.knowledge, "allocation '{id}' knowledge");
                assert_eq!(
                    got.conversation_summary, out.conversation_summary,
                    "allocation '{id}' summary"
                );
                assert!(
                    (got.recent_messages - out.recent_messages).abs() < 1e-12,
                    "allocation '{id}' recent: rust={} oracle={}",
                    got.recent_messages,
                    out.recent_messages
                );
                assert_eq!(
                    got.response_reserve, out.response_reserve,
                    "allocation '{id}' reserve"
                );
                counts[4] += 1;
            }
            OracleRow::SafeInput {
                id,
                total_limit,
                max_response_tokens,
                out,
            } => {
                assert_eq!(
                    get_safe_input_limit(total_limit, max_response_tokens),
                    out,
                    "safeInput '{id}'"
                );
                counts[5] += 1;
            }
            OracleRow::HasExtended {
                id,
                total_limit,
                out,
            } => {
                assert_eq!(has_extended_context(total_limit), out, "hasExtended '{id}'");
                counts[6] += 1;
            }
        }
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: context-budget matched oracle (counts {counts:?}).");
}
