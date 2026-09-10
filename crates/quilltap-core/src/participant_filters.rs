//! Port of the participant-list helpers from v4's lib/chat/turn-manager/
//! utils.ts — presence/control filters over a chat's participant list.
//!
//! `controlled_by` is the v4 `controlledBy` field ('user' | 'llm'); the Zod
//! schema defaults it to 'llm', so a parsed participant always carries it.

use crate::chat_predicates::{is_participant_present, ParticipantStatus};

/// A participant as these filters read it: id, presence status, who controls it,
/// and (for LLM filters) whether it carries a character id.
#[derive(Clone, Debug)]
pub struct ParticipantView {
    pub id: String,
    /// v4's `ChatParticipantBase.type` — `"CHARACTER"` for every seat the turn
    /// manager reads. Carried since P4.D172 so
    /// [`get_present_character_seats`] can spell v4's whole predicate here
    /// rather than have each caller re-derive it.
    pub participant_type: String,
    pub status: ParticipantStatus,
    pub controlled_by: String,
    pub character_id: Option<String>,
}

impl ParticipantView {
    fn is_present(&self) -> bool {
        is_participant_present(self.status)
    }
    fn is_user_controlled(&self) -> bool {
        self.controlled_by == "user"
    }
    fn is_user_driven(&self, impersonating_participant_ids: Option<&[String]>) -> bool {
        is_user_driven_seat(&self.id, &self.controlled_by, impersonating_participant_ids)
    }
    fn has_character(&self) -> bool {
        self.character_id.as_deref().is_some_and(|c| !c.is_empty())
    }
}

/// True when the human is driving this seat right now — either because they own
/// it (`controlledBy === 'user'`) or because they are impersonating it this
/// session (its id is in `impersonatingParticipantIds`).
///
/// Impersonation is an OVERLAY on top of ownership (v4 Bug 44): the seat's
/// durable `controlledBy` column is left untouched, so any reader that means
/// "who is typing right now / who does NOT get an LLM turn / whose voice a typed
/// message carries" must consult this helper rather than the bare column.
/// Readers that mean "who OWNS this seat" (the stable user seat,
/// [`is_all_llm_chat`], the identity stacks) deliberately keep reading
/// `controlledBy` — that stability is what keeps the seat from moving
/// mid-conversation.
///
/// The v4 helper takes a participant object and reads `id` + `controlledBy`;
/// here the two fields are passed directly so every participant representation
/// (`ParticipantView`, `SpeakerParticipant`, raw JSON) can consult it.
pub fn is_user_driven_seat(
    participant_id: &str,
    controlled_by: &str,
    impersonating_participant_ids: Option<&[String]>,
) -> bool {
    if controlled_by == "user" {
        return true;
    }
    matches!(impersonating_participant_ids, Some(ids) if ids.iter().any(|id| id == participant_id))
}

/// The first present, user-controlled participant, or `None`.
///
/// This is an OWNERSHIP reader (v4 Bug 44) — it intentionally reads
/// `controlledBy` only and ignores the impersonation overlay, because the "user
/// seat" must stay put while an impersonation comes and goes. Callers that need
/// the seat the human is currently speaking *as* want
/// [`find_active_user_participant`] instead.
/// (Deprecated in v4 in favour of [`find_active_user_participant`], but kept as
/// the fallback resolver.)
pub fn find_user_participant(participants: &[ParticipantView]) -> Option<&ParticipantView> {
    participants
        .iter()
        .find(|p| p.is_present() && p.is_user_controlled())
}

/// The user-controlled participant the human is currently speaking as: prefer
/// the (present) `active_typing_participant_id` selection, else fall back to
/// [`find_user_participant`].
///
/// Honours the impersonation overlay (v4 Bug 44): when the selected
/// `active_typing_participant_id` is a seat the human is currently impersonating,
/// it is returned even though its durable `controlledBy` is still `'llm'`. The
/// fallback stays on [`find_user_participant`] (a genuine owner seat) — during a
/// solo impersonation there is no owner seat, but `addImpersonation` always sets
/// `activeTypingParticipantId`, so the selected branch resolves.
pub fn find_active_user_participant<'a>(
    participants: &'a [ParticipantView],
    active_typing_participant_id: Option<&str>,
    impersonating_participant_ids: Option<&[String]>,
) -> Option<&'a ParticipantView> {
    // v4 guards on truthiness, so an empty id falls through to the fallback.
    if let Some(id) = active_typing_participant_id.filter(|s| !s.is_empty()) {
        if let Some(selected) = participants.iter().find(|p| {
            p.id == id && p.is_present() && p.is_user_driven(impersonating_participant_ids)
        }) {
            return Some(selected);
        }
    }
    find_user_participant(participants)
}

