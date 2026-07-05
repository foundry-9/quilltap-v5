//! Agent-mode helpers (full port of v4
//! `lib/services/chat-message/agent-mode-resolver.service.ts`).
//!
//! The **pure** helpers the native tool loop ([`super::native_tool_loop`])
//! consumes (ported W4.1e):
//!
//!   - [`ResolvedAgentMode`] — the resolved agent-mode state the loop reads
//!     (`enabled` / `max_turns`).
//!   - [`build_force_final_message`] — the "you hit max turns, submit now"
//!     nudge (v4 `buildForceFinalMessage`).
//!   - [`generate_iteration_summary`] — the collapsed per-iteration UI summary
//!     (v4 `generateIterationSummary`).
//!   - [`extract_submit_final_response_from_text`] — recover a `submit_final_response`
//!     payload a model emitted as raw JSON text (v4
//!     `extractSubmitFinalResponseFromText`).
//!
//! The **stateful resolver** (ported W4.4 — closes the orchestrator's agent-mode
//! seam so the spine computes the real cascade instead of taking an injected
//! `ResolvedAgentMode`):
//!
//!   - [`resolve_agent_mode_setting`] — the Global → Character → Project → Chat
//!     cascade (v4 `resolveAgentModeSetting`).
//!   - [`DEFAULT_AGENT_MODE_SETTINGS`] — the fallback when `chatSettings` (or its
//!     `agentModeSettings` block) is absent.
//!   - [`build_agent_mode_instructions`] — the agent-mode system-prompt block
//!     the orchestrator injects into `formattedMessages` when agent mode is on
//!     (v4 `buildAgentModeInstructions`).

/// The resolved agent-mode state the tool loop reads (v4 `ResolvedAgentMode`
/// subset — the two fields the loop consumes). `enabledSource` is host-side
/// diagnostics (v4 logs it only), so it is omitted at this layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedAgentMode {
    /// Whether agent mode is enabled (v4 `enabled`).
    pub enabled: bool,
    /// The maximum tool iterations before the force-final pass (v4 `maxTurns`).
    pub max_turns: i64,
}

/// v4 `AgentModeSettings` — the global agent-mode config
/// (`chatSettings.agentModeSettings`). The caller resolves it (the Zod-defaulted
/// block, or [`DEFAULT_AGENT_MODE_SETTINGS`] when the settings row is absent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentModeSettings {
    /// `maxTurns` (Zod `.min(1).max(25).default(10)`).
    pub max_turns: i64,
    /// `defaultEnabled` (Zod `.default(false)`).
    pub default_enabled: bool,
}

/// v4 `DEFAULT_AGENT_MODE_SETTINGS = { maxTurns: 10, defaultEnabled: false }` —
/// used when `globalSettings?.agentModeSettings` is null/undefined.
pub const DEFAULT_AGENT_MODE_SETTINGS: AgentModeSettings = AgentModeSettings {
    max_turns: 10,
    default_enabled: false,
};

/// v4 `resolveAgentModeSetting` — resolve the effective agent-mode setting through
/// the cascade **Global → Character → Project → Chat**. Each level overrides the
/// previous when it carries an EXPLICIT value (`Some`, i.e. v4's
/// `!== null && !== undefined`); a `None` (null/absent) leaves the running value
/// untouched. `max_turns` is taken from the global settings and is not overridden
/// by any level (v4 never mutates `maxTurns` past the initial read).
///
/// `global` is the caller's `globalSettings?.agentModeSettings ??
/// DEFAULT_AGENT_MODE_SETTINGS`. The three overrides are the nullable-boolean
/// fields `character.defaultAgentModeEnabled` / `project.defaultAgentModeEnabled`
/// / `chat.agentModeEnabled` (each `None` when JSON-null or absent).
pub fn resolve_agent_mode_setting(
    chat_agent_mode_enabled: Option<bool>,
    project_default: Option<bool>,
    character_default: Option<bool>,
    global: AgentModeSettings,
) -> ResolvedAgentMode {
    // Start with global settings.
    let mut enabled = global.default_enabled;
    let max_turns = global.max_turns;

    // Character level override (if explicitly set).
    if let Some(c) = character_default {
        enabled = c;
    }
    // Project level override (if explicitly set).
    if let Some(p) = project_default {
        enabled = p;
    }
    // Chat level override (if explicitly set).
    if let Some(ch) = chat_agent_mode_enabled {
        enabled = ch;
    }

    ResolvedAgentMode { enabled, max_turns }
}

