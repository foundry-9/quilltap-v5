//! Absent characters named in a chat, for the Salon's name chips (v4
//! `lib/chat/absent-mentions.ts`, `9a88f862f`).
//!
//! The Host used to post a card into the conversation the first time an absent
//! workspace character was named. The model now gets that card from the
//! referent ledger's trailing section instead
//! ([`crate::services::referent_ledger`]), and the operator gets it from a chip
//! on the name, at its first mention in the chat, with the card in a hover
//! popup. This module answers the chip's two questions — which message first
//! names each absent character, and what their card says — from the same
//! matcher ([`crate::name_mentions`]) and the same card fields
//! ([`crate::services::off_scene::off_scene_card_fields`]) the model's side
//! uses, so what the operator reads is what the model was told.
//!
//! "Absent" is [`presence_of`]: seated (not removed) characters are in the
//! room, and so is an unseated persona wherever the operator speaks without a
//! seat (in an autonomous room nobody speaks as them, so they are absent, bug
//! 172). The context builder's ledger asks the same function.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;

use crate::chat_predicates::operator_speaks_without_seat;
use crate::chat_tasks::strip_tool_artifacts;
use crate::jsstr::js_trim;
use crate::name_mentions::{build_name_matcher, first_name_match};
use crate::services::off_scene::{
    off_scene_card_fields, OffSceneCardFields, OffSceneCharacterCard,
};

const CONTEXT: &str = "chat.absent-mentions";

/// One chip: where an absent character is first named, and their card. Key
/// order is the wire contract (`messageId, characterId, text, card`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AbsentMention {
    pub message_id: String,
    pub character_id: String,
    /// The name or alias exactly as it first appears in that message.
    pub text: String,
    pub card: OffSceneCardFields,
}

/// Who is in the room (v4 `presenceOf`'s closure, as data).
#[derive(Clone, Debug, Default)]
pub struct Presence {
    seated: HashSet<String>,
    persona: Option<String>,
}

impl Presence {
    /// v4 `c => seated.has(c.id) || (!!persona && c.name.trim().toLowerCase()
    /// === persona)`.
    pub fn is_in_room(&self, id: &str, name: &str) -> bool {
        self.seated.contains(id)
            || self
                .persona
                .as_deref()
                .is_some_and(|p| !p.is_empty() && js_trim(name).to_lowercase() == p)
    }
}

