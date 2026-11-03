//! The Host's off-scene character cards (v4 `lib/services/host-notifications/
//! writer.ts:840-1000`).
//!
//! An absent workspace character is described by ONE card —
//! [`OffSceneCardFields`] — wherever it appears: the referent ledger's
//! "Particulars of those not in the room" ([`crate::services::referent_ledger`]),
//! the Salon's name-chip popup ([`crate::services::absent_mentions`]), and
//! Continue Elsewhere's left-behind notice (the only Host off-scene message still
//! POSTED — [`crate::services::host_notifications`]). The per-turn "spoken of"
//! scan + post v4 deleted in `9a88f862f` is gone here too: the model hears about
//! an absent person from the ledger on every turn they are named, not from a
//! card wedged into the history.
//!
//! Which Host rows are records only is [`crate::services::off_scene_kinds`].

use serde::Serialize;
use serde_json::Value;

use crate::collation::locale_compare;
use crate::services::off_scene_kinds::{
    HOST_KIND_OFF_SCENE_CHARACTERS, HOST_KIND_OFF_SCENE_LEFT_BEHIND,
};

/// v4 `applyHostTemplates`: replace `{{char}}` / `{{user}}` with the supplied
/// names, leaving *unbound* tokens untouched (partial-binding contexts). No-op
/// when the text has no `{{`.
pub fn apply_host_templates(
    text: &str,
    char_name: Option<&str>,
    user_name: Option<&str>,
) -> String {
    if text.is_empty() || !text.contains("{{") {
        return text.to_string();
    }
    let mut result = text.to_string();
    if let Some(cn) = char_name {
        if !cn.is_empty() {
            result = result.replace("{{char}}", cn);
        }
    }
    if let Some(un) = user_name {
        if !un.is_empty() {
            result = result.replace("{{user}}", un);
        }
    }
    result
}

/// An off-scene character card (v4 `OffSceneCharacterCard`), built from a
/// user-character read.
#[derive(Clone, Debug)]
pub struct OffSceneCharacterCard {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    /// `(subject, object, possessive)`.
    pub pronouns: Option<(String, String, String)>,
    pub identity: Option<String>,
    pub description: Option<String>,
}

impl OffSceneCharacterCard {
    /// Build a card from an overlaid character `Value` (v4
    /// `characters.findByUserId(...)` rows). Absent optional fields → `None`.
    pub fn from_value(v: &Value) -> Option<Self> {
        let id = v.get("id").and_then(Value::as_str)?.to_string();
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let aliases = v
            .get("aliases")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let pronouns = v.get("pronouns").and_then(Value::as_object).and_then(|p| {
            let s = p.get("subject").and_then(Value::as_str)?.to_string();
            let o = p.get("object").and_then(Value::as_str)?.to_string();
            let poss = p.get("possessive").and_then(Value::as_str)?.to_string();
            Some((s, o, poss))
        });
        let identity = v
            .get("identity")
            .and_then(Value::as_str)
            .map(str::to_string);
        let description = v
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        Some(OffSceneCharacterCard {
            id,
            name,
            aliases,
            pronouns,
            identity,
            description,
        })
    }
}

/// Why the Host is introducing these characters (v4 `OffSceneIntroductionReason`,
/// `acadcc7cd`):
///
/// - [`Mentioned`](Self::Mentioned) — named in the conversation but never seated
///   here. **No longer posted** (v4 `9a88f862f`): the referent ledger carries the
///   card to the model and the name chip to the operator. Kept, as v4 keeps the
///   type, with the writer that still takes it. The default.
/// - [`LeftBehind`](Self::LeftBehind) — seated in the chat this one continues
///   from, but not brought along. The carried-over transcript still has the cast
///   talking to them, and their own lines were dropped in the replay, so without
///   this they read as present and silent (bug 171).
///
/// On the wire through [`Self::system_kind`] (v4 `d68c1af88`, `writer.ts:989`):
/// a left-behind notice is written `off-scene-left-behind` — it stays in the
/// model's history — and a mention introduction `off-scene-characters`, which
/// the history filters drop as a record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OffSceneIntroductionReason {
    #[default]
    Mentioned,
    LeftBehind,
}

impl OffSceneIntroductionReason {
    /// v4's string form (`'mentioned' | 'left-behind'`).
    pub fn as_str(self) -> &'static str {
        match self {
            OffSceneIntroductionReason::Mentioned => "mentioned",
            OffSceneIntroductionReason::LeftBehind => "left-behind",
        }
    }

    /// The posted row's `systemKind` (v4 `reason === 'left-behind' ?
    /// HOST_KIND_OFF_SCENE_LEFT_BEHIND : HOST_KIND_OFF_SCENE_CHARACTERS`).
    pub fn system_kind(self) -> &'static str {
        match self {
            OffSceneIntroductionReason::Mentioned => HOST_KIND_OFF_SCENE_CHARACTERS,
            OffSceneIntroductionReason::LeftBehind => HOST_KIND_OFF_SCENE_LEFT_BEHIND,
        }
    }
}