/// All present, user-controlled participants.
pub fn find_user_controlled_participants(
    participants: &[ParticipantView],
) -> Vec<&ParticipantView> {
    participants
        .iter()
        .filter(|p| p.is_present() && p.is_user_controlled())
        .collect()
}

/// All present, LLM-controlled participants carrying a character id.
///
/// An OWNERSHIP reader (v4 Bug 44): it reads the bare `controlledBy` column and
/// ignores the impersonation overlay on purpose — an impersonated seat's durable
/// ownership is still `'llm'`, so it stays in this set. (The who-responds gate
/// that must EXCLUDE an impersonated seat is `resolve_responding_participant`'s
/// `is_llm_candidate`, which consults the overlay.)
pub fn get_active_llm_participants(participants: &[ParticipantView]) -> Vec<&ParticipantView> {
    participants
        .iter()
        .filter(|p| p.is_present() && p.has_character() && p.controlled_by == "llm")
        .collect()
}

/// The scalar predicate behind "is this a character seat present in the room?"
/// — v4 `utils.ts:153-159`'s `getPresentCharacterSeats` filter, spelled once.
///
/// `type === 'CHARACTER' && isParticipantPresent(status) && !!characterId`.
/// Present means `active` OR `silent`; `absent` and `removed` are not present.
///
/// Before P4.D172 v5 spelled this THREE ways — `SpeakerParticipant::
/// is_active_character`, `services/orchestrator.rs`'s own `is_active_character`
/// over a raw `Value`, and an inline walk in `message_finalizer.rs` that dropped
/// only `removed` and therefore KEPT `absent` seats v4 excludes. The third was a
/// live divergence; every caller now reaches this predicate.
pub fn is_present_character_seat(
    participant_type: &str,
    status: ParticipantStatus,
    character_id: Option<&str>,
) -> bool {
    participant_type == "CHARACTER"
        && is_participant_present(status)
        && character_id.is_some_and(|c| !c.is_empty())
}

/// The character seats present in the room this turn (v4
/// `getPresentCharacterSeats`). The one predicate for "who is in the scene" —
/// turn skipping, the cycle wrap, the cycle rotation, responder resolution and
/// the multi-seat guard all read it.
///
/// NOT to be confused with [`get_active_character_participants`], which despite
/// its name returns LLM-controlled seats only (v4 bug 131).
pub fn get_present_character_seats(participants: &[ParticipantView]) -> Vec<&ParticipantView> {
    participants
        .iter()
        .filter(|p| {
            is_present_character_seat(&p.participant_type, p.status, p.character_id.as_deref())
        })
        .collect()
}

/// Deprecated alias of [`get_active_llm_participants`] (v4 kept the old name).
///
/// ⚠ Despite the name this is LLM-ONLY. v4 bug 131: four selection sites built
/// their talkativeness map from it, so a seat the human drives was invisible —
/// its talkativeness fell through to the 0.5 default and an archived character
/// on it was never dropped. The whole-room reader is
/// [`get_present_character_seats`] / [`crate::room_characters::load_room_characters`].
/// v4 still calls the alias at `autonomous-room-announce.ts:129`, which is why
/// it stays.
pub fn get_active_character_participants(
    participants: &[ParticipantView],
) -> Vec<&ParticipantView> {
    get_active_llm_participants(participants)
}

/// Whether the chat needs multi-character controls: ≥2 user-controlled
/// participants, or ≥1 LLM-controlled participant.
pub fn is_multi_character_chat(participants: &[ParticipantView]) -> bool {
    find_user_controlled_participants(participants).len() >= 2
        || !get_active_llm_participants(participants).is_empty()
}

