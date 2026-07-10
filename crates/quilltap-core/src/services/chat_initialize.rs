//! Chat-context assembly for new-chat creation — v4's `lib/chat/initialize.ts`.
//!
//! [`build_chat_context`] resolves the `{systemPrompt, firstMessage, character,
//! userCharacter}` bundle the create handler seeds a fresh chat with: the
//! vault-overlaid responding character (+ an optional user-controlled
//! "playing-as" character, or the character's `defaultPartnerId`), the
//! system-prompt selection (`selectedSystemPromptId` → default → first →
//! nothing), the scenario override, and the template pass.
//!
//! The system-prompt built here is v4 `initialize.ts`'s OWN `buildSystemPrompt`
//! — a legacy single-shot prompt written into the chat's head SYSTEM message —
//! **distinct** from the per-turn [`crate::system_prompt::build_system_prompt`]
//! (which consumes the compiled identity stack). Both compose the ported
//! template processor ([`crate::templates`]); this one adds the flat
//! "You are roleplaying as …" / "Character Description:" / "Scenario:" layout.
//!
//! Pure read composition over `characters_read` + the template processor; no
//! clock/id minting on this path.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::{characters_read, DbError};
use crate::jsstr::js_trim;
use crate::system_prompt::Pronouns;
use crate::templates::{
    process_character_templates, process_template, TemplateCharacter, TemplateContext,
    TemplateUserCharacter,
};

/// The user-controlled "playing-as" character (v4 `UserCharacter`), the subset
/// `buildSystemPrompt` + the Host user-character announcement read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserCharacter {
    pub id: String,
    pub name: String,
    /// `Some` only when non-empty (v4 `aliases?.length ? aliases : undefined`).
    pub aliases: Option<Vec<String>>,
    pub pronouns: Option<Pronouns>,
    pub description: Option<String>,
    pub manifesto: Option<String>,
    pub personality: Option<String>,
}

/// v4 `ChatContext` — the seed bundle. `character` is the overlaid character row
/// (the spine reads `.id` / `.name` and passes it into the seed writers).
#[derive(Clone, Debug)]
pub struct ChatContext {
    pub system_prompt: String,
    pub first_message: String,
    pub character: Value,
    pub user_character: Option<UserCharacter>,
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// v4 `getDefaultSystemPrompt`: `defaultSystemPromptId` → `isDefault` → first →
/// `''`.
fn get_default_system_prompt(character: &Value) -> String {
    let prompts = character.get("systemPrompts").and_then(Value::as_array);
    let Some(prompts) = prompts else {
        return String::new();
    };
    if prompts.is_empty() {
        return String::new();
    }
    if let Some(default_id) = character
        .get("defaultSystemPromptId")
        .and_then(Value::as_str)
    {
        if let Some(by_id) = prompts
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(default_id))
        {
            return by_id
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
        }
    }
    // isDefault flag, then first prompt, then ''.
    let by_default = prompts
        .iter()
        .find(|p| p.get("isDefault").and_then(Value::as_bool).unwrap_or(false));
    let content = by_default
        .and_then(|p| p.get("content").and_then(Value::as_str))
        .filter(|c| !c.is_empty())
        .or_else(|| {
            prompts
                .first()
                .and_then(|p| p.get("content").and_then(Value::as_str))
        })
        .unwrap_or_default();
    content.to_string()
}

/// v4 `getSelectedOrDefaultSystemPrompt`: a matching `selectedSystemPromptId`'s
/// content, else the default (a missing selection falls through with a warn in
/// v4 — no observable effect here).
fn get_selected_or_default_system_prompt(character: &Value, selected: Option<&str>) -> String {
    if let Some(sel) = selected {
        if let Some(prompts) = character.get("systemPrompts").and_then(Value::as_array) {
            if let Some(p) = prompts
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(sel))
            {
                return p
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
            }
        }
    }
    get_default_system_prompt(character)
}

/// The `TemplateCharacter` a character Value projects to (the template-processed
/// fields). `None` for absent/null string fields (the processor's `|| ''`
/// convention treats `None`/empty identically).
fn template_character(character: &Value) -> TemplateCharacter {
    TemplateCharacter {
        name: s(character, "name").unwrap_or_default(),
        description: s(character, "description"),
        manifesto: s(character, "manifesto"),
        personality: s(character, "personality"),
        first_message: s(character, "firstMessage"),
        example_dialogues: s(character, "exampleDialogues"),
    }
}