/// v4 `OffSceneCardFields` (`writer.ts:847-853`) — what an absent character's
/// card says, as data. The one source of the card: the ledger's particulars,
/// the name chip's popup and the left-behind notice all render or ship THIS.
/// Key order is the wire contract (`name, aliases, pronouns, body`); `pronouns`
/// is `null`, never absent, `aliases` `[]` and `body` `''` when there is none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OffSceneCardFields {
    pub name: String,
    pub aliases: Vec<String>,
    /// `subject/object/possessive`, or `null`.
    pub pronouns: Option<String>,
    pub body: String,
}

/// v4 `offSceneCardFields(c, userCharacterName)` (`writer.ts:855-866`): the body
/// is the trimmed identity, else the trimmed description, with `{{char}}` /
/// `{{user}}` bound — and `''` when both are empty.
pub fn off_scene_card_fields(
    c: &OffSceneCharacterCard,
    user_character_name: Option<&str>,
) -> OffSceneCardFields {
    // `(c.identity ?? '').trim() || (c.description ?? '').trim()`.
    let identity_trim = c
        .identity
        .as_deref()
        .map(crate::jsstr::js_trim)
        .unwrap_or("");
    let body = if !identity_trim.is_empty() {
        identity_trim
    } else {
        c.description
            .as_deref()
            .map(crate::jsstr::js_trim)
            .unwrap_or("")
    };
    OffSceneCardFields {
        name: c.name.clone(),
        aliases: c.aliases.clone(),
        pronouns: c.pronouns.as_ref().map(|(s, o, p)| format!("{s}/{o}/{p}")),
        body: if body.is_empty() {
            String::new()
        } else {
            apply_host_templates(body, Some(&c.name), user_character_name)
        },
    }
}

/// v4 `renderOffSceneCard` (`writer.ts:868-877`, exported since `d68c1af88`):
/// `### <name>`, then `Aliases: …` / `Pronouns: …` when present, then the body
/// when non-empty — rendered FROM [`off_scene_card_fields`], joined `\n`.
pub fn render_off_scene_card(
    c: &OffSceneCharacterCard,
    user_character_name: Option<&str>,
) -> String {
    let fields = off_scene_card_fields(c, user_character_name);
    let mut lines: Vec<String> = vec![format!("### {}", fields.name)];
    if !fields.aliases.is_empty() {
        lines.push(format!("Aliases: {}", fields.aliases.join(", ")));
    }
    if let Some(pronouns) = &fields.pronouns {
        lines.push(format!("Pronouns: {pronouns}"));
    }
    if !fields.body.is_empty() {
        lines.push(fields.body);
    }
    lines.join("\n")
}

/// v4 `buildOffSceneCharactersContent`: the persona-voiced Host introduction
/// (sorted alphabetically by name via `localeCompare`).
pub fn build_off_scene_characters_content(
    characters: &[OffSceneCharacterCard],
    user_character_name: Option<&str>,
    reason: OffSceneIntroductionReason,
) -> String {
    let mut sorted: Vec<&OffSceneCharacterCard> = characters.iter().collect();
    sorted.sort_by(|a, b| locale_compare(&a.name, &b.name));

    let intro = match (reason, sorted.len() == 1) {
        (OffSceneIntroductionReason::LeftBehind, true) => "The Host observes that one member of the previous company did not make the journey. This person remained behind in the earlier scene: not in this room, unable to hear what is said here, and unable to answer. Mentioned for accurate reference only; not a summons to the scene.",
        (OffSceneIntroductionReason::LeftBehind, false) => "The Host observes that certain members of the previous company did not make the journey. These persons remained behind in the earlier scene: not in this room, unable to hear what is said here, and unable to answer. Mentioned for accurate reference only; not a summons to the scene.",
        (OffSceneIntroductionReason::Mentioned, true) => "The Host begs leave to introduce a person spoken of in this conversation but not presently in the Salon — for accurate reference only; not a summons to the scene.",
        (OffSceneIntroductionReason::Mentioned, false) => "The Host begs leave to introduce certain persons spoken of in this conversation but not presently in the Salon — for accurate reference only; not a summons to the scene.",
    };

    let mut parts: Vec<String> = vec![intro.to_string(), String::new()];
    parts.extend(
        sorted
            .iter()
            .map(|c| render_off_scene_card(c, user_character_name)),
    );
    parts.join("\n\n")
}

