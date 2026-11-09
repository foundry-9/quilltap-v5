//! Character kind — how a character is performed (v4 `lib/characters/kind.ts`,
//! `b09ca46bb` + `96156566b` / `3ae4cf582`, ported whole at `624d0d25c`).
//!
//! `roleplay` (the default) is a person inside a story: they have stakes, they
//! react to what happens, and their tools are things they do in the scene.
//! `assistant` is a character whose job is to do work for the operator —
//! research, drafting, organizing — while keeping a personality.
//!
//! Kind describes the performance, not the fiction. An AI in the story (a
//! ship's mind, a household system) who lives in the scene is still
//! `roleplay`.
//!
//! Stored in the character vault's `properties.json` beside the other
//! per-character settings, so it travels with the vault, exports and shared
//! vaults. Absent, unrecognized or unreadable values read as `roleplay`.
//!
//! This module is the one place a kind is read or compared. Callers ask
//! [`resolve_character_kind`] / [`is_assistant_character`] — and branch on the
//! [`KindPolicy`] fields [`character_kind_policy`] derives — rather than
//! testing the raw field or the enum, so a third kind, or a change to the
//! default, lands here alone. A source census
//! (`crates/quilltap-harness/tests/character_kind_chokepoint_census.rs`, the
//! twin of v4's `kind-chokepoint.test.ts`) holds that line.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// v4 `CHARACTER_KINDS`.
pub const CHARACTER_KINDS: [&str; 2] = ["roleplay", "assistant"];

/// v4 `DEFAULT_CHARACTER_KIND`.
pub const DEFAULT_CHARACTER_KIND: CharacterKind = CharacterKind::Roleplay;

/// v4 `CharacterKind`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CharacterKind {
    #[default]
    #[serde(rename = "roleplay")]
    Roleplay,
    #[serde(rename = "assistant")]
    Assistant,
}

impl CharacterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CharacterKind::Roleplay => "roleplay",
            CharacterKind::Assistant => "assistant",
        }
    }
}

/// v4 `characterKindOrDefault`: normalize any stored value to a kind; anything
/// unrecognized is the default. EXACT match — `"Assistant"`, `" assistant"`,
/// `""`, `null`, a number, an array or an object all read as `roleplay`.
/// `None` is v4's `undefined` (an absent key).
pub fn kind_or_default(value: Option<&Value>) -> CharacterKind {
    match value.and_then(Value::as_str) {
        Some("assistant") => CharacterKind::Assistant,
        // `roleplay`, any other string, absent, null, a number, an object …
        _ => DEFAULT_CHARACTER_KIND,
    }
}

/// v4 `resolveCharacterKind`: the kind of a character. A character read
/// without its vault (a raw row) or from a vault written before kinds existed
/// resolves to the default. v4's `character?.kind` on a non-object (a bare
/// string or number) reads `undefined`, so a non-object Value is "no kind".
pub fn resolve_character_kind(character: Option<&Value>) -> CharacterKind {
    kind_or_default(
        character
            .and_then(Value::as_object)
            .and_then(|o| o.get("kind")),
    )
}

/// v4 `isAssistantCharacter`: whether a character is performed as an
/// assistant rather than a roleplay character. (No v4 production caller at the
/// pin — ported for parity.)
pub fn is_assistant_character(character: Option<&Value>) -> bool {
    resolve_character_kind(character) == CharacterKind::Assistant
}

/// v4 `AgentModeFlavor`: what Agent Mode means for a character. `research` is
/// the assistant's loop: iterate, verify, and finish with a structured
/// `submit_final_response`. `in-character` makes tools the character's hands in
/// the scene: act, then write the turn in prose, with no summary and no
/// `submit_final_response`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentModeFlavor {
    #[serde(rename = "research")]
    Research,
    #[serde(rename = "in-character")]
    InCharacter,
}

impl AgentModeFlavor {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentModeFlavor::Research => "research",
            AgentModeFlavor::InCharacter => "in-character",
        }
    }
}

/// v4 `ToolRulesVariant`: which tool-rules preamble a character hears. Both
/// keep every rule against narrating a tool call instead of making it;
/// `roleplay` drops the claim that the rules override the character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolRulesVariant {
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "roleplay")]
    Roleplay,
}