/// v4 `initialize.ts`'s own `buildSystemPrompt` — the flat single-shot prompt
/// written to the chat head. Distinct from the per-turn identity-stack builder.
fn build_system_prompt(
    character: &Value,
    user_character: Option<&UserCharacter>,
    scenario: Option<&str>,
    selected_system_prompt_id: Option<&str>,
) -> String {
    let character_name = s(character, "name").unwrap_or_default();
    let system_prompt_content =
        get_selected_or_default_system_prompt(character, selected_system_prompt_id);

    let tchar = template_character(character);
    let user_ct = user_character.map(|uc| TemplateUserCharacter {
        name: uc.name.clone(),
        description: uc.description.clone(),
    });
    let processed = process_character_templates(
        &tchar,
        user_ct.as_ref(),
        scenario,
        Some(&system_prompt_content),
    );

    let mut prompt = processed.system_prompt.clone();

    prompt.push_str(&format!("\n\nYou are roleplaying as {character_name}."));

    if !processed.description.is_empty() {
        prompt.push_str(&format!(
            "\n\nCharacter Description:\n{}",
            processed.description
        ));
    }
    if !processed.manifesto.is_empty() {
        prompt.push_str(&format!("\n\nManifesto:\n{}", processed.manifesto));
    }
    if !processed.personality.is_empty() {
        prompt.push_str(&format!("\n\nPersonality:\n{}", processed.personality));
    }

    if let Some(uc) = user_character {
        let alias_note = match &uc.aliases {
            Some(a) if !a.is_empty() => format!(" (also known as: {})", a.join(", ")),
            _ => String::new(),
        };
        let pronoun_note = match &uc.pronouns {
            Some(p) => format!(" (pronouns: {}/{}/{})", p.subject, p.object, p.possessive),
            None => String::new(),
        };
        prompt.push_str(&format!(
            "\n\nYou are talking to {}{}{}.",
            uc.name, alias_note, pronoun_note
        ));
        if let Some(desc) = uc.description.as_deref().filter(|d| !d.is_empty()) {
            // From the user character's perspective, the AI is the "user".
            let mut ctx = TemplateContext::default();
            ctx.set("char", uc.name.clone());
            ctx.set("user", character_name.clone());
            let processed_user_desc = process_template(desc, &ctx);
            prompt.push_str(&format!("\n{processed_user_desc}"));
        }
        if let Some(pers) = uc.personality.as_deref().filter(|p| !p.is_empty()) {
            let mut ctx = TemplateContext::default();
            ctx.set("char", uc.name.clone());
            ctx.set("user", character_name.clone());
            let processed_user_personality = process_template(pers, &ctx);
            prompt.push_str(&format!("\nThey are: {processed_user_personality}"));
        }
    }

    if !processed.scenario.is_empty() {
        prompt.push_str(&format!("\n\nScenario:\n{}", processed.scenario));
    }
    if !processed.example_dialogues.is_empty() {
        prompt.push_str(&format!(
            "\n\nExample Dialogue:\n{}",
            processed.example_dialogues
        ));
    }

    prompt.push_str(&format!(
        "\n\nStay in character at all times. Respond naturally and consistently with {character_name}'s personality and the current scenario."
    ));

    js_trim(&prompt).to_string()
}

/// Project a vault-overlaid character Value into the `UserCharacter` shape (only
/// when it is user-controlled — the caller has already checked `controlledBy`).
fn user_character_from_value(uc: &Value) -> UserCharacter {
    let aliases = uc
        .get("aliases")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .filter(|a| !a.is_empty());
    let pronouns = uc.get("pronouns").and_then(|p| {
        Some(Pronouns {
            subject: p.get("subject")?.as_str()?.to_string(),
            object: p.get("object")?.as_str()?.to_string(),
            possessive: p.get("possessive")?.as_str()?.to_string(),
        })
    });
    UserCharacter {
        id: s(uc, "id").unwrap_or_default(),
        name: s(uc, "name").unwrap_or_default(),
        aliases,
        pronouns,
        description: s(uc, "description"),
        manifesto: s(uc, "manifesto"),
        personality: s(uc, "personality"),
    }
}

