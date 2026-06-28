//! Port of the pure preview helper in v4's lib/chat-utils.ts.

use crate::jsstr::{js_trim, utf16_len, utf16_truncate};

/// Build a one-line preview from the last message's content: newlines collapsed
/// to spaces, trimmed, and truncated to 100 (UTF-16) characters with a trailing
/// `...`. Returns `None` when there is no last message. `messages` carries the
/// message contents in order. Mirrors v4's `getCharacterChatPreview`.
pub fn get_character_chat_preview(messages: &[String]) -> Option<String> {
    let last = messages.last()?;
    let content = js_trim(&last.replace('\n', " ")).to_string();
    if utf16_len(&content) > 100 {
        Some(format!("{}...", utf16_truncate(&content, 100)))
    } else {
        Some(content)
    }
}
