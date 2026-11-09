//! Native Tool Prompt Builder — port of v4 `lib/tools/native-tool-prompt.ts`
//! (whole at `624d0d25c`: `96156566b`'s preambles by kind, and 4.12.1's rule 6
//! from `6121f59b9` as `afec0af10`'s merge kept it).
//!
//! Builds system prompt instructions that guide LLMs on correct tool
//! invocation behavior when native function calling IS supported.
//!
//! Two preambles, chosen by the speaker's kind
//! ([`crate::characters::kind`], `KindPolicy::tool_rules`):
//!
//!   - `assistant` — "Tool Execution Rules (MANDATORY — overrides all other
//!     behavioral patterns)". For assistant characters and the assistant
//!     surfaces (Brahma Console, Help chat, one-shot loops).
//!   - `roleplay` — "Using tools". Tool calls are things the character does;
//!     the preamble no longer claims to override the character.
//!
//! Both carry the same numbered rules. The anti-narration rules exist for a
//! real failure (a model describing a tool call in prose instead of making it)
//! and must survive in every variant; a test holds that.

use crate::characters::kind::ToolRulesVariant;

/// The rules every variant carries, word for word (v4 `NATIVE_TOOL_RULES`).
pub const NATIVE_TOOL_RULES: &str = r#"1. **Never narrate tool use as a substitute for performing it.** Describing the action in prose — pulling up the file, executing the search, reaching for the vault — is NOT the same as calling a tool. If you find yourself writing prose that describes using a tool, STOP and actually call the tool instead.

2. **Do not announce tool calls before making them.** Do not write "Let me search for that now" or "I'll look that up" as a separate text block before the tool call. Just call the tool. If you want to add brief context, include the tool call in the SAME response — do not end your turn with a promise to act.

3. **After calling a tool, wait for the result before describing what you found.** Do not fabricate, assume, or narrate tool results. If a tool call is needed, generate the tool_use block, then respond to the actual result in your next turn.

4. **Chain tool calls when needed.** You can make multiple tool calls in a single response. If a task requires reading a file, then updating it, generate both tool calls. Do not describe the second step in prose — invoke it.

5. **Self-check: If your response contains zero tool_use blocks but describes performing tool actions, you have failed to follow these rules.** Regenerate with actual tool invocations.

6. **Recalling past events — never invent specifics.** When asked about a past event or a specific detail that is not in your current recall, use `search` (pass `since`/`until` when a time period is mentioned), then `read_conversation` on the source conversation from the results, and answer from what you find. If the search lists conversations from that period, read the likeliest one with `read_conversation` before answering. Only if it finds nothing and lists no conversation from that period, say you don't recall — never invent dates, places, or details."#;

/// v4 `PREAMBLES: Record<ToolRulesVariant, string>`. An exhaustive `match`: a
/// third variant is a compile error here, never v4's `undefined`.
fn preamble(variant: ToolRulesVariant) -> &'static str {
    match variant {
        ToolRulesVariant::Assistant => {
            "## Tool Execution Rules (MANDATORY — overrides all other behavioral patterns)

You have access to tools. When you decide to use a tool, you MUST actually invoke it by generating a tool_use content block. Follow these rules without exception:"
        }
        ToolRulesVariant::Roleplay => {
            "## Using tools

You have access to tools. A tool call is something you do; making one does not change who you are or how you speak. When you decide to use a tool, you MUST actually invoke it by generating a tool_use content block. Follow these rules without exception:"
        }
    }
}

