//! Port of the pure sizing subset of lib/chat/context/compression.ts — the
//! sliding-window context-compression triggers, message split, and the
//! compressed-history block builder.
//!
//! Only the side-effect-free functions are ported here: the async LLM
//! orchestrator (`applyContextCompression`) is a Phase-3 mocked-LLM target, not
//! tier-1 material. These four decide WHETHER and WHAT to compress; the actual
//! cheap-LLM summarization lives above this seam.

/// The compression settings fields these functions read. (The full v4
/// `ContextCompressionSettings` carries token targets too, but the sizing
/// predicates only consult `enabled` and `windowSize`.)
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

/// Determines if compression should be applied based on message count and
/// settings — the legacy count-driven trigger. Disabled or bypassed short-circuit
/// to false; otherwise compress only when there are MORE messages than the window
/// (messages 1..=windowSize get full context).
pub fn should_apply_compression(
    message_count: i64,
    settings: &CompressionSettings,
    bypass_compression: bool,
) -> bool {
    if !settings.enabled {
        return false;
    }
    if bypass_compression {
        return false;
    }
    message_count > settings.window_size
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
