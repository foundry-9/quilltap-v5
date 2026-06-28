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
    fn has_character(&self) -> bool {
        self.character_id.as_deref().is_some_and(|c| !c.is_empty())
    }
}

/// The first present, user-controlled participant, or `None`.
/// (Deprecated in v4 in favour of [`find_active_user_participant`], but kept as
/// the fallback resolver.)
pub fn find_user_participant(participants: &[ParticipantView]) -> Option<&ParticipantView> {
    participants
        .iter()
        .find(|p| p.is_present() && p.is_user_controlled())
}

/// The user-controlled participant the human is currently speaking as: prefer
/// the (present, user-controlled) `active_typing_participant_id` selection, else
/// fall back to [`find_user_participant`].
pub fn find_active_user_participant<'a>(
    participants: &'a [ParticipantView],
    active_typing_participant_id: Option<&str>,
) -> Option<&'a ParticipantView> {
    // v4 guards on truthiness, so an empty id falls through to the fallback.
    if let Some(id) = active_typing_participant_id.filter(|s| !s.is_empty()) {
        if let Some(selected) = participants
            .iter()
            .find(|p| p.id == id && p.is_present() && p.is_user_controlled())
        {
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
pub fn get_active_llm_participants(participants: &[ParticipantView]) -> Vec<&ParticipantView> {
    participants
        .iter()
        .filter(|p| p.is_present() && p.has_character() && p.controlled_by == "llm")
        .collect()
}

/// Deprecated alias of [`get_active_llm_participants`] (v4 kept the old name).
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
pub fn is_all_llm_chat(participants: &[ParticipantView]) -> bool {
    find_user_controlled_participants(participants).is_empty()
}
