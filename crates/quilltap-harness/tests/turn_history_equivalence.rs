//! Tier-1 differential (P4.D347, v4 `bdd09b75b`): the pure heart of bugs
//! 263 / 264 / 265, diffed against v4's REAL `token-counter.ts` /
//! `message-selector.ts` / `turn-history.ts` / `turn-compression.ts` at the pin
//! (`harness/oracle/cases/turn-history.ts`).
//!
//! - **Bug 263 — the budget's unit prices.** `count_message_tokens` (content +
//!   ROLE tokens + 4), `count_messages_tokens` (Σ + 3; 0 for `[]`),
//!   `selectable_message_tokens` (the row price + the `[name] ` prefix; an
//!   empty name is JS-falsy), `select_recent_messages` (the force-include keeps
//!   `id`), the `packTurnHistory` clamp + the "getting long" advice, and the
//!   leading system blocks' bytes and count.
//! - **Bug 264 — the cast window.** `from_window`, compared against v4's
//!   `turnHistoryRows` with a cast whose every seat has full history access and
//!   no whispers, so the rows' ids are exactly `fromWindow`'s (attribution is
//!   `build_context_tier3`'s subject). v4's WARN `[ContextManager] Could not
//!   place the compression window in the cast history; keeping it whole` is
//!   captured on both sides with its three keys — and its silence on every
//!   other arm.
//! - **Bug 265 — the verbatim window.** `verbatim_window_start` over the row
//!   shapes the visible-conversation rule drops (a `context-summary`, an
//!   `inform` record, a Host "spoken of" card, an empty row, a tool-only
//!   assistant turn) and keeps (a legacy left-behind notice), plus
//!   `effective_window_size` (a zero standard → 5).
//!
//! R-11: each row carries its chars-per-token rate. Measured at the pin
//! (`token-counter.ts` `CHARS_PER_TOKEN`): 3.5 for the default and every
//! provider but GOOGLE (3.8); none runs at 4.
//!
//! Generate (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/turn-history.ts > /tmp/oracle-turn-history.ndjson
//! Run:
//!   QT_ORACLE_TURN_HISTORY=/tmp/oracle-turn-history.ndjson \
//!     cargo test -p quilltap-harness --test turn_history_equivalence -- --nocapture

use std::collections::BTreeMap;

use serde_json::{json, Value};

use quilltap_core::chat_tasks::RawMessage;
use quilltap_core::context_budget::should_summarize_conversation;
use quilltap_core::context_compression::{effective_window_size, verbatim_window_start};
use quilltap_core::message_selector::{
    select_recent_messages, selectable_message_tokens, SelectableMessage,
};
use quilltap_core::services::build_context::{
    from_window, ExistingMessage, MessageWithParticipant,
};
use quilltap_core::system_prompt::build_identity_reinforcement;
use quilltap_core::token_estimation::{count_message_tokens, count_messages_tokens};

const WARN: &str =
    "[ContextManager] Could not place the compression window in the cast history; keeping it whole";

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|m| (s(m, "role").unwrap(), s(m, "content").unwrap()))
        .collect()
}

fn selectable(v: &Value) -> SelectableMessage {
    SelectableMessage {
        role: s(v, "role").unwrap(),
        content: s(v, "content").unwrap(),
        id: s(v, "id"),
        thought_signature: s(v, "thoughtSignature"),
        name: s(v, "name"),
        participant_id: s(v, "participantId"),
    }
}

/// v4 `keepFields` serialised by `JSON.stringify`: an `undefined` field is
/// omitted (the corpus carries no `null`s).
fn selectable_json(m: &SelectableMessage) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("role".into(), json!(m.role));
    o.insert("content".into(), json!(m.content));
    for (k, v) in [
        ("id", &m.id),
        ("thoughtSignature", &m.thought_signature),
        ("name", &m.name),
        ("participantId", &m.participant_id),
    ] {
        if let Some(v) = v {
            o.insert(k.into(), json!(v));
        }
    }
    Value::Object(o)
}

fn rows_of(v: &Value) -> Vec<SelectableMessage> {
    v.as_array().unwrap().iter().map(selectable).collect()
}