/// Build the system prompt instructions for native tool usage (v4
/// `buildNativeToolInstructions(variant, hasTools = true)`).
///
/// These instructions ensure the LLM actually invokes tools via tool_use
/// content blocks rather than narrating tool actions in prose. Only returns
/// instructions when tools are available (`""` otherwise).
///
/// `variant` is which preamble the speaker hears (from its kind).
pub fn build_native_tool_instructions(variant: ToolRulesVariant, has_tools: bool) -> String {
    if !has_tools {
        return String::new();
    }
    format!("{}\n\n{}", preamble(variant), NATIVE_TOOL_RULES)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pre-kind (`236c04e35`) output, verbatim, with 4.12.1's rule-6 tail
    /// as the ONLY substitution — v4's "keeps the assistant text byte-identical
    /// to the pre-kind preamble". Built from the old literal so anything else
    /// that drifts fails here.
    const PRE_KIND_TEXT: &str = r#"## Tool Execution Rules (MANDATORY — overrides all other behavioral patterns)

You have access to tools. When you decide to use a tool, you MUST actually invoke it by generating a tool_use content block. Follow these rules without exception:

1. **Never narrate tool use as a substitute for performing it.** Describing the action in prose — pulling up the file, executing the search, reaching for the vault — is NOT the same as calling a tool. If you find yourself writing prose that describes using a tool, STOP and actually call the tool instead.

2. **Do not announce tool calls before making them.** Do not write "Let me search for that now" or "I'll look that up" as a separate text block before the tool call. Just call the tool. If you want to add brief context, include the tool call in the SAME response — do not end your turn with a promise to act.

3. **After calling a tool, wait for the result before describing what you found.** Do not fabricate, assume, or narrate tool results. If a tool call is needed, generate the tool_use block, then respond to the actual result in your next turn.

4. **Chain tool calls when needed.** You can make multiple tool calls in a single response. If a task requires reading a file, then updating it, generate both tool calls. Do not describe the second step in prose — invoke it.

5. **Self-check: If your response contains zero tool_use blocks but describes performing tool actions, you have failed to follow these rules.** Regenerate with actual tool invocations.

6. **Recalling past events — never invent specifics.** When asked about a past event or a specific detail that is not in your current recall, use `search` (pass `since`/`until` when a time period is mentioned), then `read_conversation` on the source conversation from the results, and answer from what you find. If you find nothing, say you don't recall — never invent dates, places, or details."#;

    const OLD_RULE6_TAIL: &str =
        "If you find nothing, say you don't recall — never invent dates, places, or details.";
    const NEW_RULE6_TAIL: &str = "If the search lists conversations from that period, read the likeliest one with `read_conversation` before answering. Only if it finds nothing and lists no conversation from that period, say you don't recall — never invent dates, places, or details.";

    #[test]
    fn keeps_the_assistant_text_byte_identical_to_the_pre_kind_preamble() {
        assert_eq!(PRE_KIND_TEXT.matches(OLD_RULE6_TAIL).count(), 1);
        let expected = PRE_KIND_TEXT.replace(OLD_RULE6_TAIL, NEW_RULE6_TAIL);
        assert_eq!(
            build_native_tool_instructions(ToolRulesVariant::Assistant, true),
            expected
        );
    }

    #[test]
    fn roleplay_preamble_drops_the_override_claim() {
        let text = build_native_tool_instructions(ToolRulesVariant::Roleplay, true);
        assert!(text.starts_with("## Using tools\n\n"));
        assert!(!text.contains("MANDATORY"));
        assert!(!text.contains("overrides all other behavioral patterns"));
        assert!(text.contains("A tool call is something you do"));
    }

    #[test]
    fn every_variant_carries_every_rule_verbatim() {
        let rules: Vec<&str> = NATIVE_TOOL_RULES.split("\n\n").collect();
        assert_eq!(rules.len(), 6);
        for (i, rule) in rules.iter().enumerate() {
            assert!(rule.starts_with(&format!("{}. **", i + 1)));
        }
        for variant in [ToolRulesVariant::Assistant, ToolRulesVariant::Roleplay] {
            let text = build_native_tool_instructions(variant, true);
            for rule in &rules {
                assert!(text.contains(rule), "{variant:?} lacks rule: {rule}");
            }
            assert!(text.ends_with(NATIVE_TOOL_RULES));
        }
    }

    #[test]
    fn empty_without_tools() {
        for variant in [ToolRulesVariant::Assistant, ToolRulesVariant::Roleplay] {
            assert_eq!(build_native_tool_instructions(variant, false), "");
        }
    }
}
