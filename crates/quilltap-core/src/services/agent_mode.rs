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
//!
//! The cascade decides only whether Agent Mode is on. What it means — its
//! flavour — comes from the responding character's kind
//! ([`crate::characters::kind`]): a roleplay character acts in the scene
//! (`in-character`), an assistant researches and reports (`research`). The
//! Brahma Console, the Help chat and the one-shot loops are assistants by
//! construction and always pass `research` (v4 `96156566b`).

use serde_json::Value;

use crate::characters::kind::{character_kind_policy, AgentModeFlavor};

/// Which cascade level supplied `enabled` (v4 `ResolvedAgentMode.enabledSource`).
/// v4 only logs it from the orchestrator, but the `toggle-agent-mode` action
/// returns it to the client as `agentModeSource` — so it is a real output, not
/// diagnostics (P4.9E3A).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentModeSource {
    Global,
    Character,
    Project,
    Chat,
}

impl AgentModeSource {
    /// v4's wire spelling (`'global' | 'character' | 'project' | 'chat'`).
    pub fn as_str(self) -> &'static str {
        match self {
            AgentModeSource::Global => "global",
            AgentModeSource::Character => "character",
            AgentModeSource::Project => "project",
            AgentModeSource::Chat => "chat",
        }
    }
}

/// The resolved agent-mode state (v4 `ResolvedAgentMode`). The tool loop reads
/// `enabled` / `max_turns`; `enabled_source` is what `toggle-agent-mode` echoes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedAgentMode {
    /// Whether agent mode is enabled (v4 `enabled`).
    pub enabled: bool,
    /// The maximum tool iterations before the force-final pass (v4 `maxTurns`).
    pub max_turns: i64,
    /// Which cascade level supplied `enabled` (v4 `enabledSource`).
    pub enabled_source: AgentModeSource,
    /// What Agent Mode means for the responding character (from its kind) —
    /// v4 `flavor`.
    pub flavor: AgentModeFlavor,
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
/// v4 `globalSettings?.agentModeSettings ?? DEFAULT_AGENT_MODE_SETTINGS`. The
/// stored block is Zod-defaulted by v4's read, so a present block always carries
/// both keys; a missing key falls back per-field to the same defaults.
pub fn global_agent_mode_settings(settings: Option<&Value>) -> AgentModeSettings {
    let block = settings.and_then(|s| s.get("agentModeSettings"));
    match block {
        Some(b) if !b.is_null() => AgentModeSettings {
            max_turns: b
                .get("maxTurns")
                .and_then(Value::as_i64)
                .unwrap_or(DEFAULT_AGENT_MODE_SETTINGS.max_turns),
            default_enabled: b
                .get("defaultEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(DEFAULT_AGENT_MODE_SETTINGS.default_enabled),
        },
        _ => DEFAULT_AGENT_MODE_SETTINGS,
    }
}

/// `character` is the RESPONDING character's row (v4's `@param character`):
/// its `defaultAgentModeEnabled` is the character tier (a boolean, else unset —
/// the reads every v5 caller made before P4.D370) and its kind sets the
/// flavour (`None` → `in-character`, v4's `characterKindPolicy(null)`).
/// `chat_id` is v4's `chat?.id` — every v5 caller holds the chat, so the
/// key-omitted arm is reachable only from the tier-1 driver.
///
/// Logs v4's `[AgentMode] Resolved agent mode` INFO from this ONE home, so it
/// fires from the Salon turn, the chat GET and the agent-mode toggle, as v4's
/// does.
pub fn resolve_agent_mode_setting(
    chat_id: Option<&str>,
    chat_agent_mode_enabled: Option<bool>,
    project_default: Option<bool>,
    character: Option<&Value>,
    global: AgentModeSettings,
) -> ResolvedAgentMode {
    // Start with global settings.
    let mut enabled = global.default_enabled;
    let mut enabled_source = AgentModeSource::Global;
    let max_turns = global.max_turns;

    // Character level override (if explicitly set).
    if let Some(c) = character
        .and_then(|c| c.get("defaultAgentModeEnabled"))
        .and_then(Value::as_bool)
    {
        enabled = c;
        enabled_source = AgentModeSource::Character;
    }
    // Project level override (if explicitly set).
    if let Some(p) = project_default {
        enabled = p;
        enabled_source = AgentModeSource::Project;
    }
    // Chat level override (if explicitly set).
    if let Some(ch) = chat_agent_mode_enabled {
        enabled = ch;
        enabled_source = AgentModeSource::Chat;
    }

    let result = ResolvedAgentMode {
        enabled,
        max_turns,
        enabled_source,
        flavor: character_kind_policy(character).agent_flavor,
    };

    // v4 `logger.info('[AgentMode] Resolved agent mode', { chatId: chat?.id,
    // ...result })` — `chatId` OMITTED when the chat is null (v4's undefined).
    match chat_id {
        Some(chat_id) => tracing::info!(
            target: "quilltap::agent_mode",
            chatId = %chat_id,
            enabled = result.enabled,
            maxTurns = result.max_turns,
            enabledSource = result.enabled_source.as_str(),
            flavor = result.flavor.as_str(),
            "[AgentMode] Resolved agent mode"
        ),
        None => tracing::info!(
            target: "quilltap::agent_mode",
            enabled = result.enabled,
            maxTurns = result.max_turns,
            enabledSource = result.enabled_source.as_str(),
            flavor = result.flavor.as_str(),
            "[AgentMode] Resolved agent mode"
        ),
    }

    result
}

/// v4 `buildAgentModeInstructions` — the agent-mode system-prompt block the
/// orchestrator injects into `formattedMessages` when agent mode is enabled. The
/// text is byte-verbatim from v4 (the leading `\n` + trailing `\n` are
/// `.trim()`ed away, matching v4's `.trim()`); `max_turns` interpolates into the
/// "up to **N tool iterations**" line. Byte-exactness matters — the injected
/// message becomes part of the provider request (the canned stream key).
///
/// `research` guides the assistant's loop, including when and how to use the
/// `submit_final_response` tool. `in-character` tells a roleplay character that
/// its tools are things it does in the scene, and that the turn is still
/// written in character, as prose; that flavour is never offered
/// `submit_final_response`.
pub fn build_agent_mode_instructions(max_turns: i64, flavor: AgentModeFlavor) -> String {
    if flavor == AgentModeFlavor::InCharacter {
        return format!(
            "## Acting in the scene\n\
\n\
You can use tools this turn, and you may use several in a row when the moment calls for it. Your tools are things you do in the story: looking something up, checking a record, moving an object, sending word to someone. Use them when you would, for your own reasons, and not because they are there.\n\
\n\
When you have done what you meant to do, write your turn as you always do: in character, in prose. Do not summarize what you did, report results as a list, or address anyone outside the story.\n\
\n\
You have up to **{max_turns} tool steps** this turn."
        );
    }
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
/// reaches `maxTurns`. `research` prompts the model to call
/// `submit_final_response` with its best answer NOW; `in-character` has no
/// such tool: it tells the character to stop using tools and write the turn.
/// Fixed strings (verbatim from v4).
pub fn build_force_final_message(flavor: AgentModeFlavor) -> String {
    if flavor == AgentModeFlavor::InCharacter {
        return "You have used every tool step you have this turn. Do not call any \
                more tools. Write your turn now, in character and in prose, from \
                what you have found and done."
            .to_string();
    }
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
        let research = build_force_final_message(AgentModeFlavor::Research);
        assert!(research.starts_with("You have reached the maximum"));
        assert!(research.ends_with("only submit_final_response."));
    }

    // v4 `agent-mode-flavor.test.ts` (P4.D370).
    #[test]
    fn force_final_in_character_asks_for_prose_and_names_no_tool() {
        let text = build_force_final_message(AgentModeFlavor::InCharacter);
        assert_eq!(
            text,
            "You have used every tool step you have this turn. Do not call any more tools. \
             Write your turn now, in character and in prose, from what you have found and done."
        );
        assert!(text.contains("Do not call any more tools"));
        assert!(text.contains("in character and in prose"));
        assert!(!text.contains("submit_final_response"));
        assert!(
            build_force_final_message(AgentModeFlavor::Research).contains("submit_final_response")
        );
    }

    #[test]
    fn instructions_in_character_are_scene_actions_in_prose_without_submit() {
        let text = build_agent_mode_instructions(6, AgentModeFlavor::InCharacter);
        assert!(text.starts_with("## Acting in the scene"));
        assert!(text.contains("things you do in the story"));
        assert!(text.contains("in character, in prose"));
        assert!(text.contains("**6 tool steps**"));
        assert!(!text.contains("submit_final_response"));
        assert!(!text.contains("Think step-by-step"));
        assert!(!text.contains("structured summary"));
        assert!(text.ends_with("You have up to **6 tool steps** this turn."));
        assert!(!text.starts_with('\n') && !text.ends_with('\n'));
        let research = build_agent_mode_instructions(6, AgentModeFlavor::Research);
        assert!(research.starts_with("## Agent Mode Instructions"));
        assert!(research.contains("submit_final_response"));
        assert!(research.contains("**6 tool iterations**"));
    }

    #[test]
    fn flavour_follows_the_responding_characters_kind() {
        use serde_json::json;
        let settings = AgentModeSettings {
            max_turns: 7,
            default_enabled: true,
        };
        let r = resolve_agent_mode_setting(
            None,
            None,
            None,
            Some(&json!({"kind": "roleplay"})),
            settings,
        );
        assert_eq!(
            r,
            ResolvedAgentMode {
                enabled: true,
                max_turns: 7,
                enabled_source: AgentModeSource::Global,
                flavor: AgentModeFlavor::InCharacter,
            }
        );
        // A character from a pre-kind vault counts as roleplay; so does none.
        assert_eq!(
            resolve_agent_mode_setting(None, None, None, Some(&json!({})), settings).flavor,
            AgentModeFlavor::InCharacter
        );
        assert_eq!(
            resolve_agent_mode_setting(None, None, None, None, settings).flavor,
            AgentModeFlavor::InCharacter
        );
        assert_eq!(
            resolve_agent_mode_setting(
                None,
                None,
                None,
                Some(&json!({"kind": "assistant"})),
                settings
            )
            .flavor,
            AgentModeFlavor::Research
        );
        // The cascade still decides only whether it is on.
        let r = resolve_agent_mode_setting(
            Some("c"),
            Some(false),
            None,
            Some(&json!({"kind": "assistant", "defaultAgentModeEnabled": true})),
            settings,
        );
        assert!(!r.enabled);
        assert_eq!(r.enabled_source, AgentModeSource::Chat);
        assert_eq!(r.flavor, AgentModeFlavor::Research);
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
        let on = serde_json::json!({"defaultAgentModeEnabled": true});
        let off = serde_json::json!({"defaultAgentModeEnabled": false});
        let r = resolve_agent_mode_setting(None, None, None, None, global);
        assert!(!r.enabled);
        assert_eq!(r.max_turns, 15);
        // Character on, nothing higher → character wins.
        assert!(resolve_agent_mode_setting(None, None, None, Some(&on), global).enabled);
        // Project overrides character.
        assert!(!resolve_agent_mode_setting(None, None, Some(false), Some(&on), global).enabled);
        // Chat overrides project + character (chat is the last level).
        assert!(
            resolve_agent_mode_setting(None, Some(true), Some(false), Some(&off), global).enabled
        );
        assert!(
            !resolve_agent_mode_setting(None, Some(false), Some(true), Some(&on), global).enabled
        );
        // Global default ON with no explicit override stays on.
        let global_on = AgentModeSettings {
            max_turns: 3,
            default_enabled: true,
        };
        let r = resolve_agent_mode_setting(None, None, None, None, global_on);
        assert!(r.enabled);
        assert_eq!(r.max_turns, 3);
        // A None at a level does NOT clear a lower-precedence explicit value.
        assert!(resolve_agent_mode_setting(None, None, None, Some(&on), global).enabled);
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
        let s = build_agent_mode_instructions(12, AgentModeFlavor::Research);
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