/// v4 `buildOffSceneCharactersOpaqueContent`: the persona-stripped form (used by
/// the persisted message's opaque swap — W4.6b). Ported for the differential.
pub fn build_off_scene_characters_opaque_content(
    characters: &[OffSceneCharacterCard],
    user_character_name: Option<&str>,
    reason: OffSceneIntroductionReason,
) -> String {
    let mut sorted: Vec<&OffSceneCharacterCard> = characters.iter().collect();
    sorted.sort_by(|a, b| locale_compare(&a.name, &b.name));

    let intro = match (reason, sorted.len() == 1) {
        (OffSceneIntroductionReason::LeftBehind, true) => "A person from the previous scene who did not come along. They are not in this room, cannot hear what is said here, and cannot answer — for accurate reference only; not a summons to the scene:",
        (OffSceneIntroductionReason::LeftBehind, false) => "Persons from the previous scene who did not come along. They are not in this room, cannot hear what is said here, and cannot answer — for accurate reference only; not a summons to the scene:",
        (OffSceneIntroductionReason::Mentioned, true) => "A person spoken of in this conversation but not presently in the scene — for accurate reference only; not a summons to the scene:",
        (OffSceneIntroductionReason::Mentioned, false) => "Persons spoken of in this conversation but not presently in the scene — for accurate reference only; not a summons to the scene:",
    };

    let mut parts: Vec<String> = vec![intro.to_string(), String::new()];
    parts.extend(
        sorted
            .iter()
            .map(|c| render_off_scene_card(c, user_character_name)),
    );
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::off_scene_kinds::LEFT_BEHIND_INTRO_PREFIX;

    fn card(id: &str, name: &str) -> OffSceneCharacterCard {
        OffSceneCharacterCard {
            id: id.into(),
            name: name.into(),
            aliases: vec![],
            pronouns: None,
            identity: None,
            description: None,
        }
    }

    #[test]
    fn apply_host_templates_substitutes_and_preserves_unbound() {
        assert_eq!(
            apply_host_templates("Hi {{char}}, from {{user}}.", Some("Ada"), Some("Bob")),
            "Hi Ada, from Bob."
        );
        // Unbound {{user}} preserved when user_name is None.
        assert_eq!(
            apply_host_templates("Hi {{char}} & {{user}}.", Some("Ada"), None),
            "Hi Ada & {{user}}."
        );
        // No `{{` → verbatim.
        assert_eq!(
            apply_host_templates("plain", Some("Ada"), Some("Bob")),
            "plain"
        );
    }

    #[test]
    fn render_card_identity_over_description() {
        let mut c = card("id1", "Ada");
        c.identity = Some("  {{char}} is a coder.  ".into());
        c.description = Some("ignored".into());
        assert_eq!(
            render_off_scene_card(&c, Some("Bob")),
            "### Ada\nAda is a coder."
        );
    }

    #[test]
    fn render_card_aliases_pronouns_description_fallback() {
        let mut c = card("id1", "Ada");
        c.aliases = vec!["A".into(), "Ace".into()];
        c.pronouns = Some(("she".into(), "her".into(), "hers".into()));
        c.description = Some("A brave soul.".into());
        assert_eq!(
            render_off_scene_card(&c, None),
            "### Ada\nAliases: A, Ace\nPronouns: she/her/hers\nA brave soul."
        );
    }

    #[test]
    fn build_content_single_vs_plural_and_sort() {
        let single = build_off_scene_characters_content(
            std::slice::from_ref(&card("i", "Zed")),
            None,
            OffSceneIntroductionReason::Mentioned,
        );
        assert!(single.starts_with("The Host begs leave to introduce a person"));
        assert!(single.ends_with("### Zed"));

        let plural = build_off_scene_characters_content(
            &[card("i2", "Zed"), card("i1", "Ada")],
            None,
            OffSceneIntroductionReason::Mentioned,
        );
        assert!(plural.starts_with("The Host begs leave to introduce certain persons"));
        // Alphabetical: Ada before Zed.
        let ada_pos = plural.find("### Ada").unwrap();
        let zed_pos = plural.find("### Zed").unwrap();
        assert!(ada_pos < zed_pos);
    }

    #[test]
    fn left_behind_intros_open_with_the_prefix() {
        // v4 builds both left-behind intros from `LEFT_BEHIND_INTRO_PREFIX`; the
        // legacy rows are told apart by it, so the bytes must keep opening so.
        for n in [1usize, 2] {
            let cards: Vec<OffSceneCharacterCard> =
                (0..n).map(|i| card(&format!("i{i}"), "Ada")).collect();
            let s = build_off_scene_characters_content(
                &cards,
                None,
                OffSceneIntroductionReason::LeftBehind,
            );
            assert!(s.starts_with(LEFT_BEHIND_INTRO_PREFIX), "{s}");
        }
        assert_eq!(
            OffSceneIntroductionReason::LeftBehind.system_kind(),
            "off-scene-left-behind"
        );
        assert_eq!(
            OffSceneIntroductionReason::Mentioned.system_kind(),
            "off-scene-characters"
        );
    }

    #[test]
    fn card_fields_key_order_and_nulls() {
        let c = card("id1", "Ada");
        assert_eq!(
            serde_json::to_string(&off_scene_card_fields(&c, None)).unwrap(),
            r#"{"name":"Ada","aliases":[],"pronouns":null,"body":""}"#
        );
    }
}
