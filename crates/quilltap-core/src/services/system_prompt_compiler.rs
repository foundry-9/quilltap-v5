//! The system-prompt compiler write side (Phase H) — v4's
//! `lib/services/system-prompt-compiler/compiler.ts`.
//!
//! Precompiles the per-participant character-identity stack (the ported
//! [`crate::system_prompt::build_identity_stack`]) with the chat-level
//! `{{user}}`/`{{scenario}}`/`{{persona}}` template variables resolved, and
//! persists the `{participantId → stack}` map to `chats.compiledIdentityStacks`.
//! The per-turn `build_system_prompt` already CONSUMES the cached stack (via its
//! `precompiled_identity_stack` input) with a read-through fallback, so this is
//! the WRITE half — the cache lets the LLM provider see a stable cache-friendly
//! prefix across turns.
//!
//! Only [`compile_all_identity_stacks`] (chat creation / whole-chat changes) is
//! ported here — the create handler's sole caller. The single-participant
//! `compileIdentityStackForParticipant` (participant add / prompt change) is a
//! participant-`?action=` deferral (P4.6).
//!
//! Errors never propagate past the create handler: v4 wraps the whole call in a
//! try/catch, `writeStacks` swallows its own update error, and
//! `resolveUserCharacter` swallows read errors. This port keeps that shape — the
//! persist failure is swallowed here; a character-read error propagates out (as
//! v4's `buildStackFor` lets `findById` throw) for the spine's try/catch to
//! swallow.

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::{characters_read, DbError};
use crate::system_prompt::{
    build_identity_stack, BuildIdentityStackOptions, Character, PhysicalDescription, Pronouns,
    ScenarioEntry, SystemPromptEntry, UserCharacter,
};

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn str_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Project a vault-overlaid character Value into the [`Character`] subset the
/// identity-stack builder reads (INCLUDING `physicalDescription`, which the
/// stack surfaces — unlike the carina card).
fn character_from_value(c: &Value) -> Character {
    let pronouns = c.get("pronouns").and_then(|p| {
        Some(Pronouns {
            subject: p.get("subject")?.as_str()?.to_string(),
            object: p.get("object")?.as_str()?.to_string(),
            possessive: p.get("possessive")?.as_str()?.to_string(),
        })
    });
    let physical_description = c.get("physicalDescription").and_then(|pd| {
        if !pd.is_object() {
            return None;
        }
        Some(PhysicalDescription {
            name: s(pd, "name").unwrap_or_default(),
            usage_context: s(pd, "usageContext"),
            short_prompt: s(pd, "shortPrompt"),
            medium_prompt: s(pd, "mediumPrompt"),
            long_prompt: s(pd, "longPrompt"),
            complete_prompt: s(pd, "completePrompt"),
            full_description: s(pd, "fullDescription"),
        })
    });
    let scenarios = c
        .get("scenarios")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|sc| ScenarioEntry {
                    content: s(sc, "content").unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    let system_prompts = c
        .get("systemPrompts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|sp| SystemPromptEntry {
                    id: s(sp, "id").unwrap_or_default(),
                    content: s(sp, "content").unwrap_or_default(),
                    is_default: sp
                        .get("isDefault")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    Character {
        name: s(c, "name").unwrap_or_default(),
        title: s(c, "title"),
        identity: s(c, "identity"),
        description: s(c, "description"),
        manifesto: s(c, "manifesto"),
        personality: s(c, "personality"),
        aliases: str_array(c, "aliases"),
        pronouns,
        physical_description,
        example_dialogues: s(c, "exampleDialogues"),
        scenarios,
        system_prompts,
    }
}

/// v4 `resolveUserCharacter`: the first user-controlled CHARACTER participant's
/// `{name, description}`, used to populate `{{user}}`/`{{persona}}`. Read errors
/// are swallowed to `None` (v4's try/catch).
fn resolve_user_character(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
) -> Option<UserCharacter> {
    let participants = chat.get("participants").and_then(Value::as_array)?;
    let user_participant = participants.iter().find(|p| {
        p.get("type").and_then(Value::as_str) == Some("CHARACTER")
            && p.get("controlledBy").and_then(Value::as_str) == Some("user")
            && p.get("characterId").and_then(Value::as_str).is_some()
    })?;
    let character_id = user_participant
        .get("characterId")
        .and_then(Value::as_str)?;
    // v4 swallows a read error to null.
    let character = characters_read::find_by_id(main, mount, character_id).ok()??;
    Some(UserCharacter {
        name: s(&character, "name").unwrap_or_default(),
        description: s(&character, "description").unwrap_or_default(),
    })
}

/// v4 `buildStackFor`: the identity stack for one eligible LLM-controlled
/// participant, or `None` when it isn't eligible (user-controlled, no
/// characterId, removed, or the character lookup missed / produced an empty
/// stack). A character-read error propagates (v4 lets `findById` throw).
fn build_stack_for(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
    participant: &Value,
) -> Result<Option<String>, DbError> {
    let is_character = participant.get("type").and_then(Value::as_str) == Some("CHARACTER");
    let is_user = participant.get("controlledBy").and_then(Value::as_str) == Some("user");
    let character_id = participant.get("characterId").and_then(Value::as_str);
    let is_removed = participant.get("status").and_then(Value::as_str) == Some("removed");
    if !is_character || is_user || character_id.is_none() || is_removed {
        return Ok(None);
    }
    let character_id = character_id.unwrap();

    let Some(character_value) = characters_read::find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };
    let character = character_from_value(&character_value);
    let user_character = resolve_user_character(main, mount, chat);
    let scenario_text = chat.get("scenarioText").and_then(Value::as_str);
    let selected_system_prompt_id = participant
        .get("selectedSystemPromptId")
        .and_then(Value::as_str);

    let stack = build_identity_stack(&BuildIdentityStackOptions {
        character: &character,
        user_character: user_character.as_ref(),
        selected_system_prompt_id,
        scenario_text,
    });
    Ok(if stack.is_empty() { None } else { Some(stack) })
}

/// Persist the compiled map (v4 `writeStacks`): `chats.update({
/// compiledIdentityStacks: keys > 0 ? stacks : null })`. Swallows its own update
/// error (the read-through fallback covers any miss). Does not bump `updatedAt`.
fn write_stacks(main: &Connection, chat_id: &str, stacks: Map<String, Value>) {
    let value = if stacks.is_empty() {
        Value::Null
    } else {
        Value::Object(stacks)
    };
    let repo = ChatsRepository::new(main);
    let _ = repo.update(
        chat_id,
        &ChatUpdate {
            compiled_identity_stacks: Some(value),
            ..Default::default()
        },
    );
}

/// v4 `compileAllIdentityStacks`: compile every LLM-controlled CHARACTER
/// participant's identity stack and persist the map. `main` is the writer
/// connection (chats + the character slim rows live there); `mount` is the
/// mount-index connection for the vault overlay.
///
/// A character-read error propagates (for the spine's try/catch); the persist
/// itself is best-effort.
pub fn compile_all_identity_stacks(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
) -> Result<(), DbError> {
    let chat_id = chat
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| DbError::Key("chat has no id".to_string()))?;

    let mut stacks = Map::new();
    if let Some(participants) = chat.get("participants").and_then(Value::as_array) {
        for participant in participants {
            if let Some(stack) = build_stack_for(main, mount, chat, participant)? {
                if let Some(pid) = participant.get("id").and_then(Value::as_str) {
                    stacks.insert(pid.to_string(), Value::String(stack));
                }
            }
        }
    }

    write_stacks(main, chat_id, stacks);
    Ok(())
}