/// v4 `presenceOf({participants, chatType, personaName})`. Seated (not removed)
/// characters always; an unseated persona, matched by name, only where the
/// operator speaks without a seat.
pub fn presence_of(
    participants: &[Value],
    chat_type: Option<&str>,
    persona_name: Option<&str>,
) -> Presence {
    let seated = participants
        .iter()
        .filter(|p| p.get("status").and_then(Value::as_str) != Some("removed"))
        .filter_map(|p| p.get("characterId").and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    let persona = if operator_speaks_without_seat(chat_type) {
        persona_name.map(|n| js_trim(n).to_lowercase())
    } else {
        None
    };
    Presence { seated, persona }
}

/// v4 `spokenText(m)` — a line someone actually said: no Staff message, no tool
/// chatter. `None` for a non-`message` row, a Staff row, an empty row, any role
/// but USER / ASSISTANT, or an assistant row that was only tool markup.
pub fn spoken_text(
    type_: Option<&str>,
    role: Option<&str>,
    content: Option<&str>,
    system_sender: Option<&str>,
) -> Option<String> {
    if type_.is_some_and(|t| t != "message") {
        return None;
    }
    // `m.systemSender || !m.content` — JS truthiness.
    if system_sender.is_some_and(|s| !s.is_empty()) {
        return None;
    }
    let content = content.filter(|c| !c.is_empty())?;
    match role.unwrap_or("").to_uppercase().as_str() {
        "USER" => Some(content.to_string()),
        "ASSISTANT" => strip_tool_artifacts(content).filter(|s| !s.is_empty()),
        _ => None,
    }
}

/// [`spoken_text`] over a raw chat-message row.
pub fn spoken_text_of(m: &Value) -> Option<String> {
    let s = |k: &str| m.get(k).and_then(Value::as_str);
    spoken_text(s("type"), s("role"), s("content"), s("systemSender"))
}

/// v4 `findAbsentMentions({messages, candidates, isInRoom, userCharacterName})`
/// — the first mention of each absent candidate, in message order. Pure: the
/// caller supplies the messages (oldest first), the candidates and presence.
///
/// Within one message the PENDING list is walked from its end (v4's `for (i =
/// pending.length - 1; …)` with a `splice`), so two absent people first named in
/// the same message come out in the reverse of the candidates' order. A row
/// without an `id` is skipped; the scan stops once nobody is pending.
pub fn find_absent_mentions(
    messages: &[Value],
    candidates: &[OffSceneCharacterCard],
    presence: &Presence,
    user_character_name: Option<&str>,
) -> Vec<AbsentMention> {
    let mut pending: Vec<(&OffSceneCharacterCard, crate::name_mentions::NameMatcher)> = candidates
        .iter()
        .filter(|c| !presence.is_in_room(&c.id, &c.name))
        .filter_map(|c| {
            let names = std::iter::once(Some(c.name.as_str()))
                .chain(c.aliases.iter().map(|a| Some(a.as_str())));
            build_name_matcher(names).map(|m| (c, m))
        })
        .collect();

    let mut found: Vec<AbsentMention> = Vec::new();
    for m in messages {
        if pending.is_empty() {
            break;
        }
        // `if (!m.id) continue` — JS truthiness.
        let Some(message_id) = m
            .get("id")
            .and_then(Value::as_str)
            .filter(|i| !i.is_empty())
        else {
            continue;
        };
        let Some(text) = spoken_text_of(m) else {
            continue;
        };
        let mut i = pending.len();
        while i > 0 {
            i -= 1;
            let Some(hit) = first_name_match(&text, &pending[i].1) else {
                continue;
            };
            let c = pending[i].0;
            found.push(AbsentMention {
                message_id: message_id.to_string(),
                character_id: c.id.clone(),
                text: hit.text,
                card: off_scene_card_fields(c, user_character_name),
            });
            pending.remove(i);
        }
    }

    tracing::debug!(
        context = CONTEXT,
        messages = messages.len(),
        candidates = candidates.len(),
        found = found.len(),
        "[AbsentMentions] Scanned chat for absent characters"
    );
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;
    use serde_json::json;

    fn card(id: &str, name: &str, aliases: &[&str]) -> OffSceneCharacterCard {
        OffSceneCharacterCard {
            id: id.into(),
            name: name.into(),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
            pronouns: None,
            identity: Some("{{char}} keeps {{user}}'s books.".into()),
            description: None,
        }
    }

    #[test]
    fn two_names_in_one_message_come_out_reversed() {
        let msgs = vec![
            json!({"type":"message","id":"m1","role":"USER","content":"Ada and Bea went out."}),
        ];
        let out = find_absent_mentions(
            &msgs,
            &[card("a", "Ada", &[]), card("b", "Bea", &[])],
            &Presence::default(),
            Some("Charlie"),
        );
        let ids: Vec<&str> = out.iter().map(|m| m.character_id.as_str()).collect();
        assert_eq!(ids, ["b", "a"]);
        assert_eq!(out[1].card.body, "Ada keeps Charlie's books.");
        assert_eq!(
            serde_json::to_string(&out[1]).unwrap(),
            r#"{"messageId":"m1","characterId":"a","text":"Ada","card":{"name":"Ada","aliases":[],"pronouns":null,"body":"Ada keeps Charlie's books."}}"#
        );
    }

    #[test]
    fn the_persona_is_present_only_where_the_operator_speaks() {
        let salon = presence_of(&[], Some("salon"), Some(" Charlie "));
        assert!(salon.is_in_room("x", "charlie"));
        let room = presence_of(&[], Some("autonomous"), Some("Charlie"));
        assert!(!room.is_in_room("x", "Charlie"));
        let seated = presence_of(
            &[
                json!({"characterId":"a","status":"active"}),
                json!({"characterId":"b","status":"removed"}),
            ],
            Some("salon"),
            None,
        );
        assert!(seated.is_in_room("a", "Ada"));
        assert!(!seated.is_in_room("b", "Bea"));
    }

    #[test]
    fn the_scan_always_logs_its_counts() {
        let (out, lines) =
            captured_with(|| find_absent_mentions(&[], &[], &Presence::default(), None));
        assert!(out.is_empty());
        let l = lines
            .iter()
            .find(|l| l.contains("[AbsentMentions] Scanned chat for absent characters"))
            .expect("the scan line");
        assert!(l.starts_with("DEBUG"), "{l}");
        for key in ["messages=0", "candidates=0", "found=0"] {
            assert!(l.contains(key), "{key} in {l}");
        }
    }
}