impl ToolRulesVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolRulesVariant::Assistant => "assistant",
            ToolRulesVariant::Roleplay => "roleplay",
        }
    }
}

/// v4 `GeneratorDirectionKey`: which extra direction a prompt generator (AI
/// Wizard, Summon From Lore, the Character Optimizer, the External Prompt
/// generator) adds for this kind. `stakes` is how a story character reacts
/// when the story turns; `task` is how an assistant does the work. The prose
/// lives in the generators' field semantics (v4 `generatorDirectionFor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GeneratorDirectionKey {
    #[serde(rename = "stakes")]
    Stakes,
    #[serde(rename = "task")]
    Task,
}

impl GeneratorDirectionKey {
    pub fn as_str(self) -> &'static str {
        match self {
            GeneratorDirectionKey::Stakes => "stakes",
            GeneratorDirectionKey::Task => "task",
        }
    }
}

/// v4 `KindPolicy`: everything a kind decides, derived in one place. Field
/// order is v4's interface order (the serialized bytes are compared).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindPolicy {
    pub kind: CharacterKind,
    pub agent_flavor: AgentModeFlavor,
    pub tool_rules: ToolRulesVariant,
    pub generator_direction: GeneratorDirectionKey,
}

/// v4 `KIND_POLICIES` — an exhaustive `match` (no `_` arm), so a third
/// `CharacterKind` variant fails to compile until its policy lands HERE: v4's
/// "a third kind, or a change to the default, lands here alone".
const fn kind_policies(kind: CharacterKind) -> KindPolicy {
    match kind {
        CharacterKind::Roleplay => KindPolicy {
            kind: CharacterKind::Roleplay,
            agent_flavor: AgentModeFlavor::InCharacter,
            tool_rules: ToolRulesVariant::Roleplay,
            generator_direction: GeneratorDirectionKey::Stakes,
        },
        CharacterKind::Assistant => KindPolicy {
            kind: CharacterKind::Assistant,
            agent_flavor: AgentModeFlavor::Research,
            tool_rules: ToolRulesVariant::Assistant,
            generator_direction: GeneratorDirectionKey::Task,
        },
    }
}

/// v4 `kindPolicy`: the behaviour a kind decides. The only place a kind turns
/// into behaviour. (v4 re-normalizes through `characterKindOrDefault` so a bad
/// runtime value gets roleplay's policy; the Rust enum cannot hold one.)
pub fn kind_policy(kind: CharacterKind) -> KindPolicy {
    kind_policies(kind)
}

