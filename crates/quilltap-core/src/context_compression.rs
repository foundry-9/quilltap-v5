//! Port of the pure sizing subset of lib/chat/context/compression.ts — the
//! budget-driven context-compression trigger, message split, the verbatim
//! window, and the compressed-history block builder.
//!
//! Only the side-effect-free functions are ported here: the async LLM
//! orchestrator (`applyContextCompression`) is a Phase-3 mocked-LLM target, not
//! tier-1 material. These four decide WHETHER and WHAT to compress; the actual
//! cheap-LLM summarization lives above this seam.

/// The compression settings fields these functions read. (The full v4
/// `ContextCompressionSettings` carries token targets too, but the sizing
/// predicate only consults `enabled`.) v4's count-driven
/// `shouldApplyCompression` is gone at `bdd09b75b`, and so is its port.
#[derive(Clone, Copy, Debug)]
pub struct CompressionSettings {
    pub enabled: bool,
    pub window_size: i64,
}

/// A message in the conversation, simplified to what the split needs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompressibleMessage {
    pub role: String,
    pub content: String,
}

/// Determines if budget-driven compression should be applied: when the total
/// estimated prompt tokens exceed the available budget. Disabled/bypass
/// short-circuit to false. (Only `enabled` is consulted from settings.)
pub fn should_apply_budget_compression(
    total_estimated_tokens: i64,
    max_available: i64,
    settings: &CompressionSettings,
    bypass_compression: bool,
) -> bool {
    if !settings.enabled {
        return false;
    }
    if bypass_compression {
        return false;
    }
    total_estimated_tokens > max_available
}

/// The result of a sliding-window split: the older messages to compress and the
/// recent ones to keep in full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitResult {
    pub messages_to_compress: Vec<CompressibleMessage>,
    pub window_messages: Vec<CompressibleMessage>,
}

/// Split messages into those to compress and those to keep in full (the window).
///
/// When there are no more messages than the window, nothing is compressed and
/// every message is kept. Otherwise the last `window_size` messages are kept in
/// full and everything before them is compressed. (Assumes `window_size >= 1`,
/// which the v4 schema guarantees — `windowSize` is clamped to [3, 10]; the early
/// return makes the slice well-defined for every reachable input.)
pub fn split_messages_for_compression(
    messages: &[CompressibleMessage],
    window_size: i64,
) -> SplitResult {
    if (messages.len() as i64) <= window_size {
        // Nothing to compress — keep them all in the window.
        return SplitResult {
            messages_to_compress: Vec::new(),
            window_messages: messages.to_vec(),
        };
    }
    // Here window_size < messages.len(), so the split point is in range.
    let split = messages.len() - window_size as usize;
    SplitResult {
        messages_to_compress: messages[..split].to_vec(),
        window_messages: messages[split..].to_vec(),
    }
}

/// v4 `effectiveWindowSize` (`turn-compression.ts:243-257`): how many visible
/// messages stay verbatim. A cache computed for fewer messages than the chat now
/// has summarises only up to its own window, so every message since must ride
/// verbatim too or it would be in neither. Counted in visible messages, the
/// domain the async pre-compression counts in. A zero `standard` falls back to
/// v4's `DEFAULT_WINDOW_SIZE` (5) — JS `||`.
///
/// v4 logs INFO `[ContextManager] Using dynamic window size for fallback cache`
/// on the widening arm; the line is unchanged across `bdd09b75b` and v5 never
/// carried it (recorded by P4.D347, not ported here).
pub fn effective_window_size(
    standard_window_size: i64,
    visible_count: i64,
    cached_message_count: Option<i64>,
) -> i64 {
    let standard = if standard_window_size == 0 {
        5
    } else {
        standard_window_size
    };
    match cached_message_count {
        Some(cached) if cached < visible_count => standard + (visible_count - cached),
        _ => standard,
    }
}

/// v4 `verbatimWindowStart(messages, windowCount)` (`turn-compression.ts:
/// 268-282`, bug 265): the index at which the last `window_count` *visible*
/// messages begin. Counted with the same rule
/// [`crate::chat_tasks::extract_visible_conversation`] keeps rows by
/// ([`crate::chat_tasks::to_visible_conversation_message`]), so the window holds
/// exactly the messages the summary left out — a row that rule drops (an
/// assistant turn that was only tool markup, an `inform` record, a Host
/// "spoken of" card, an empty row) is carried along when it falls inside the
/// window but never counted toward it. `messages.len()` when nothing counts.
pub fn verbatim_window_start(
    messages: &[crate::chat_tasks::RawMessage],
    window_count: usize,
) -> usize {
    let mut found = 0usize;
    let mut start = messages.len();
    let mut i = messages.len();
    while i > 0 && found < window_count {
        i -= 1;
        if crate::chat_tasks::to_visible_conversation_message(&messages[i]).is_some() {
            found += 1;
            start = i;
        }
    }
    start
}

/// Build the compressed-history system block, or `None` when there's nothing to
/// emit. Mirrors the TS `!compressedHistory` falsiness: BOTH `None` and an empty
/// string yield `None` (an empty summary is not a block).
///
/// Emitted as a *separate* system message (the caller's concern) so the persona
/// prompt's cacheable bytes stay stable across turns — concatenating this on
/// would rehash the system prefix every time the rolling summary refreshed.
pub fn build_compressed_history_block(compressed_history: Option<&str>) -> Option<String> {
    match compressed_history {
        Some(h) if !h.is_empty() => Some(format!(
            "## Conversation Context (Compressed Summary of Earlier Messages)\n\n\
             The following is a summary of the earlier conversation. Recent messages follow this summary.\n\n\
             {h}"
        )),
        _ => None,
    }
}
