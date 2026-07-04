//! Agent-mode helpers (partial port of v4
//! `lib/services/chat-message/agent-mode-resolver.service.ts`).
//!
//! Only the **pure** helpers the native tool loop
//! ([`super::native_tool_loop`]) consumes are ported here (W4.1e):
//!
//!   - [`ResolvedAgentMode`] — the resolved agent-mode state the loop reads
//!     (`enabled` / `max_turns`). A plain input struct at this layer.
//!   - [`build_force_final_message`] — the "you hit max turns, submit now"
//!     nudge (v4 `buildForceFinalMessage`).
//!   - [`generate_iteration_summary`] — the collapsed per-iteration UI summary
//!     (v4 `generateIterationSummary`).
//!   - [`extract_submit_final_response_from_text`] — recover a `submit_final_response`
//!     payload a model emitted as raw JSON text (v4
//!     `extractSubmitFinalResponseFromText`).
//!
//! **Tracked deferral (W4.4):** the stateful resolver `resolveAgentModeSetting`
//! (the Global → Character → Project → Chat cascade), the
//! `DEFAULT_AGENT_MODE_SETTINGS` table read, and `buildAgentModeInstructions`
//! (the system-prompt injection) belong to the agent-mode-resolution unit
//! (W4.4). Here `ResolvedAgentMode` is supplied by the caller.

/// The resolved agent-mode state the tool loop reads (v4 `ResolvedAgentMode`
/// subset — the two fields the loop consumes). `enabled_source` is host-side
/// diagnostics the loop never reads, so it is omitted at this layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedAgentMode {
    /// Whether agent mode is enabled (v4 `enabled`).
    pub enabled: bool,
    /// The maximum tool iterations before the force-final pass (v4 `maxTurns`).
    pub max_turns: i64,
}

/// v4 `buildForceFinalMessage` — the user-turn message injected when the loop
/// reaches `maxTurns` without a `submit_final_response`, prompting the model to
/// submit its best answer NOW. A fixed string (verbatim from v4).
pub fn build_force_final_message() -> String {
    "You have reached the maximum number of agent turns. Please call the \
     submit_final_response tool NOW with your best answer based on the \
     information you have gathered. Do not call any other tools - only \
     submit_final_response."
        .to_string()
}

/// v4 `generateIterationSummary` — a brief collapsed summary of one agent
/// iteration for the UI. `iteration_number` is 1-indexed; `tools_used` is the
/// tool names called this iteration.
pub fn generate_iteration_summary(
    iteration_number: i64,
    tools_used: &[String],
    _content_preview: Option<&str>,
) -> String {
    if tools_used.is_empty() {
        return format!("Turn {iteration_number}: Thinking...");
    }
    let tool_list = tools_used.join(", ");
    if tools_used.len() == 1 {
        return format!("Turn {iteration_number}: Used {tool_list}");
    }
    format!(
        "Turn {iteration_number}: Used {} tools ({tool_list})",
        tools_used.len()
    )
}

/// v4 `extractSubmitFinalResponseFromText` — recover the inner `response` string
/// when a model emitted a `submit_final_response` payload as raw JSON text
/// (`{"response":"…"}`) instead of a proper tool call. Returns the inner
/// `response` when the text is EXACTLY such a payload (JS: `trimmed.startsWith('{"response"')`
/// then `JSON.parse` with a non-empty string `response`), otherwise the original
/// text unchanged.
pub fn extract_submit_final_response_from_text(text: &str) -> String {
    let trimmed = crate::jsstr::js_trim(text);
    if !trimmed.starts_with("{\"response\"") {
        return text.to_string();
    }
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(resp) = parsed.get("response").and_then(|v| v.as_str()) {
            if !resp.is_empty() {
                return resp.to_string();
            }
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_final_message_is_verbatim() {
        assert!(build_force_final_message().starts_with("You have reached the maximum"));
        assert!(build_force_final_message().ends_with("only submit_final_response."));
    }

    #[test]
    fn iteration_summary_thinking_single_multi() {
        assert_eq!(
            generate_iteration_summary(1, &[], None),
            "Turn 1: Thinking..."
        );
        assert_eq!(
            generate_iteration_summary(2, &["search".into()], None),
            "Turn 2: Used search"
        );
        assert_eq!(
            generate_iteration_summary(3, &["search".into(), "read_conversation".into()], None),
            "Turn 3: Used 2 tools (search, read_conversation)"
        );
    }

    #[test]
    fn extract_submit_final_response_from_text_paths() {
        // Exact payload → inner response.
        assert_eq!(
            extract_submit_final_response_from_text(r#"{"response":"hello world"}"#),
            "hello world"
        );
        // Leading whitespace is trimmed before the startsWith check.
        assert_eq!(
            extract_submit_final_response_from_text("  {\"response\":\"ok\"}"),
            "ok"
        );
        // Empty response → original text (JS `.length > 0` guard).
        assert_eq!(
            extract_submit_final_response_from_text(r#"{"response":""}"#),
            r#"{"response":""}"#
        );
        // Not a response payload → unchanged.
        assert_eq!(
            extract_submit_final_response_from_text("just prose"),
            "just prose"
        );
        // Malformed JSON that still starts with `{"response"` → unchanged.
        assert_eq!(
            extract_submit_final_response_from_text(r#"{"response": broken"#),
            r#"{"response": broken"#
        );
    }
}