/// v4 `characterKindPolicy`: the behaviour a character's kind decides.
pub fn character_kind_policy(character: Option<&Value>) -> KindPolicy {
    kind_policy(resolve_character_kind(character))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_to_roleplay() {
        assert_eq!(DEFAULT_CHARACTER_KIND, CharacterKind::Roleplay);
        assert_eq!(DEFAULT_CHARACTER_KIND, CharacterKind::default());
        assert_eq!(CHARACTER_KINDS, ["roleplay", "assistant"]);
        assert_eq!(
            CHARACTER_KINDS,
            [
                CharacterKind::Roleplay.as_str(),
                CharacterKind::Assistant.as_str()
            ]
        );
    }

    #[test]
    fn normalizes_any_stored_value() {
        let cases: [(Option<Value>, CharacterKind); 8] = [
            (Some(json!("roleplay")), CharacterKind::Roleplay),
            (Some(json!("assistant")), CharacterKind::Assistant),
            (None, CharacterKind::Roleplay),
            (Some(json!(null)), CharacterKind::Roleplay),
            (Some(json!("")), CharacterKind::Roleplay),
            (Some(json!("Assistant")), CharacterKind::Roleplay),
            (Some(json!("butler")), CharacterKind::Roleplay),
            (Some(json!(1)), CharacterKind::Roleplay),
        ];
        for (value, expected) in &cases {
            assert_eq!(kind_or_default(value.as_ref()), *expected, "{value:?}");
        }
    }

    #[test]
    fn resolves_a_character_with_no_kind_as_roleplay() {
        assert_eq!(
            resolve_character_kind(Some(&json!({}))),
            CharacterKind::Roleplay
        );
        assert_eq!(
            resolve_character_kind(Some(&json!(null))),
            CharacterKind::Roleplay
        );
        assert_eq!(resolve_character_kind(None), CharacterKind::Roleplay);
        // `character?.kind` on a bare string is `undefined`.
        assert_eq!(
            resolve_character_kind(Some(&json!("assistant"))),
            CharacterKind::Roleplay
        );
    }

    #[test]
    fn resolves_a_stored_kind() {
        assert_eq!(
            resolve_character_kind(Some(&json!({"kind": "assistant"}))),
            CharacterKind::Assistant
        );
        assert_eq!(
            resolve_character_kind(Some(&json!({"kind": "roleplay"}))),
            CharacterKind::Roleplay
        );
    }

    #[test]
    fn answers_whether_a_character_is_an_assistant() {
        assert!(is_assistant_character(Some(&json!({"kind": "assistant"}))));
        assert!(!is_assistant_character(Some(&json!({"kind": "roleplay"}))));
        assert!(!is_assistant_character(Some(&json!({"kind": "wizard"}))));
        assert!(!is_assistant_character(None));
    }

    #[test]
    fn derives_the_policy_for_each_kind() {
        assert_eq!(
            serde_json::to_string(&kind_policy(CharacterKind::Roleplay)).unwrap(),
            r#"{"kind":"roleplay","agentFlavor":"in-character","toolRules":"roleplay","generatorDirection":"stakes"}"#
        );
        assert_eq!(
            serde_json::to_string(&kind_policy(CharacterKind::Assistant)).unwrap(),
            r#"{"kind":"assistant","agentFlavor":"research","toolRules":"assistant","generatorDirection":"task"}"#
        );
    }

    #[test]
    fn derives_a_characters_policy_defaulting_to_roleplay() {
        assert_eq!(
            character_kind_policy(Some(&json!({"kind": "assistant"}))).agent_flavor,
            AgentModeFlavor::Research
        );
        assert_eq!(
            character_kind_policy(Some(&json!({}))).agent_flavor,
            AgentModeFlavor::InCharacter
        );
        assert_eq!(
            character_kind_policy(None).tool_rules,
            ToolRulesVariant::Roleplay
        );
    }

    /// A third kind lands here alone: every variant has a policy whose `kind`
    /// is itself. `kind_policies` is an exhaustive `match` with no `_` arm, so
    /// adding a variant without its policy is a compile error, and this test
    /// enumerates the variants through an exhaustive `match` too.
    #[test]
    fn every_kind_has_its_own_policy() {
        fn all() -> [CharacterKind; 2] {
            // Exhaustive: a new variant fails to compile here until listed.
            match CharacterKind::Roleplay {
                CharacterKind::Roleplay | CharacterKind::Assistant => {}
            }
            [CharacterKind::Roleplay, CharacterKind::Assistant]
        }
        for k in all() {
            assert_eq!(kind_policy(k).kind, k);
            assert_eq!(kind_or_default(Some(&json!(k.as_str()))), k);
            assert_eq!(
                serde_json::to_string(&k).unwrap(),
                format!("\"{}\"", k.as_str())
            );
        }
        assert_eq!(all().len(), CHARACTER_KINDS.len());
    }

    #[test]
    fn the_enums_spell_v4s_values() {
        assert_eq!(AgentModeFlavor::InCharacter.as_str(), "in-character");
        assert_eq!(
            serde_json::to_string(&AgentModeFlavor::InCharacter).unwrap(),
            "\"in-character\""
        );
        assert_eq!(AgentModeFlavor::Research.as_str(), "research");
        assert_eq!(ToolRulesVariant::Assistant.as_str(), "assistant");
        assert_eq!(ToolRulesVariant::Roleplay.as_str(), "roleplay");
        assert_eq!(GeneratorDirectionKey::Stakes.as_str(), "stakes");
        assert_eq!(GeneratorDirectionKey::Task.as_str(), "task");
    }
}
