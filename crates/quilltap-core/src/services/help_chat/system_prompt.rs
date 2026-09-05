//! v4 `lib/help-chat/system-prompt-builder.ts` — the system prompt for a
//! character in help-chat mode. Simpler than the Salon builder: character
//! identity, the help-assistant role, page context, tool guidance. Omits
//! roleplay templates, scene state, timestamps, the Concierge, project
//! context. Byte-exact; pinned by the tier-1 family
//! `help_system_prompt_equivalence`.
//!
//! The ten parts are joined with `\n\n` and JS-trimmed. Every fixed block is a
//! byte copy of v4's template literal; every conditional is v4's truthiness
//! test (`character.personality` → a non-empty string; `character.pronouns` →
//! an object; `toolInstructions` → a non-empty string; `userCharacter` → not
//! null; `otherCharacterNames && length > 0`).

use serde_json::Value;

use super::context_resolver::HelpPageContext;
use crate::jsstr::js_trim;
use crate::system_prompt::{
    build_identity_reinforcement, first_active_scenario_content, ScenarioEntry,
};
use crate::templates::{process_template, TemplateContext};

/// v4 `userCharacter?: { name: string; description: string } | null` — the
/// orchestrator builds `{ name: user.name || 'User', description: '' }` when the
/// user row exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpUserCharacter {
    pub name: String,
    pub description: String,
}

/// v4 `HelpSystemPromptOptions`. `character` is the OVERLAID character `Value`
/// (v4's `Character`); the builder reads `name`, `description`, `personality`,
/// `pronouns`, `scenarios` off it with v4's own truthiness.
pub struct HelpSystemPromptOptions<'a> {
    pub character: &'a Value,
    pub user_character: Option<&'a HelpUserCharacter>,
    pub page_context: Option<&'a HelpPageContext>,
    pub additional_page_contexts: &'a [HelpPageContext],
    pub other_character_names: Option<&'a [String]>,
    /// `toolInstructions?: string` — `None`/empty both read as v4's falsy.
    pub tool_instructions: Option<&'a str>,
}

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// A JS-truthy string read: absent / null / `""` → `None`.
fn truthy_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    s(v, key).filter(|x| !x.is_empty())
}

