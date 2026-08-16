//! Turn Extras — the parts of an outgoing payload that are not context.
//!
//! Port of v4's `lib/services/chat-message/turn-extras.ts` (`f933ba9c`, bug 70).
//!
//! The context builder produces the messages; the orchestrator then adds things
//! the builder never sees: the tool/function schemas (which ride beside the
//! message array entirely) and one or two system messages spliced in downstream
//! (agent-mode instructions, the tool-change notice).
//!
//! All of it counts against the model's context window, and none of it used to
//! count against the budget — the builder packed history right up to the ceiling
//! and the orchestrator then piled these on top, which is how a turn could clear
//! the budget and still exceed the window.
//!
//! **This module is the one place those additions are built and measured.**
//! Building them here and handing the *same strings* back to the splice sites is
//! the point: a reservation computed from separately-constructed text is a
//! reservation that drifts the first time either copy is edited.
//!
//! Seam: v4 passes the `Provider` to the estimator, which resolves its
//! chars-per-token from the plugin registry. v5 injects the rate instead (the
//! established [`crate::token_estimation`] seam), so `chars_per_token` is a
//! parameter here.

use serde_json::Value;

use crate::token_estimation::{count_message_tokens, count_tool_schema_tokens};

/// Extract tool names from provider-shaped tool definitions — v4
/// `extractToolNames`.
///
/// Handles both the OpenAI-style `{type, function: {name}}` wrapper and the flat
/// `{name}` shape; anything unrecognisable is dropped rather than named (v4 maps
/// it to the literal `'unknown'` and then filters that out).
///
/// v4's `toolObj.function?.name || toolObj.name` is a truthiness chain, so an
/// EMPTY name falls through to the next candidate exactly as an absent one does.
/// A non-string name is out of the modeled domain: every producer of this slate
/// (v4's tool builders and v5's [`crate::tools`]) types the name as a string.
/// (Likewise a `null`/primitive ARRAY ENTRY: v4's `toolObj.function?.name`
/// optional-chains only past `.function`, so a null entry throws there where
/// this walk drops it — out of the modeled domain, noted not modeled.)
pub fn extract_tool_names(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .filter_map(|tool| {
            let truthy = |v: Option<&Value>| {
                v.and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            truthy(tool.get("function").and_then(|f| f.get("name")))
                .or_else(|| truthy(tool.get("name")))
        })
        .collect()
}

/// Build the notice telling the model its tool roster changed mid-chat — v4
/// `buildToolChangeNotice`.
///
/// Only fires when the user explicitly changed tool settings — not on the first
/// message and not on an interval.
pub fn build_tool_change_notice(tool_names: &[String]) -> String {
    if tool_names.is_empty() {
        return "[System Notice] Your available tools have been updated. All tools have been disabled for this chat. Do not attempt to use any tools.".to_string();
    }

    format!(
        "[System Notice] Your available tools have been updated. You now have access to the following {} tool(s): {}. Tools not listed are no longer available for this chat.",
        tool_names.len(),
        tool_names.join(", ")
    )
}

/// v4 `TurnExtrasOptions`.
pub struct TurnExtrasOptions<'a> {
    /// Tools as they will be sent (already narrowed to the native-calling set).
    pub tools: &'a [Value],
    /// Whether agent mode is on for this turn.
    pub agent_mode_enabled: bool,
    /// The agent-mode turn ceiling (only read when enabled).
    pub agent_mode_max_turns: i64,
    /// Whether the user changed tool settings since the last turn.
    pub tool_settings_changed: bool,
    /// The estimator's chars-per-token rate (v4 passes the provider instead).
    pub chars_per_token: f64,
}

/// v4 `TurnExtras`.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnExtras {
    /// Agent-mode instructions to splice in, or `None` when agent mode is off.
    pub agent_mode_instructions: Option<String>,
    /// Tool-change notice to splice in, or `None` when tools did not change.
    pub tool_change_notice: Option<String>,
    /// Tokens the tool schemas occupy beside the message array.
    pub tool_schema_tokens: i64,
    /// Total tokens these additions will contribute to the outgoing payload —
    /// pass as `reserved_outgoing_tokens` to the context builder so history is
    /// not packed into space they will take.
    pub reserved_tokens: i64,
}

/// Build and measure everything this turn will add after the context is built —
/// v4 `collectTurnExtras`.
///
/// Call before building context (all the inputs are known by then), pass
/// `reserved_tokens` into the builder, and splice the returned strings rather
/// than rebuilding them.
pub fn collect_turn_extras(options: TurnExtrasOptions<'_>) -> TurnExtras {
    let agent_mode_instructions = if options.agent_mode_enabled {
        Some(super::agent_mode::build_agent_mode_instructions(
            options.agent_mode_max_turns,
        ))
    } else {
        None
    };

    let tool_change_notice = if options.tool_settings_changed {
        Some(build_tool_change_notice(&extract_tool_names(options.tools)))
    } else {
        None
    };

    let tool_schema_tokens = count_tool_schema_tokens(options.tools, options.chars_per_token);

    // Both extras are spliced in as system messages, so they carry the same
    // per-message overhead the builder's own accounting applies.
    let agent_mode_tokens = agent_mode_instructions
        .as_deref()
        .map(|s| count_message_tokens("system", s, options.chars_per_token))
        .unwrap_or(0);
    let tool_change_tokens = tool_change_notice
        .as_deref()
        .map(|s| count_message_tokens("system", s, options.chars_per_token))
        .unwrap_or(0);

    TurnExtras {
        agent_mode_instructions,
        tool_change_notice,
        tool_schema_tokens,
        reserved_tokens: tool_schema_tokens + agent_mode_tokens + tool_change_tokens,
    }
}
