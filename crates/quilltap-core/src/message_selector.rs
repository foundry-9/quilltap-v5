//! Port of v4's lib/chat/context/message-selector.ts — `selectRecentMessages`,
//! the greedy token-budget tail fit used to select recent messages to fit
//! within a context budget.
//!
//! Supports both single-character and multi-character message formats (the
//! `name`/`participantId` fields carry the multi-character attribution).
//!
//! Token estimation is reused from [`crate::token_estimation::estimate_tokens`]
//! (never re-ported); v4's `estimateTokens(text, provider)` resolves a
//! provider to a chars-per-token rate via the plugin registry, so here the
//! caller passes the resolved `chars_per_token` directly (the oracle drives it
//! at v4's default of 3.5, `token_estimation::DEFAULT_CHARS_PER_TOKEN`).

/// Extended message type with optional participant info (v4's
/// `SelectableMessage`).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectableMessage {
    pub role: String,
    pub content: String,
    pub id: Option<String>,
    pub thought_signature: Option<String>,
    pub name: Option<String>,
    pub participant_id: Option<String>,
}

/// Result of selecting recent messages (v4's `MessageSelectionResult`).
#[derive(Debug, Clone, PartialEq)]
pub struct MessageSelectionResult {
    pub messages: Vec<SelectableMessage>,
    pub token_count: i64,
    pub truncated: bool,
}

/// v4 `selectableMessageTokens` (`message-selector.ts:78-86`, bug 263): a
/// history message's cost in the request — the shared per-message rule
/// (`countMessageTokens`, which the pre-send check also uses) plus the name
/// prefix a provider without a `name` field renders it under. The role tokens
/// are the ROW's role as stored (`USER` / `ASSISTANT`), exactly what v4's
/// `countMessageTokens(msg)` reads. An empty `name` is JS-falsy: no prefix.
pub fn selectable_message_tokens(msg: &SelectableMessage, chars_per_token: f64) -> i64 {
    let name_overhead = match msg.name.as_deref().filter(|n| !n.is_empty()) {
        Some(name) => {
            crate::token_estimation::estimate_tokens(&format!("[{name}] "), chars_per_token)
        }
        None => 0,
    };
    crate::token_estimation::count_message_tokens(&msg.role, &msg.content, chars_per_token)
        + name_overhead
}

/// v4 `keepFields` (`message-selector.ts:88-102`): every field the request
/// needs, `id` included — it is the source chat_messages row, which the
/// attachment anchor uses to tell the user's own turn from a staff whisper
/// wearing role=user (bug 95). Both arms keep it since `bdd09b75b` (the
/// force-include used to drop it).
fn keep_fields(msg: &SelectableMessage) -> SelectableMessage {
    msg.clone()
}

/// Select recent messages to fit within a token budget. Supports both
/// single-character and multi-character message formats.
///
/// Walks `messages` from the END (most recent) backwards, greedily including
/// each message while `totalTokens + msgTokens <= maxTokens` (v4's break
/// condition is `>`, so the boundary is inclusive — a message that exactly
/// fills the remaining budget IS included). The first message that would
/// overflow stops the scan (v4 does not skip past it to try older/smaller
/// messages — this is a *tail* fit, not a knapsack).
///
/// If nothing fit (the very last message alone already exceeds the budget),
/// v4 still force-includes that last message so the result is never empty
/// when the input isn't — `truncated` is set in that case too.
pub fn select_recent_messages(
    messages: &[SelectableMessage],
    max_tokens: i64,
    chars_per_token: f64,
) -> MessageSelectionResult {
    if messages.is_empty() {
        return MessageSelectionResult {
            messages: Vec::new(),
            token_count: 0,
            truncated: false,
        };
    }

    let mut selected_messages: Vec<SelectableMessage> = Vec::new();
    let mut total_tokens: i64 = 0;
    let mut truncated = false;

    // Work backwards from most recent.
    for msg in messages.iter().rev() {
        let msg_tokens = selectable_message_tokens(msg, chars_per_token);

        if total_tokens + msg_tokens > max_tokens {
            truncated = true;
            break;
        }

        selected_messages.insert(0, keep_fields(msg));
        total_tokens += msg_tokens;
    }

    // Ensure we have at least the last message if possible.
    if selected_messages.is_empty() && !messages.is_empty() {
        let last_msg = &messages[messages.len() - 1];
        selected_messages.push(keep_fields(last_msg));
        total_tokens = selectable_message_tokens(last_msg, chars_per_token);
        truncated = true;
    }

    MessageSelectionResult {
        messages: selected_messages,
        token_count: total_tokens,
        truncated,
    }
}