fn rust_out(kind: &str, input: &Value) -> Value {
    let cpt = input.get("cpt").and_then(Value::as_f64).unwrap_or(3.5);
    match kind {
        "count_messages" => json!(count_messages_tokens(&pairs(&input["messages"]), cpt)),
        "count_message" => json!(count_message_tokens(
            &s(input, "role").unwrap(),
            &s(input, "content").unwrap(),
            cpt
        )),
        "select_tokens" => json!(selectable_message_tokens(&selectable(&input["msg"]), cpt)),
        "select" => {
            let r = select_recent_messages(
                &rows_of(&input["messages"]),
                input["maxTokens"].as_i64().unwrap(),
                cpt,
            );
            json!({
                "messages": r.messages.iter().map(selectable_json).collect::<Vec<_>>(),
                "tokenCount": r.token_count,
                "truncated": r.truncated,
            })
        }
        "pack" => {
            // v4 `packTurnHistory`: `selectRecentMessages(rows, max(0, min(remaining,
            // recentMessagesBudget)))` + the advice — the same clamp `build_context`
            // applies inline.
            let rows = rows_of(&input["rows"]);
            let remaining = input["remainingTokens"].as_i64().unwrap();
            let budget = input["recentMessagesBudget"].as_i64().unwrap();
            let total_limit = input["totalLimit"].as_i64().unwrap();
            let r = select_recent_messages(&rows, remaining.min(budget).max(0), cpt);
            let mut warnings: Vec<String> = Vec::new();
            if r.truncated {
                let all: Vec<(String, String)> = rows
                    .iter()
                    .map(|m| (m.role.clone(), m.content.clone()))
                    .collect();
                if should_summarize_conversation(
                    rows.len() as i64,
                    count_messages_tokens(&all, cpt),
                    total_limit,
                ) {
                    warnings.push("Conversation is getting long. Consider generating a summary for better context management.".to_string());
                }
            }
            json!({
                "messages": r.messages.iter().map(selectable_json).collect::<Vec<_>>(),
                "tokens": r.token_count,
                "truncated": r.truncated,
                "warnings": warnings,
            })
        }
        "leading_blocks" => {
            let mut blocks = vec![
                ("system".to_string(), s(input, "systemPrompt").unwrap()),
                (
                    "system".to_string(),
                    build_identity_reinforcement(&s(input, "characterName").unwrap()),
                ),
            ];
            if let Some(b) = s(input, "compressedHistoryBlock").filter(|b| !b.is_empty()) {
                blocks.push(("system".to_string(), b));
            }
            json!({
                "blocks": blocks.iter().map(|(r, c)| json!({"role": r, "content": c})).collect::<Vec<_>>(),
                "tokens": count_messages_tokens(&blocks, cpt),
            })
        }
        "window_start" => {
            let rows: Vec<RawMessage> = input["messages"]
                .as_array()
                .unwrap()
                .iter()
                // P4.D372: the ONE raw-row read (R-L1).
                .map(RawMessage::from_event)
                .collect();
            json!(verbatim_window_start(
                &rows,
                input["windowCount"].as_u64().unwrap() as usize
            ))
        }
        "effective_window" => json!(effective_window_size(
            // v4 `standardWindowSize || DEFAULT` — an undefined standard is 0 here.
            input["standard"].as_i64().unwrap_or(0),
            input["visible"].as_i64().unwrap(),
            input["cached"].as_i64(),
        )),
        "from_window" => {
            let rows: Vec<MessageWithParticipant> = input["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| MessageWithParticipant {
                    id: s(m, "id"),
                    role: s(m, "role").unwrap(),
                    content: s(m, "content").unwrap(),
                    participant_id: s(m, "participantId"),
                    ..Default::default()
                })
                .collect();
            let existing: Vec<ExistingMessage> = input["existingMessages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| ExistingMessage {
                    role: s(m, "role").unwrap(),
                    content: s(m, "content").unwrap(),
                    id: s(m, "id"),
                    ..Default::default()
                })
                .collect();
            let window_start = input["windowStart"].as_u64().unwrap() as usize;
            let (got, lines) = quilltap_core::test_support::captured_with(|| {
                from_window(&rows, &existing, window_start)
            });
            // Rebuild v4's `{message, context}` from the captured line: the three
            // keys must be on it as `key=value`.
            let warns: Vec<Value> = lines
                .iter()
                .filter(|l| l.contains(WARN))
                .map(|l| {
                    assert!(l.starts_with("WARN"), "{l}");
                    let field = |k: &str| -> i64 {
                        let at = l
                            .find(&format!("{k}="))
                            .unwrap_or_else(|| panic!("{k} missing: {l}"));
                        l[at + k.len() + 1..]
                            .split(' ')
                            .next()
                            .unwrap()
                            .parse()
                            .unwrap()
                    };
                    json!({
                        "message": WARN,
                        "context": {
                            "windowStart": field("windowStart"),
                            "rows": field("rows"),
                            "existingMessages": field("existingMessages"),
                        },
                    })
                })
                .collect();
            json!({
                "ids": got.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
                "contents": got.iter().map(|m| m.content.clone()).collect::<Vec<_>>(),
                "warns": warns,
            })
        }
        other => panic!("unknown kind {other}"),
    }
}

#[test]
fn turn_history_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_TURN_HISTORY") else {
        eprintln!("SKIP: set QT_ORACLE_TURN_HISTORY to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        let got = rust_out(kind, &row["input"]);
        if got != row["out"] {
            failures.push(format!("{kind}/{id}:\n  v4 {}\n  v5 {}", row["out"], got));
        }
        *kinds.entry(kind.to_string()).or_insert(0) += 1;
    }
    assert!(
        failures.is_empty(),
        "{} rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // A regenerated oracle that lost an arm must not pass.
    for (kind, min) in [
        ("count_message", 12),
        ("count_messages", 10),
        ("select_tokens", 10),
        ("select", 14),
        ("pack", 6),
        ("leading_blocks", 2),
        ("window_start", 10),
        ("effective_window", 7),
        ("from_window", 8),
    ] {
        let n = kinds.get(kind).copied().unwrap_or(0);
        assert!(n >= min, "{kind}: {n} rows < {min}");
    }
    eprintln!("turn-history: {kinds:?} all equal");
}
