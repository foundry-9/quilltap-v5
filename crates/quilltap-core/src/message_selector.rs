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
        // Account for potential name prefix in token count.
        let name_overhead = match &msg.name {
            Some(name) => {
                crate::token_estimation::estimate_tokens(&format!("[{name}] "), chars_per_token)
            }
            None => 0,
        };
        let msg_tokens = crate::token_estimation::estimate_tokens(&msg.content, chars_per_token)
            + name_overhead
            + 4; // +4 for message overhead

        if total_tokens + msg_tokens > max_tokens {
            truncated = true;
            break;
        }

        // Preserve all fields including name and participantId (id is
        // dropped, mirroring v4's object literal which omits it too).
        selected_messages.insert(
            0,
            SelectableMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                id: None,
                thought_signature: msg.thought_signature.clone(),
                name: msg.name.clone(),
                participant_id: msg.participant_id.clone(),
            },
        );
        total_tokens += msg_tokens;
    }

    // Ensure we have at least the last message if possible.
    if selected_messages.is_empty() && !messages.is_empty() {
        let last_msg = &messages[messages.len() - 1];
        selected_messages.push(SelectableMessage {
            role: last_msg.role.clone(),
            content: last_msg.content.clone(),
            id: None,
            thought_signature: last_msg.thought_signature.clone(),
            name: last_msg.name.clone(),
            participant_id: last_msg.participant_id.clone(),
        });
        let name_overhead = match &last_msg.name {
            Some(name) => {
                crate::token_estimation::estimate_tokens(&format!("[{name}] "), chars_per_token)
            }
            None => 0,
        };
        total_tokens = crate::token_estimation::estimate_tokens(&last_msg.content, chars_per_token)
            + name_overhead
            + 4;
        truncated = true;
    }

    MessageSelectionResult {
        messages: selected_messages,
        token_count: total_tokens,
        truncated,
    }
}