/// `firstActiveScenarioContent(character.scenarios)` over the character Value
/// (the `carina_query` shape).
fn scenarios_of(character: &Value) -> Vec<ScenarioEntry> {
    character
        .get("scenarios")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|sc| ScenarioEntry {
                    content: s(sc, "content").unwrap_or_default().to_string(),
                    archived: sc.get("archived") == Some(&Value::Bool(true)),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `${obj.key}` inside a template literal — a missing key renders `undefined`.
fn js_interp(v: &Value, key: &str) -> String {
    match v.get(key) {
        None | Some(Value::Null) => {
            if v.get(key).is_none() {
                "undefined".to_string()
            } else {
                "null".to_string()
            }
        }
        Some(Value::String(x)) => x.clone(),
        Some(other) => other.to_string(),
    }
}

/// v4 `buildHelpChatSystemPrompt(options)`.
pub fn build_help_chat_system_prompt(options: &HelpSystemPromptOptions<'_>) -> String {
    let character = options.character;
    let character_name = s(character, "name").unwrap_or_default().to_string();
    let mut parts: Vec<String> = Vec::new();
    // `userCharacter?.name || 'User'`.
    let user_name = options
        .user_character
        .map(|u| u.name.as_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("User")
        .to_string();

    // Template context for {{char}}/{{user}} replacement.
    let mut ctx = TemplateContext::default();
    ctx.set("char", character_name.clone());
    ctx.set("user", user_name);
    ctx.set(
        "description",
        s(character, "description").unwrap_or_default(),
    );
    ctx.set(
        "personality",
        s(character, "personality").unwrap_or_default(),
    );
    ctx.set(
        "scenario",
        first_active_scenario_content(&scenarios_of(character)),
    );
    ctx.set(
        "persona",
        options
            .user_character
            .map(|u| u.description.as_str())
            .unwrap_or(""),
    );

    // 1. Identity preamble (same anchor as Salon).
    parts.push(process_template(
        "## Character Identity\nYou are {{char}}. Everything that follows defines who you are and how you behave. Stay in character at all times.",
        &ctx,
    ));

    // 2. Help chat role definition.
    parts.push(format!(
        "## Help Assistant Role\nYou are assisting the user with Quilltap, a self-hosted AI workspace for writers, worldbuilders, and roleplayers. Your role is to answer questions about the application, help them navigate features, and troubleshoot issues — all while staying in character as {character_name}.\n\nWhen helping:\n- Use your tools (help_search, help_settings, help_navigate) to find accurate information\n- Be specific and actionable in your guidance\n- If you're not sure about something, search for it rather than guessing\n- **IMPORTANT:** Whenever you direct the user to a specific page, settings tab, or section, you MUST call the `help_navigate` tool with the appropriate URL. This gives the user a clickable button to go directly there. Do not just describe the navigation steps — always also call the tool so they can click through. The help documentation includes the correct URLs for each page.\n- Stay warm and helpful while remaining in character"
    ));

    // 3. Tool instructions.
    let tool_instructions = options.tool_instructions.filter(|t| !t.is_empty());
    if let Some(ti) = tool_instructions {
        parts.push(process_template(ti, &ctx));
    }

    // 4. Character personality (simplified — no scenario/dialogues for help).
    // Second-person wrapper mirrors the Salon builder — see the WHY note in
    // lib/chat/context/system-prompt-builder.ts.
    if let Some(personality) = truthy_str(character, "personality") {
        let processed = process_template(personality, &ctx);
        parts.push(format!(
            "## Character Personality\nThe following is what you know about yourself. Others do not see it unless you show them.\n{processed}"
        ));
    }

    // 5. Character pronouns — second person, mirroring the Salon builder.
    // `if (character.pronouns)` — an OBJECT is truthy (even `{}`); null/absent not.
    if let Some(pronouns) = character.get("pronouns").filter(|p| p.is_object()) {
        parts.push(format!(
            "## Character Pronouns\nYour pronouns are {}/{}/{}. Use them whenever you refer to yourself in narration, and expect others to use them for you.",
            js_interp(pronouns, "subject"),
            js_interp(pronouns, "object"),
            js_interp(pronouns, "possessive"),
        ));
    }

    // 6. Tool reinforcement. Second person, mirroring the Salon builder — see the
    // WHY note in lib/chat/context/system-prompt-builder.ts. The pronoun lookup it
    // replaces defaulted to 'they', which rendered "they CALLS them".
    if tool_instructions.is_some() {
        parts.push(process_template(
            "When you use workspace tools, you CALL them — you do not merely describe calling them. Every tool action produces a tool_use block, not prose.",
            &ctx,
        ));
    }

    // 7. Page context (the resolved help documentation).
    if let Some(pc) = options.page_context {
        parts.push(format!(
            "## Current Page Context\nThe user is currently viewing: **{}**\nURL: `{}`\n\n### Page Documentation\n{}",
            pc.title, pc.url, pc.content
        ));
    }

    // Additional page contexts (wildcard docs like sidebar, search).
    for ctx_doc in options.additional_page_contexts {
        parts.push(format!(
            "### Additional Context: {}\n{}",
            ctx_doc.title, ctx_doc.content
        ));
    }

    // 8. User character info.
    if let Some(u) = options.user_character {
        parts.push(format!(
            "## User Character\nYou are speaking with {}. {}",
            u.name, u.description
        ));
    }

    // 9. Multi-character note (if multiple help characters).
    if let Some(names) = options.other_character_names.filter(|n| !n.is_empty()) {
        parts.push(format!(
            "## Other Help Characters\nYou are one of several characters helping the user. The others are: {}. Each of you will respond to the user's questions. Try not to repeat what others have already said.",
            names.join(", ")
        ));
    }

    // 10. Identity reinforcement bookend.
    parts.push(build_identity_reinforcement(&character_name));

    js_trim(&parts.join("\n\n")).to_string()
}