/// Whether every participant is non-user-controlled (no user-controlled
/// present) — the gate for all-LLM pause logic.
///
/// An OWNERSHIP reader (v4 Bug 44): it deliberately keeps reading `controlledBy`
/// (via [`find_user_controlled_participants`]) and ignores the impersonation
/// overlay — a chat whose only "user-driven" seat is an impersonated LLM
/// character is still structurally all-LLM, which is what returns the card's
/// Stop-Impersonate button in solo-shaped casts.
pub fn is_all_llm_chat(participants: &[ParticipantView]) -> bool {
    find_user_controlled_participants(participants).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(id: &str, controlled_by: &str) -> ParticipantView {
        ParticipantView {
            id: id.to_string(),
            participant_type: "CHARACTER".to_string(),
            status: ParticipantStatus::Active,
            controlled_by: controlled_by.to_string(),
            character_id: Some(format!("char-{id}")),
        }
    }

    // v4 `getPresentCharacterSeats`: present is active OR silent, and a
    // user-driven seat is IN (bug 131's invariant).
    #[test]
    fn present_character_seats_is_v4s_predicate() {
        let mut parts = vec![part("p1", "llm"), part("p2", "user")];
        parts.push(ParticipantView {
            status: ParticipantStatus::Silent,
            ..part("p3", "llm")
        });
        parts.push(ParticipantView {
            status: ParticipantStatus::Absent,
            ..part("p4", "llm")
        });
        parts.push(ParticipantView {
            status: ParticipantStatus::Removed,
            ..part("p5", "llm")
        });
        parts.push(ParticipantView {
            character_id: None,
            ..part("p6", "llm")
        });
        parts.push(ParticipantView {
            participant_type: "USER".to_string(),
            ..part("p7", "llm")
        });
        let seats: Vec<&str> = get_present_character_seats(&parts)
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(seats, vec!["p1", "p2", "p3"]);
    }

    // The alias is LLM-only and the whole-room reader is not — the bug-131
    // difference, pinned so a future consolidation cannot quietly merge them.
    #[test]
    fn the_alias_is_narrower_than_the_room() {
        let parts = vec![part("p1", "llm"), part("p2", "user")];
        assert_eq!(get_active_character_participants(&parts).len(), 1);
        assert_eq!(get_present_character_seats(&parts).len(), 2);
    }

    #[test]
    fn is_user_driven_seat_column_or_overlay() {
        // Owner seat: user column → driven regardless of overlay.
        assert!(is_user_driven_seat("p1", "user", None));
        assert!(is_user_driven_seat(
            "p1",
            "user",
            Some(&["other".to_string()])
        ));
        // LLM seat, no overlay → not driven.
        assert!(!is_user_driven_seat("p1", "llm", None));
        assert!(!is_user_driven_seat(
            "p1",
            "llm",
            Some(&["other".to_string()])
        ));
        // LLM seat listed in the overlay → driven (Bug 44), column untouched.
        assert!(is_user_driven_seat("p1", "llm", Some(&["p1".to_string()])));
    }

    // v4 turn-manager.test.ts (Bug 44): findActiveUserParticipant honours an
    // impersonated LLM seat via the overlay, without the column ever moving.
    #[test]
    fn find_active_user_honours_overlay_without_moving_column() {
        let jackie = part("p-jackie", "user");
        let abigail_seat = part("p-abigail", "llm");
        let parts = vec![jackie.clone(), abigail_seat.clone()];

        // Without the overlay, selecting the LLM character is not honoured — it
        // falls back to the first genuine owner seat.
        assert_eq!(
            find_active_user_participant(&parts, Some("p-abigail"), None).map(|p| p.id.as_str()),
            Some("p-jackie"),
        );
        // With the seat listed in impersonatingParticipantIds, the SAME still-LLM
        // seat IS honoured as the active speaker.
        let overlay = ["p-abigail".to_string()];
        assert_eq!(
            find_active_user_participant(&parts, Some("p-abigail"), Some(&overlay))
                .map(|p| p.id.as_str()),
            Some("p-abigail"),
        );
        // The column was never touched.
        assert_eq!(abigail_seat.controlled_by, "llm");
    }

    // A solo impersonation has no owner seat, but the selected branch resolves
    // because addImpersonation always sets activeTypingParticipantId.
    #[test]
    fn find_active_user_solo_impersonation_resolves() {
        let abigail_seat = part("p-abigail", "llm");
        let parts = vec![abigail_seat];
        let overlay = ["p-abigail".to_string()];
        assert_eq!(
            find_active_user_participant(&parts, Some("p-abigail"), Some(&overlay))
                .map(|p| p.id.as_str()),
            Some("p-abigail"),
        );
        // No fallback owner seat exists, so without the selection it is None.
        assert!(find_active_user_participant(&parts, None, Some(&overlay)).is_none());
    }

    // An impersonated LLM seat is still an OWNERSHIP-LLM seat (Bug 44 keep-list):
    // it stays in the all-LLM/active-LLM sets, which is what returns the card's
    // Stop button in solo-shaped casts.
    #[test]
    fn ownership_readers_ignore_the_overlay() {
        let abigail_seat = part("p-abigail", "llm");
        let parts = vec![abigail_seat];
        assert!(is_all_llm_chat(&parts));
        assert_eq!(get_active_llm_participants(&parts).len(), 1);
    }
}
