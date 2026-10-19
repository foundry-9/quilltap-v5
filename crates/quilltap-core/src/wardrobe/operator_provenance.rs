//! Operator provenance — who the operator was when they changed an outfit
//! (v4 `lib/wardrobe/history/operator-provenance.ts`, `4ae20977f`).
//!
//! An operator change (the Wardrobe dialog's Done or slot buttons, Wear again)
//! is recorded against the character the user was speaking as at that moment:
//! the "Speaking As" seat (`activeTypingParticipantId`), impersonation
//! included, falling back to the first user-owned seat — the same resolver
//! that attributes a typed message
//! ([`crate::services::user_identity_resolver::speaking_as_seat_index`], the
//! ONE composition of v4's `findActiveUserParticipant`). A chat where the user
//! drives no character records the operator alone.

use serde_json::Value;

use crate::db::chat_wardrobe_history::{WardrobeHistoryProvenance, WardrobeHistorySource};
use crate::services::user_identity_resolver::speaking_as_seat_index;

/// `{ source: 'operator' }`, naming the user's live character when there is
/// one (v4 `operatorWardrobeProvenance(chat)`). Reads the chat's OWN
/// `activeTypingParticipantId` (never a request field) and its impersonation
/// overlay.
///
/// v4 `seat?.characterId ?? undefined` then a TRUTHY check: a seat whose
/// `characterId` is `''` records the bare operator, yet the DEBUG logs that
/// `''` verbatim (`'' ?? null` is `''`) — `null` only when there is no seat or
/// the seat carries no `characterId`.
pub fn operator_wardrobe_provenance(chat: &Value) -> WardrobeHistoryProvenance {
    let active_typing = chat
        .get("activeTypingParticipantId")
        .and_then(Value::as_str);
    let seat = speaking_as_seat_index(chat, active_typing).and_then(|i| {
        chat.get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| ps.get(i))
    });
    let actor_character_id = seat
        .and_then(|p| p.get("characterId"))
        .and_then(Value::as_str);
    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    tracing::debug!(
        target: "quilltap::wardrobe_history",
        context = "wardrobe-history",
        chatId = chat_id,
        actorCharacterId = actor_character_id.unwrap_or("null"),
        "Resolved operator for an outfit change"
    );
    let provenance = WardrobeHistoryProvenance::new(WardrobeHistorySource::Operator);
    match actor_character_id.filter(|id| !id.is_empty()) {
        Some(id) => provenance.with_actor(id),
        None => provenance,
    }
}

#[cfg(test)]
mod tests {
    //! v4 `operator-provenance` as `outfit.test.ts` exercises it ("records the
    //! operator as the character the user is speaking as") plus the four seat
    //! shapes the order names; the DEBUG pinned on the test thread.
    use super::*;
    use crate::test_support::captured_with;
    use serde_json::json;

    fn seat(id: &str, controlled_by: &str, character_id: Value) -> Value {
        json!({ "id": id, "type": "CHARACTER", "status": "active",
                "controlledBy": controlled_by, "characterId": character_id })
    }

    fn debug_line(lines: &[String]) -> String {
        let hits: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Resolved operator for an outfit change"))
            .collect();
        assert_eq!(hits.len(), 1, "{lines:#?}");
        hits[0].clone()
    }

    #[test]
    fn names_the_active_typing_user_seat() {
        let chat = json!({ "id": "chat-1", "activeTypingParticipantId": "p-user-2",
            "participants": [seat("p-user-1", "user", json!("char-u1")),
                             seat("p-user-2", "user", json!("char-u2")),
                             seat("p-llm", "llm", json!("char-l"))] });
        let (p, lines) = captured_with(|| operator_wardrobe_provenance(&chat));
        assert_eq!(
            p,
            WardrobeHistoryProvenance::new(WardrobeHistorySource::Operator).with_actor("char-u2")
        );
        assert!(debug_line(&lines).contains(
            "Resolved operator for an outfit change context=wardrobe-history chatId=chat-1 actorCharacterId=char-u2"
        ));
    }

    #[test]
    fn names_an_impersonated_llm_seat() {
        let chat = json!({ "id": "chat-2", "activeTypingParticipantId": "p-llm",
            "impersonatingParticipantIds": ["p-llm"],
            "participants": [seat("p-llm", "llm", json!("char-l"))] });
        let p = operator_wardrobe_provenance(&chat);
        assert_eq!(p.actor_character_id.as_deref(), Some("char-l"));
    }

    #[test]
    fn falls_back_to_the_first_user_seat_and_is_bare_without_one() {
        let chat = json!({ "id": "chat-3",
            "participants": [seat("p-llm", "llm", json!("char-l")),
                             seat("p-user", "user", json!("char-u"))] });
        assert_eq!(
            operator_wardrobe_provenance(&chat)
                .actor_character_id
                .as_deref(),
            Some("char-u")
        );
        let none =
            json!({ "id": "chat-4", "participants": [seat("p-llm", "llm", json!("char-l"))] });
        let (p, lines) = captured_with(|| operator_wardrobe_provenance(&none));
        assert_eq!(
            p,
            WardrobeHistoryProvenance::new(WardrobeHistorySource::Operator)
        );
        assert!(
            debug_line(&lines).ends_with("chatId=chat-4 actorCharacterId=null"),
            "{lines:#?}"
        );
        let bare = json!({ "id": "chat-5" });
        assert_eq!(
            operator_wardrobe_provenance(&bare),
            WardrobeHistoryProvenance::new(WardrobeHistorySource::Operator)
        );
    }

    #[test]
    fn an_empty_character_id_is_bare_but_logged_verbatim() {
        let chat = json!({ "id": "chat-6", "participants": [seat("p-user", "user", json!(""))] });
        let (p, lines) = captured_with(|| operator_wardrobe_provenance(&chat));
        assert_eq!(
            p,
            WardrobeHistoryProvenance::new(WardrobeHistorySource::Operator)
        );
        let line = debug_line(&lines);
        assert!(line.ends_with("chatId=chat-6 actorCharacterId="), "{line}");
    }
}
