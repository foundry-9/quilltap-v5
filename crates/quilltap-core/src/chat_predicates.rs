//! Port of the pure chat-type / participant-status predicates from v4's
//! lib/schemas/chat.types.ts.

/// A participant's presence status in a chat scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticipantStatus {
    Active,
    Silent,
    Absent,
    Removed,
}

impl ParticipantStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ParticipantStatus::Active => "active",
            ParticipantStatus::Silent => "silent",
            ParticipantStatus::Absent => "absent",
            ParticipantStatus::Removed => "removed",
        }
    }
}

/// Help-like chat surfaces (`help` / `brahma`): lightweight titling, no
/// story-background, no autonomous machinery. (Governs titling/summary routing
/// only — moderation policy is separate; see [`is_moderation_exempt_chat_type`].)
pub fn is_help_like_chat_type(chat_type: Option<&str>) -> bool {
    matches!(chat_type, Some("help") | Some("brahma"))
}

/// Chat types exempt from dangerous-content moderation (`help` / `brahma`).
/// Deliberately a separate predicate from [`is_help_like_chat_type`] — the two
/// covering the same set today is a coincidence, not a contract.
pub fn is_moderation_exempt_chat_type(chat_type: Option<&str>) -> bool {
    matches!(chat_type, Some("help") | Some("brahma"))
}

/// Whether a participant is present in the scene (active or silent) — both
/// perceive and take turns.
pub fn is_participant_present(status: ParticipantStatus) -> bool {
    matches!(
        status,
        ParticipantStatus::Active | ParticipantStatus::Silent
    )
}

/// Whether a participant can receive whispers (must be present).
pub fn can_receive_whisper(status: ParticipantStatus) -> bool {
    matches!(
        status,
        ParticipantStatus::Active | ParticipantStatus::Silent
    )
}

/// Convert legacy `isActive`/`removedAt` to the status enum. Precedence:
/// active wins; else a *truthy* `removedAt` → removed; else absent. (v4 guards
/// `removedAt` on truthiness, so an empty string is falsy → absent.)
pub fn migrate_is_active_to_status(is_active: bool, removed_at: Option<&str>) -> ParticipantStatus {
    if is_active {
        ParticipantStatus::Active
    } else if removed_at.is_some_and(|s| !s.is_empty()) {
        ParticipantStatus::Removed
    } else {
        ParticipantStatus::Absent
    }
}
