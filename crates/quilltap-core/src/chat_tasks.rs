//! Port of the pure cheap-LLM-task helpers in v4's
//! lib/memory/cheap-llm-tasks/chat-tasks.ts: stripping tool-call artifacts from
//! assistant text and extracting the visible user/assistant conversation.
//!
//! Regex-engine fidelity: the tool-marker patterns are literal/ASCII; the only
//! `\s` (in the JSON key-value line test and the leading-JSON quick-exit) uses
//! the JS whitespace set via [`crate::jsstr`]. Line/`\n` splitting and `.trim()`
//! follow JS semantics.

use crate::jsstr::{is_js_ws, js_trim, utf16_len, JS_WS_CLASS};
use regex::Regex;

/// Strip tool-call artifacts from assistant message content so downstream cheap
/// tasks (titles, summaries) see the conversation, not tool machinery. Returns
/// `None` when fewer than 20 (UTF-16) characters of conversational text remain.
/// Mirrors v4's `stripToolArtifacts`.
pub fn strip_tool_artifacts(content: &str) -> Option<String> {
    // Quick exit: no tool markers and not leading JSON → return unchanged. The
    // `^\s*[{[\],}]` test = first non-JS-whitespace char is a JSON structural one.
    let starts_json = content
        .trim_start_matches(is_js_ws)
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '{' | '[' | ']' | ',' | '}'));
    if !content.contains("[Tool") && !content.contains("\"toolName\"") && !starts_json {
        return Some(content.to_string());
    }

    // Remove [Tool call made] markers and [Tool Result: …] blocks.
    let cleaned = content.replace("[Tool call made]", "");
    let tool_result = Regex::new(r"\[Tool Result:[^\]]*\]").unwrap();
    let cleaned = tool_result.replace_all(&cleaned, "");

    // Drop lines that look like JSON rather than conversation. Kept lines retain
    // their original (untrimmed) form, exactly as v4 does.
    let json_start = Regex::new(r"^[{}\[\],:]").unwrap();
    let json_kv = Regex::new(&format!(r#"^"[^"]*"{JS_WS_CLASS}*:"#)).unwrap();
    let kept: Vec<&str> = cleaned
        .split('\n')
        .filter(|line| {
            let t = js_trim(line);
            !t.is_empty() && !json_start.is_match(t) && !json_kv.is_match(t)
        })
        .collect();

    let final_text = js_trim(&kept.join("\n")).to_string();
    if utf16_len(&final_text) < 20 {
        return None;
    }
    Some(final_text)
}

/// A visible conversational message: role is `"user"` or `"assistant"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// A message-like input row: any of `type` / `role` / `content` may be absent.
#[derive(Clone, Debug, Default)]
pub struct RawMessage {
    pub type_: Option<String>,
    pub role: Option<String>,
    pub content: Option<String>,
}

/// Extract only the visible user/assistant conversational messages from a
/// message-like array — the standard filter for cheap LLM tasks. Skips
/// non-`message` typed entries, empty content, and non-user/assistant roles;
/// runs [`strip_tool_artifacts`] on assistant text and drops it when nothing
/// survives. Mirrors v4's `extractVisibleConversation`.
pub fn extract_visible_conversation(messages: &[RawMessage]) -> Vec<ChatMessage> {
    let mut result = Vec::new();
    for m in messages {
        // Skip non-message entries (system / context-summary events).
        if let Some(t) = &m.type_ {
            if t != "message" {
                continue;
            }
        }
        // Skip entries without content (falsy: absent or empty).
        let content = match &m.content {
            Some(c) if !c.is_empty() => c,
            _ => continue,
        };
        // Only USER / ASSISTANT roles (case-insensitive).
        let role = m.role.as_deref().unwrap_or("").to_uppercase();
        if role != "USER" && role != "ASSISTANT" {
            continue;
        }
        let lower_role = role.to_lowercase();
        if lower_role == "assistant" {
            match strip_tool_artifacts(content) {
                Some(cleaned) => result.push(ChatMessage {
                    role: lower_role,
                    content: cleaned,
                }),
                None => continue,
            }
        } else {
            result.push(ChatMessage {
                role: lower_role,
                content: content.clone(),
            });
        }
    }
    result
}