/// v4 `buildChatContext`: resolve the seed bundle for a new chat. `main` +
/// `mount` are the two DB connections (`characters.findById` is vault-overlaid).
///
/// Errors: `Character not found` (v4 throws) when the responding character is
/// absent. The user-character lookup that misses / isn't user-controlled simply
/// yields `None` (v4's `if (uc && uc.controlledBy === 'user')`).
pub fn build_chat_context(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    user_character_id: Option<&str>,
    custom_scenario: Option<&str>,
    selected_system_prompt_id: Option<&str>,
) -> Result<ChatContext, DbError> {
    let character = characters_read::find_by_id(main, mount, character_id)?
        .ok_or_else(|| DbError::Key("Character not found".to_string()))?;

    // Resolve the user-controlled character (explicit id, else defaultPartnerId).
    let mut user_character: Option<UserCharacter> = None;
    if let Some(ucid) = user_character_id {
        if let Some(uc) = characters_read::find_by_id(main, mount, ucid)? {
            if uc.get("controlledBy").and_then(Value::as_str) == Some("user") {
                user_character = Some(user_character_from_value(&uc));
            }
        }
    } else if let Some(partner_id) = character.get("defaultPartnerId").and_then(Value::as_str) {
        if let Some(partner) = characters_read::find_by_id(main, mount, partner_id)? {
            if partner.get("controlledBy").and_then(Value::as_str) == Some("user") {
                user_character = Some(user_character_from_value(&partner));
            }
        }
    }

    // v4: `resolvedScenario = customScenario || undefined`.
    let scenario = custom_scenario.filter(|s| !s.is_empty());

    let system_prompt = build_system_prompt(
        &character,
        user_character.as_ref(),
        scenario,
        selected_system_prompt_id,
    );

    // First message: the processed character firstMessage (system prompt =
    // selected-or-default content, per v4).
    let default_system_prompt =
        get_selected_or_default_system_prompt(&character, selected_system_prompt_id);
    let tchar = template_character(&character);
    let user_ct = user_character.as_ref().map(|uc| TemplateUserCharacter {
        name: uc.name.clone(),
        description: uc.description.clone(),
    });
    let processed = process_character_templates(
        &tchar,
        user_ct.as_ref(),
        scenario,
        Some(&default_system_prompt),
    );
    let first_message = processed.first_message;

    Ok(ChatContext {
        system_prompt,
        first_message,
        character,
        user_character,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_system_prompt_selection() {
        let c = json!({
            "systemPrompts": [
                {"id": "a", "content": "Alpha", "isDefault": false},
                {"id": "b", "content": "Beta", "isDefault": true}
            ]
        });
        // isDefault wins.
        assert_eq!(get_default_system_prompt(&c), "Beta");
        // defaultSystemPromptId wins over isDefault.
        let c2 = json!({
            "defaultSystemPromptId": "a",
            "systemPrompts": [
                {"id": "a", "content": "Alpha", "isDefault": false},
                {"id": "b", "content": "Beta", "isDefault": true}
            ]
        });
        assert_eq!(get_default_system_prompt(&c2), "Alpha");
        // Empty → ''.
        assert_eq!(get_default_system_prompt(&json!({})), "");
        // Selected match.
        assert_eq!(
            get_selected_or_default_system_prompt(&c, Some("a")),
            "Alpha"
        );
        // Selected miss → default.
        assert_eq!(
            get_selected_or_default_system_prompt(&c, Some("zzz")),
            "Beta"
        );
    }

    #[test]
    fn flat_system_prompt_layout() {
        let c = json!({
            "name": "Aria",
            "description": "A brave knight",
            "personality": "bold",
            "systemPrompts": [{"id": "a", "content": "Base.", "isDefault": true}]
        });
        let prompt = build_system_prompt(&c, None, Some("A dark forest."), None);
        assert!(prompt.starts_with("Base.\n\nYou are roleplaying as Aria."));
        assert!(prompt.contains("\n\nCharacter Description:\nA brave knight"));
        assert!(prompt.contains("\n\nPersonality:\nbold"));
        assert!(prompt.contains("\n\nScenario:\nA dark forest."));
        assert!(prompt.contains("Stay in character at all times."));
        assert!(prompt.ends_with("Aria's personality and the current scenario."));
    }

    #[test]
    fn user_character_note_with_aliases_and_pronouns() {
        let c = json!({ "name": "Aria", "systemPrompts": [] });
        let uc = UserCharacter {
            id: "u1".into(),
            name: "Sam".into(),
            aliases: Some(vec!["Sammy".into(), "S".into()]),
            pronouns: Some(Pronouns {
                subject: "they".into(),
                object: "them".into(),
                possessive: "their".into(),
            }),
            description: Some("A traveler".into()),
            manifesto: None,
            personality: Some("curious".into()),
        };
        let prompt = build_system_prompt(&c, Some(&uc), None, None);
        assert!(prompt.contains(
            "You are talking to Sam (also known as: Sammy, S) (pronouns: they/them/their)."
        ));
        assert!(prompt.contains("\nA traveler"));
        assert!(prompt.contains("\nThey are: curious"));
    }
}