/// v4 `buildAgentModeInstructions` — the agent-mode system-prompt block the
/// orchestrator injects into `formattedMessages` when agent mode is enabled. The
/// text is byte-verbatim from v4 (the leading `\n` + trailing `\n` are
/// `.trim()`ed away, matching v4's `.trim()`); `max_turns` interpolates into the
/// "up to **N tool iterations**" line. Byte-exactness matters — the injected
/// message becomes part of the provider request (the canned stream key).
pub fn build_agent_mode_instructions(max_turns: i64) -> String {
    format!(
        "## Agent Mode Instructions\n\
\n\
You are operating in **Agent Mode**. This means you should:\n\
\n\
1. **Use tools iteratively** to gather information, verify results, and refine your understanding before answering\n\
2. **Think step-by-step** and use multiple tool calls when needed to build a comprehensive answer\n\
3. **Verify your work** by checking results and correcting any errors you discover\n\
4. **Submit your final answer** using the `submit_final_response` tool **only** when you have completed multi-step agentic work this turn that warrants a structured summary\n\
\n\
### Important Guidelines:\n\
- You have up to **{max_turns} tool iterations** before you must submit your final response\n\
- Each time you use a tool counts as one iteration\n\
- When you reach the limit, you will be prompted to submit your final response immediately\n\
- Do NOT call other tools after calling `submit_final_response`\n\
- The `response` parameter should contain your complete, well-formatted answer\n\
\n\
### When to Submit — and When NOT To:\n\
- **Submit** when you have done real agentic work this turn (multi-step research, file edits, verification) and the user needs a structured summary of what you did.\n\
- **Do NOT submit** if the user's message is conversational, relational, or a simple follow-up — just respond in character with natural prose. You may still use tools like memory search to inform that reply; agent mode availability does not mean every turn must end in `submit_final_response`.\n\
- **Do NOT submit** just to re-summarize or wrap up work from a previous turn that already concluded. Each turn stands on its own.\n\
- If uncertain between submitting and replying conversationally, prefer the natural prose reply.\n\
\n\
### Response Format:\n\
When you do submit, your final response should be polished and ready for the user to read. Include all relevant information from your tool use, formatted clearly. Otherwise, reply as you normally would — in character, as prose."
    )
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
    fn resolve_cascade_precedence_and_maxturns() {
        let global = AgentModeSettings {
            max_turns: 15,
            default_enabled: false,
        };
        // Nothing explicit → global default (off); max_turns from global.
        let r = resolve_agent_mode_setting(None, None, None, global);
        assert!(!r.enabled);
        assert_eq!(r.max_turns, 15);
        // Character on, nothing higher → character wins.
        assert!(resolve_agent_mode_setting(None, None, Some(true), global).enabled);
        // Project overrides character.
        assert!(!resolve_agent_mode_setting(None, Some(false), Some(true), global).enabled);
        // Chat overrides project + character (chat is the last level).
        assert!(resolve_agent_mode_setting(Some(true), Some(false), Some(false), global).enabled);
        assert!(!resolve_agent_mode_setting(Some(false), Some(true), Some(true), global).enabled);
        // Global default ON with no explicit override stays on.
        let global_on = AgentModeSettings {
            max_turns: 3,
            default_enabled: true,
        };
        let r = resolve_agent_mode_setting(None, None, None, global_on);
        assert!(r.enabled);
        assert_eq!(r.max_turns, 3);
        // A None at a level does NOT clear a lower-precedence explicit value.
        assert!(resolve_agent_mode_setting(None, None, Some(true), global).enabled);
    }

    #[test]
    fn default_settings_match_v4() {
        // Bind to a runtime local so the assertion isn't const-evaluable
        // (clippy::assertions_on_constants).
        let d = DEFAULT_AGENT_MODE_SETTINGS;
        assert_eq!(d.max_turns, 10);
        assert!(!d.default_enabled);
    }

    #[test]
    fn agent_mode_instructions_shape() {
        let s = build_agent_mode_instructions(12);
        // Trimmed (no leading/trailing newline), header first, maxTurns interpolated.
        assert!(s.starts_with("## Agent Mode Instructions"));
        assert!(s.contains("up to **12 tool iterations**"));
        assert!(s.ends_with("reply as you normally would — in character, as prose."));
        assert!(!s.starts_with('\n') && !s.ends_with('\n'));
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
