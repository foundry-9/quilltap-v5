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
    // === P4.D205 (v4 `e7d77bb60`) ===
    /// v4 widened the input shape to read `systemKind` so the Host's `inform`
    /// record can be skipped. See [`extract_visible_conversation`].
    pub system_kind: Option<String>,
    // === end P4.D205 ===
    // === P4.D347 (v4 `d68c1af88` / `bdd09b75b`) ===
    /// `systemSender`, read by the off-scene rule in
    /// [`to_visible_conversation_message`]: a Host "spoken of" card is a record,
    /// never conversation (v4 `VisibleConversationCandidate.systemSender`).
    pub system_sender: Option<String>,
    // === end P4.D347 ===
    // === P4.D372 (v4 `db6190b44`) ===
    /// `isEstablishedFact` — `Some(b)` only for a JSON boolean (v4's strict
    /// `=== true` seat-line predicate). Read by the established-fact arm of
    /// [`to_visible_conversation_message`].
    pub is_established_fact: Option<bool>,
    // === end P4.D372 ===
}

// === P4.D372 ===
impl RawMessage {
    /// The ONE read of a raw `ChatEvent` row into this shape (R-L1): v4's
    /// cheap-LLM callers pass RAW rows (`getMessages`), so every field the
    /// visibility rule reads — `type`, `role`, `content`, `systemKind`,
    /// `systemSender`, `isEstablishedFact` — must survive to it. Each hand-kept
    /// per-site spill is how such a field went missing before (P4.D205,
    /// P4.D347); every `Value` construction site calls this instead.
    pub fn from_event(v: &serde_json::Value) -> Self {
        let s = |k: &str| {
            v.get(k)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };
        RawMessage {
            type_: s("type"),
            role: s("role"),
            content: s("content"),
            system_kind: s("systemKind"),
            system_sender: s("systemSender"),
            is_established_fact: v
                .get("isEstablishedFact")
                .and_then(serde_json::Value::as_bool),
        }
    }
}
// === end P4.D372 ===

/// Extract only the visible user/assistant conversational messages from a
/// message-like array — the standard filter for cheap LLM tasks: one
/// [`to_visible_conversation_message`] per row. Mirrors v4's
/// `extractVisibleConversation`.
pub fn extract_visible_conversation(messages: &[RawMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .filter_map(to_visible_conversation_message)
        .collect()
}

/// One row of [`extract_visible_conversation`]: the clean user/assistant line, or
/// `None` when the row is not part of the visible conversation (v4
/// `toVisibleConversationMessage`, `bdd09b75b`). The one rule for what counts —
/// anything that must agree with that function's output row for row (the
/// compression window, [`crate::context_compression::verbatim_window_start`])
/// asks this rather than re-deriving it.
///
/// Skips non-`message` typed entries, the record-only rows (the Host's `inform`
/// record, a Host "spoken of" card), empty content and non-user/assistant roles;
/// runs [`strip_tool_artifacts`] on assistant text and drops it when nothing
/// survives.
pub fn to_visible_conversation_message(m: &RawMessage) -> Option<ChatMessage> {
    // Skip non-message entries (system / context-summary events).
    if let Some(t) = &m.type_ {
        if t != "message" {
            return None;
        }
    }
    // === P4.D205 (v4 `e7d77bb60`, `chat-tasks.ts:254`) ===
    // Record-only: documents a delivery, is not part of the conversation.
    //
    // This is the back door, and it was open on both sides until v4 closed
    // it. The filter below selects by ROLE, and the Host's inform record
    // wears `ASSISTANT` — so without this guard the record would be folded
    // into a rolling summary by the async pre-compression and handed back to
    // the model as system block 3 on every turn thereafter. v5 reproduced
    // the hole exactly (`RawMessage` did not even carry the field);
    // measured red-first against v4's post-fix oracle.
    //
    // The guard is deliberately narrow: every OTHER Staff announcement stays,
    // because that is deliberate context.
    if m.system_kind.as_deref() == Some("inform") {
        return None;
    }
    // === end P4.D205 ===
    // Record-only too: the Host's "spoken of" card. Folded into a compression
    // summary it would come back into every prompt as system block 3 (v4
    // `d68c1af88`, `chat-tasks.ts:279`).
    if crate::services::off_scene_kinds::is_off_scene_mention_introduction(
        m.system_sender.as_deref(),
        m.system_kind.as_deref(),
        m.content.as_deref(),
    ) {
        return None;
    }
    // Skip entries without content (falsy: absent or empty).
    let content = match &m.content {
        Some(c) if !c.is_empty() => c,
        _ => return None,
    };
    // === P4.D372 (v4 `db6190b44`, `chat-tasks.ts:286-288`) ===
    // Established facts, framed in the plain voice — always, because this
    // output can return into an opaque character's prompt as system block 3.
    // A machine fact is an input to the scene, so it becomes a user line
    // (and is never run through `strip_tool_artifacts`).
    if let Some(framed) = crate::established_fact::framed_established_fact_body(
        &crate::established_fact::EstablishedFactFields {
            is_established_fact: m.is_established_fact,
            system_sender: m.system_sender.as_deref(),
            system_kind: m.system_kind.as_deref(),
        },
        Some(content),
        crate::established_fact::EstablishedFactVoice::Plain,
    ) {
        return Some(ChatMessage {
            role: "user".to_string(),
            content: framed,
        });
    }
    // === end P4.D372 ===
    // Only USER / ASSISTANT roles (case-insensitive).
    let role = m.role.as_deref().unwrap_or("").to_uppercase();
    if role == "USER" {
        return Some(ChatMessage {
            role: "user".to_string(),
            content: content.clone(),
        });
    }
    if role != "ASSISTANT" {
        return None;
    }
    strip_tool_artifacts(content).map(|cleaned| ChatMessage {
        role: "assistant".to_string(),
        content: cleaned,
    })
}
