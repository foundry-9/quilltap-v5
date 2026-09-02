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

/// Parse a participant's stored `status` string into [`ParticipantStatus`].
///
/// The shared boundary step for the sites that read participants as raw JSON
/// rather than through a typed view. v4 has no counterpart: its participants
/// arrive already Zod-parsed (`ParticipantStatusEnum.default('active')` in
/// `ChatParticipantBase`), so `p.status` is a `ParticipantStatus` by the time
/// any predicate sees it. This function is that Zod step:
///
/// - absent/`None` → `Active`, matching the `.default('active')`;
/// - an unrecognised value → `Absent`, i.e. NOT present — v4's enum would
///   refuse the row outright, and "not in the scene" is the safe reading of a
///   status nobody can name.
///
/// **P4.68 consolidated every private copy of this exact rule onto this one.**
/// The ten swept sites — `enclave::announce`, `services::{commonplace_notifications,
/// fold_episode_pass, message_finalizer, participant_resolver, turn_orchestrator,
/// user_identity_resolver}`, and at string level `tools::{self_inventory,
/// whisper}` plus `tools::doc_edit::shared` — each carried a match
/// byte-identical to this one, so the sweep was behaviour-neutral by
/// measurement.
///
/// **Sites deliberately NOT consolidated** (each parses status, but to a
/// DIFFERENT rule that its own v4 twin fixes — a differing `_ =>` arm is a
/// measurement, not a typo):
///
/// - `services::build_context::parse_sys_status` — returns
///   `Option<system_prompt::ParticipantStatus>` (a different enum) and maps an
///   unknown value to `None`. Its v4 twin `buildOtherParticipantsInfo`
///   (`lib/chat/context/system-prompt-builder.ts:455`) never parses at all: it
///   skips only `status === 'removed'` and passes the raw string through as
///   `participant.status as ParticipantStatus`.
/// - `services::build_context::parse_attr_status` and
///   `services::answer_confirmation` (the inline match feeding
///   `AttributionParticipant`) — both map an unknown value to `Active`. Their
///   only status consumer is `find_user_participant_name`, whose v4 twin
///   (`lib/chat/context/message-attribution.ts:274`) gates on
///   `isParticipantPresent(p.status)` — v4's `chat.types.ts:557`
///   `status === 'active' || status === 'silent'`, i.e. unknown → NOT present,
///   which is what THIS function's `Absent` arm produces. See the P4.68 lane
///   record: `parse_attr_status` was consolidated on that measurement;
///   `answer_confirmation`'s twin copy is outside P4.68's ownership and is
///   RECORDED for its owning lane.
/// - `skip_signal::participant_is_present` — deliberately does NOT default a
///   missing `status` (an absent field returns `false` outright); its own doc
///   comment carries the reason.
/// - `db::chats_messages`, `turn_order`, `api::salon`,
///   `services::{chat_participants, regenerate_swipe, orchestrator}`,
///   `services::announcer::character_voiced`, `db::chats_participants` — outside
///   P4.68's ownership; measured and listed in the lane record.
///
/// `services::host_notifications` and `services::chat_participants`'s status→prose
/// maps are LABEL tables, not parsers.
pub fn participant_status_from_str(s: Option<&str>) -> ParticipantStatus {
    match s.unwrap_or("active") {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "removed" => ParticipantStatus::Removed,
        _ => ParticipantStatus::Absent,
    }
}

/// v4 `[70505745a]`'s story-background participant gate applied to a raw
/// participant object: `isParticipantPresent(p.status)`.
///
/// > Absent and (soft-)removed participants must never be painted into the
/// > background — the crafter is told to place every enumerated character as a
/// > figure in the frame, so a stale enumeration puts someone in the room who
/// > walked out of it. 'silent' counts as present: they are standing there,
/// > just not speaking.
pub fn json_participant_is_present(participant: &serde_json::Value) -> bool {
    is_participant_present(participant_status_from_str(
        participant
            .get("status")
            .and_then(serde_json::Value::as_str),
    ))
}

#[cfg(test)]
mod status_from_str_tests {
    use super::*;

    #[test]
    fn parses_the_four_statuses_and_defaults_like_zod() {
        assert_eq!(
            participant_status_from_str(Some("active")),
            ParticipantStatus::Active
        );
        assert_eq!(
            participant_status_from_str(Some("silent")),
            ParticipantStatus::Silent
        );
        assert_eq!(
            participant_status_from_str(Some("absent")),
            ParticipantStatus::Absent
        );
        assert_eq!(
            participant_status_from_str(Some("removed")),
            ParticipantStatus::Removed
        );
        // `.default('active')`.
        assert_eq!(participant_status_from_str(None), ParticipantStatus::Active);
        // Unrecognised → not present.
        assert_eq!(
            participant_status_from_str(Some("wallpaper")),
            ParticipantStatus::Absent
        );
    }

    #[test]
    fn json_gate_admits_active_and_silent_only() {
        let p = |s: &str| serde_json::json!({ "status": s });
        assert!(json_participant_is_present(&p("active")));
        assert!(json_participant_is_present(&p("silent")));
        assert!(!json_participant_is_present(&p("absent")));
        assert!(!json_participant_is_present(&p("removed")));
        // No `status` key at all → the Zod default → present.
        assert!(json_participant_is_present(&serde_json::json!({})));
    }
}
