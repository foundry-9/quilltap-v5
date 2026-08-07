//! The participant resolver (Phase-3 Unit-3 wave 3) — v4
//! `lib/services/chat-message/participant-resolver.service.ts`
//! (`resolveRespondingParticipant`, `loadAllParticipantData`,
//! `getRoleplayTemplate`) plus the connection-profile fallback chain of
//! `lib/chat/connection-resolver.ts` (`resolveConnectionProfile`, in the
//! [`connection`] child module).
//!
//! DB-reading, no model call. Picks which character participant will respond in a
//! chat, loads that character + its connection profile, and (for multi-character
//! chats) all participant characters; also resolves the chat's roleplay template
//! through a chat → project → user/global fallback chain (persisting an inherited
//! default onto the chat).
//!
//! ## The responding-participant algorithm ([`resolve_responding_participant`])
//!
//! Faithful to v4's branch structure (the traps preserved):
//!
//!   * **user participant** — resolved via
//!     [`crate::participant_filters::find_active_user_participant`] (honor the
//!     "Speaking As" selection), only for `userParticipant`/`userParticipantId`.
//!   * **requested participant present** — the requested id, when it names a
//!     present CHARACTER with a character id, wins. If it is missing/inactive:
//!       - **continue mode** → `throw` (a mismatched participant would save
//!         content under the wrong character; the chain loop handles the throw).
//!         Exact message: `Requested participant <id> not found or inactive in
//!         chat <chatId>`.
//!       - **normal mode** → warn + fall back to the first active character.
//!   * **no requested participant** — pick the next LLM responder by weighted
//!     talkativeness over the LLM candidate set (`controlledBy !== 'user'`):
//!       - **zero** candidates → fall back to the first active character (so a
//!         solo user-character chat surfaces a recognizable error, not a silent
//!         null);
//!       - **one** candidate → that one;
//!       - **multiple** → build the talkativeness map (one `findById` per
//!         candidate), read messages, compute the turn state
//!         ([`crate::turn_state::calculate_turn_state_from_history`]), and pick via
//!         [`crate::select_speaker::select_next_speaker`] (RNG injected as
//!         `random01`, mirroring [`crate::services::turn_orchestrator`]), with a
//!         defensive fall-back to the first candidate.
//!   * a `characterParticipant` with no character id → `throw "No active
//!     character in chat"`.
//!   * the character must resolve → else `throw "Character not found"`.
//!   * resolve the connection profile via the fallback chain (participant
//!     override → character default → chat default; else throw) — a resolution
//!     failure is re-thrown as `"No connection profile configured for
//!     character"`; a resolved id that no row matches → `"Connection profile not
//!     found"`.
//!
//! ## Injected impurities
//!
//! * **`Math.random()`** — the sole impurity inside `select_next_speaker`; the
//!   caller threads `random01` (only consumed on the multiple-candidate path).
//!
//! ## Host-side deferred seam
//!
//! * **The decrypted API key.** v4 fetches + decrypts it via
//!   `repos.connections.findApiKeyById(profile.apiKeyId)`. Per the established
//!   deferral (mirroring [`crate::services::cheap_llm_exec`]), API-key acquisition
//!   stays host-side: this resolver returns the resolved `connection_profile` row
//!   (carrying `apiKeyId` when set); fetching the key from the `api_keys` table is
//!   the host's job. The result's `api_key` slot is therefore `None` here.

use serde_json::Value;

use crate::chat_predicates::{is_participant_present, ParticipantStatus};
use crate::db::document_store_overlay::OverlayError;
use crate::db::runtime::Db;
use crate::db::{
    characters_read, chats_messages_read, connection_profiles, projects, roleplay_templates,
    DbError,
};
use crate::participant_filters::{
    find_active_user_participant, is_multi_character_chat, ParticipantView as FilterParticipant,
};
use crate::select_speaker::{select_next_speaker, SpeakerParticipant};
use crate::turn_state::{calculate_turn_state_from_history, MessageView};

use std::collections::HashMap;

/// Error variants that mirror v4's thrown `Error`s (message strings preserved).
#[derive(Debug)]
pub enum ResolveError {
    /// A DB read/write failed.
    Db(DbError),
    /// v4 `throw new Error("Requested participant ... not found or inactive ...")`.
    RequestedNotFound(String),
    /// v4 `throw new Error("No active character in chat")`.
    NoActiveCharacter,
    /// v4 `throw new Error("Character not found")`.
    CharacterNotFound,
    /// v4 `throw new Error("No connection profile configured for character")`.
    NoConnectionProfileConfigured,
    /// v4 `throw new Error("Connection profile not found")`.
    ConnectionProfileNotFound,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Db(e) => write!(f, "{e:?}"),
            ResolveError::RequestedNotFound(m) => write!(f, "{m}"),
            ResolveError::NoActiveCharacter => write!(f, "No active character in chat"),
            ResolveError::CharacterNotFound => write!(f, "Character not found"),
            ResolveError::NoConnectionProfileConfigured => {
                write!(f, "No connection profile configured for character")
            }
            ResolveError::ConnectionProfileNotFound => write!(f, "Connection profile not found"),
        }
    }
}

impl std::error::Error for ResolveError {}

impl From<DbError> for ResolveError {
    fn from(e: DbError) -> Self {
        ResolveError::Db(e)
    }
}

/// The v4 `ParticipantResolutionResult`, minus the decrypted API key (host-side
/// deferred). The three entity fields are the loaded JSON rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticipantResolution {
    /// The character participant who will respond (the chat participant object).
    pub character_participant: Value,
    /// The character data (vault-overlaid).
    pub character: Value,
    /// The resolved connection profile row.
    pub connection_profile: Value,
    /// The decrypted API key — **always `None` here** (host-side deferred seam).
    pub api_key: Option<String>,
    /// Image profile id from the chat level (`chat.imageProfileId || null`).
    pub image_profile_id: Option<String>,
    /// The user participant (user-controlled character), or `None`.
    pub user_participant: Option<Value>,
    /// The user participant id, or `None`.
    pub user_participant_id: Option<String>,
    /// Whether this is a multi-character chat.
    pub is_multi_character: bool,
}

// ---------------------------------------------------------------------------
// Shared participant / message marshaling (chat JSON → the pure view structs).
// ---------------------------------------------------------------------------

fn parse_status(s: Option<&str>) -> ParticipantStatus {
    match s.unwrap_or("active") {
        "active" => ParticipantStatus::Active,
        "silent" => ParticipantStatus::Silent,
        "removed" => ParticipantStatus::Removed,
        _ => ParticipantStatus::Absent,
    }
}

fn str_field<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}

/// A participant's non-empty character id (JS truthiness: empty string dropped).
fn nonempty_character_id(p: &Value) -> Option<String> {
    str_field(p, "characterId")
        .filter(|c| !c.is_empty())
        .map(String::from)
}

fn to_filter_participant(p: &Value) -> FilterParticipant {
    FilterParticipant {
        id: str_field(p, "id").unwrap_or_default().to_string(),
        status: parse_status(str_field(p, "status")),
        controlled_by: str_field(p, "controlledBy").unwrap_or("llm").to_string(),
        character_id: nonempty_character_id(p),
    }
}

fn to_speaker_participant(p: &Value) -> SpeakerParticipant {
    SpeakerParticipant {
        id: str_field(p, "id").unwrap_or_default().to_string(),
        participant_type: str_field(p, "type").unwrap_or_default().to_string(),
        status: parse_status(str_field(p, "status")),
        character_id: nonempty_character_id(p),
        controlled_by: str_field(p, "controlledBy").unwrap_or("llm").to_string(),
        talkativeness: p.get("talkativeness").and_then(Value::as_f64),
    }
}

/// v4's continue-/normal-mode candidate predicate: a present CHARACTER carrying a
/// character id.
fn is_active_character(p: &Value) -> bool {
    str_field(p, "type") == Some("CHARACTER")
        && is_participant_present(parse_status(str_field(p, "status")))
        && nonempty_character_id(p).is_some()
}

/// v4's LLM-candidate predicate: `is_active_character` AND NOT user-driven — a
/// user-controlled seat OR a seat the human is impersonating this session waits
/// for the human (v4 Bug 44: impersonation is an overlay, `controlledBy` stays
/// `'llm'`, so an impersonated seat must be excluded via the overlay, not the
/// column).
fn is_llm_candidate(p: &Value, impersonating_participant_ids: Option<&[String]>) -> bool {
    is_active_character(p)
        && !crate::participant_filters::is_user_driven_seat(
            str_field(p, "id").unwrap_or_default(),
            str_field(p, "controlledBy").unwrap_or("llm"),
            impersonating_participant_ids,
        )
}

/// The chat's `impersonatingParticipantIds` (v4 `chat.impersonatingParticipantIds`),
/// tolerating absence the `|| []` way.
fn impersonating_ids(chat: &Value) -> Vec<String> {
    chat.get("impersonatingParticipantIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn participants_of(chat: &Value) -> Vec<Value> {
    chat.get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn to_message_views(messages: &[Value]) -> Vec<MessageView> {
    messages
        .iter()
        .filter(|m| str_field(m, "type") == Some("message"))
        .map(|m| {
            // A Host turn-pass record is tagged so `calculateTurnStateFromHistory`
            // advances `lastSpeakerId` past it (v4 `isTurnPassMessage`, b90cd1f5).
            if let Some(pid) = crate::skip_signal::turn_pass_participant_id(m) {
                return MessageView {
                    msg_type: Some(crate::turn_state::TURN_PASS_VIEW_TYPE.to_string()),
                    role: str_field(m, "role").unwrap_or_default().to_string(),
                    participant_id: Some(pid.to_string()),
                    target_participant_ids: None,
                };
            }
            MessageView {
                msg_type: Some("message".to_string()),
                role: str_field(m, "role").unwrap_or_default().to_string(),
                participant_id: str_field(m, "participantId").map(String::from),
                target_participant_ids: m.get("targetParticipantIds").and_then(|t| {
                    t.as_array().map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                }),
            }
        })
        .collect()
}

/// Read a character (vault-overlaid, v4 `repos.characters.findById`) — main+mount.
fn read_character(db: &Db, id: &str) -> Result<Option<Value>, DbError> {
    let id = id.to_string();
    db.read_main(|main| db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &id)))
}

// ---------------------------------------------------------------------------
// The connection-profile resolver (connection-resolver.ts).
// ---------------------------------------------------------------------------

/// Port of `lib/chat/connection-resolver.ts`.
pub mod connection {
    use super::str_field;
    use serde_json::Value;

    /// Resolve which connection profile id to use for a CHARACTER participant
    /// (v4 `resolveConnectionProfile`). Order:
    ///   1. `participant.connectionProfileId` (per-chat override),
    ///   2. `character.defaultConnectionProfileId` (character default),
    ///   3. `chat_default_profile_id` (chat-level fallback, if provided),
    ///   4. else `None` (v4 `throw`s here — the caller maps `None` to the
    ///      re-thrown `"No connection profile configured for character"`).
    ///
    /// The JS truthiness guards mean an empty string does NOT satisfy a step (it
    /// falls through), matching v4's `if (participant.connectionProfileId)`.
    pub fn resolve_connection_profile(
        participant: &Value,
        character: &Value,
        chat_default_profile_id: Option<&str>,
    ) -> Option<String> {
        if let Some(id) = str_field(participant, "connectionProfileId").filter(|s| !s.is_empty()) {
            return Some(id.to_string());
        }
        if let Some(id) =
            str_field(character, "defaultConnectionProfileId").filter(|s| !s.is_empty())
        {
            return Some(id.to_string());
        }
        if let Some(id) = chat_default_profile_id.filter(|s| !s.is_empty()) {
            return Some(id.to_string());
        }
        None
    }
}

// ---------------------------------------------------------------------------
// resolveRespondingParticipant
// ---------------------------------------------------------------------------

/// Resolve the responding participant for a chat message (v4
/// `resolveRespondingParticipant`). `chat` is the already-loaded chat row
/// (`serde_json::Value`, from [`crate::db::chats_read::find_by_id`]); `random01`
/// injects `Math.random()` (consumed only on the multiple-candidate weighted
/// path). See the module docs for the branch structure + error strings.
#[allow(clippy::too_many_arguments)]
pub async fn resolve_responding_participant(
    db: &Db,
    chat: &Value,
    _user_id: &str,
    requested_responding_participant_id: Option<&str>,
    is_continue_mode: bool,
    active_user_participant_id: Option<&str>,
    random01: f64,
) -> Result<ParticipantResolution, ResolveError> {
    let chat_id = str_field(chat, "id").unwrap_or_default().to_string();
    let participants = participants_of(chat);
    let filter_participants: Vec<FilterParticipant> =
        participants.iter().map(to_filter_participant).collect();
    let impersonating = impersonating_ids(chat);

    // User participant (honoring the "Speaking As" selection + the impersonation
    // overlay, v4 Bug 44).
    let user_participant_view = find_active_user_participant(
        &filter_participants,
        active_user_participant_id,
        Some(&impersonating),
    );
    let user_participant_id = user_participant_view.map(|p| p.id.clone());
    // The full participant JSON object (v4 returns the participant, not the view).
    let user_participant = user_participant_id.as_ref().and_then(|uid| {
        participants
            .iter()
            .find(|p| str_field(p, "id") == Some(uid.as_str()))
            .cloned()
    });

    // Resolve the character participant.
    let mut character_participant: Option<Value> = None;

    if let Some(requested) = requested_responding_participant_id.filter(|s| !s.is_empty()) {
        // Continue mode with a specific participant requested — find them.
        character_participant = participants
            .iter()
            .find(|p| {
                str_field(p, "id") == Some(requested)
                    && str_field(p, "type") == Some("CHARACTER")
                    && is_participant_present(parse_status(str_field(p, "status")))
                    && nonempty_character_id(p).is_some()
            })
            .cloned();

        if character_participant.is_none() {
            if is_continue_mode {
                return Err(ResolveError::RequestedNotFound(format!(
                    "Requested participant {requested} not found or inactive in chat {chat_id}"
                )));
            }
            // Normal mode: fall back to the first active character.
            character_participant = participants
                .iter()
                .find(|p| is_active_character(p))
                .cloned();
        }
    } else {
        // Normal / continue-without-specific: weighted next LLM responder.
        let llm_candidates: Vec<&Value> = participants
            .iter()
            .filter(|p| is_llm_candidate(p, Some(&impersonating)))
            .collect();

        if llm_candidates.is_empty() {
            character_participant = participants
                .iter()
                .find(|p| is_active_character(p))
                .cloned();
        } else if llm_candidates.len() == 1 {
            character_participant = Some(llm_candidates[0].clone());
        } else {
            // Build the talkativeness map (one findById per candidate).
            let mut talkativeness: HashMap<String, f64> = HashMap::new();
            for p in &llm_candidates {
                if let Some(cid) = nonempty_character_id(p) {
                    if let Some(ch) = read_character(db, &cid)? {
                        if let Some(t) = ch.get("talkativeness").and_then(Value::as_f64) {
                            talkativeness.insert(cid, t);
                        }
                    }
                }
            }

            let messages = {
                let chat_id = chat_id.clone();
                db.read_main(move |conn| chats_messages_read::get_messages(conn, &chat_id))?
            };
            let message_views = to_message_views(&messages);

            let spoken_json = chat
                .get("spokenThisCycleParticipantIds")
                .and_then(Value::as_str);
            let turn_state = calculate_turn_state_from_history(&message_views, spoken_json);

            // v4 passes `llmCandidates` (the filtered set) to selectNextSpeaker.
            let speaker_participants: Vec<SpeakerParticipant> = llm_candidates
                .iter()
                .map(|p| to_speaker_participant(p))
                .collect();

            let selection = select_next_speaker(
                &speaker_participants,
                &talkativeness,
                &[], // v4 passes turnState.queue; the resolver's turn state has none
                &turn_state.spoken_since_user_turn,
                turn_state.last_speaker_id.as_deref(),
                random01,
                Some(&impersonating),
            );

            if let Some(next_id) = &selection.next_speaker_id {
                character_participant = participants
                    .iter()
                    .find(|p| str_field(p, "id") == Some(next_id.as_str()))
                    .cloned();
            }
            // Defensive fallback if selection somehow yielded nothing.
            if character_participant.is_none() {
                character_participant = Some(llm_candidates[0].clone());
            }
        }
    }

    // A character participant with a character id is required.
    let character_participant = match &character_participant {
        Some(p) if nonempty_character_id(p).is_some() => p.clone(),
        _ => return Err(ResolveError::NoActiveCharacter),
    };
    let character_id =
        nonempty_character_id(&character_participant).ok_or(ResolveError::NoActiveCharacter)?;

    // Load the character.
    let character = read_character(db, &character_id)?.ok_or(ResolveError::CharacterNotFound)?;

    // Resolve the connection profile via the fallback chain.
    let resolved_profile_id =
        connection::resolve_connection_profile(&character_participant, &character, None)
            .ok_or(ResolveError::NoConnectionProfileConfigured)?;

    // Load the connection profile (host-side deferred: no API-key fetch).
    let connection_profile = {
        let id = resolved_profile_id.clone();
        db.read_main(move |conn| connection_profiles::find_by_id(conn, &id))?
    }
    .ok_or(ResolveError::ConnectionProfileNotFound)?;

    let image_profile_id = str_field(chat, "imageProfileId")
        .filter(|s| !s.is_empty())
        .map(String::from);

    let is_multi_character = is_multi_character_chat(&filter_participants);

    Ok(ParticipantResolution {
        character_participant,
        character,
        connection_profile,
        api_key: None,
        image_profile_id,
        user_participant,
        user_participant_id,
        is_multi_character,
    })
}

// ---------------------------------------------------------------------------
// loadAllParticipantData
// ---------------------------------------------------------------------------

/// Load all participant characters for a multi-character chat (v4
/// `loadAllParticipantData`). The already-loaded `primary_character` is reused for
/// its own id (no re-read). Returns a map of characterId → character JSON, over
/// the present CHARACTER participants. A character that doesn't resolve is
/// dropped.
pub fn load_all_participant_data(
    db: &Db,
    chat: &Value,
    primary_character: &Value,
) -> Result<HashMap<String, Value>, DbError> {
    let primary_id = primary_character
        .get("id")
        .and_then(Value::as_str)
        .map(String::from);

    let mut out: HashMap<String, Value> = HashMap::new();
    for p in participants_of(chat) {
        if str_field(&p, "type") != Some("CHARACTER") {
            continue;
        }
        let Some(cid) = nonempty_character_id(&p) else {
            continue;
        };
        if !is_participant_present(parse_status(str_field(&p, "status"))) {
            continue;
        }
        if Some(&cid) == primary_id.as_ref() {
            out.insert(cid, primary_character.clone());
        } else if let Some(ch) = read_character(db, &cid)? {
            out.insert(cid, ch);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// getRoleplayTemplate
// ---------------------------------------------------------------------------

/// Resolve the roleplay template for a chat, with the chat → project →
/// user/global-default fallback chain (v4 `getRoleplayTemplate`). Returns the
/// resolved template's `systemPrompt`, or `None` when no template resolves.
///
/// When the chat has no `roleplayTemplateId` (older/imported), the first
/// available default is inherited — a project chat's `defaultRoleplayTemplateId`
/// wins over the user/global `chat_settings_default_roleplay_template_id` — and
/// **persisted onto the chat** so it sticks for future runs.
///
/// ### The persist write — column note
///
/// v4 does `repos.chats.update(chat.id, { roleplayTemplateId: inheritedTemplateId
/// })`, which writes the chat's own **`roleplayTemplateId`** column (the active
/// template, NOT the chat-level `defaultRoleplayTemplateId` default) and (per
/// v4's `chats.update`, which re-passes `existing.updatedAt` when the caller
/// omits it) **preserves `updatedAt`**. The persist goes through
/// `ChatUpdate.roleplay_template_id`, whose `update` reproduces both properties
/// (the differential stays zero-normalization).
pub async fn get_roleplay_template(
    db: &Db,
    chat: &Value,
    chat_settings_default_roleplay_template_id: Option<&str>,
) -> Result<Option<RoleplayTemplateResult>, ResolveError> {
    let chat_id = str_field(chat, "id").unwrap_or_default().to_string();

    // v4: `roleplayTemplateId === undefined || null`. In the net read shape a NULL
    // column is OMITTED, so an absent key == v4's undefined/null.
    let mut roleplay_template_id = str_field(chat, "roleplayTemplateId")
        .filter(|s| !s.is_empty())
        .map(String::from);

    if roleplay_template_id.is_none() {
        let mut inherited: Option<String> = None;

        // Project default (for project chats).
        if let Some(project_id) = str_field(chat, "projectId").filter(|s| !s.is_empty()) {
            let project_id = project_id.to_string();
            let project = db.read_main(|main| {
                db.read_mount_index(|mount| {
                    projects::ProjectsRepository::new(main, mount)
                        .find_by_id(&project_id)
                        .map_err(|e| match e {
                            OverlayError::Db(db_err) => db_err,
                            other => DbError::Key(format!("projects.findById: {other}")),
                        })
                })
            })?;
            if let Some(project) = project {
                if let Some(pid) = project
                    .get("defaultRoleplayTemplateId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    inherited = Some(pid.to_string());
                }
            }
        }

        // User/global default.
        if inherited.is_none() {
            inherited = chat_settings_default_roleplay_template_id
                .filter(|s| !s.is_empty())
                .map(String::from);
        }

        if let Some(inherited_id) = inherited {
            // Persist the inherited default onto the chat via the
            // `ChatUpdate.roleplay_template_id` setter (the chat's own
            // `roleplayTemplateId` column; `updatedAt` preserved by `update`'s
            // v4-faithful resolve — see the doc note above).
            let chat_id = chat_id.clone();
            let inherited_id_for_write = inherited_id.clone();
            db.write(move |writers| {
                writers.main().chats().update(
                    &chat_id,
                    &crate::db::chats::ChatUpdate {
                        roleplay_template_id: Some(Some(inherited_id_for_write)),
                        ..Default::default()
                    },
                )?;
                Ok::<(), DbError>(())
            })
            .await?;
            roleplay_template_id = Some(inherited_id);
        }
    }

    let Some(template_id) = roleplay_template_id else {
        return Ok(None);
    };

    let system_prompt = {
        let template_id = template_id.clone();
        db.read_main(move |conn| roleplay_templates::find_system_prompt_by_id(conn, &template_id))?
    };

    Ok(system_prompt.map(|system_prompt| RoleplayTemplateResult { system_prompt }))
}

/// The resolved roleplay template (v4 `{ systemPrompt }`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleplayTemplateResult {
    pub system_prompt: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn connection_resolver_fallback_order() {
        // Participant override wins.
        let p = json!({ "id": "p1", "connectionProfileId": "cp-part" });
        let c = json!({ "id": "c1", "defaultConnectionProfileId": "cp-char" });
        assert_eq!(
            connection::resolve_connection_profile(&p, &c, Some("cp-chat")),
            Some("cp-part".to_string())
        );

        // No participant override → character default.
        let p2 = json!({ "id": "p1" });
        assert_eq!(
            connection::resolve_connection_profile(&p2, &c, Some("cp-chat")),
            Some("cp-char".to_string())
        );

        // Neither → chat default.
        let c2 = json!({ "id": "c1" });
        assert_eq!(
            connection::resolve_connection_profile(&p2, &c2, Some("cp-chat")),
            Some("cp-chat".to_string())
        );

        // None at all → None (v4 throws).
        assert_eq!(connection::resolve_connection_profile(&p2, &c2, None), None);

        // Empty string does not satisfy a step (JS truthiness).
        let p3 = json!({ "id": "p1", "connectionProfileId": "" });
        assert_eq!(connection::resolve_connection_profile(&p3, &c2, None), None);
    }

    #[test]
    fn is_llm_candidate_excludes_user_controlled() {
        let user = json!({ "id": "u", "type": "CHARACTER", "status": "active", "characterId": "cu", "controlledBy": "user" });
        let llm = json!({ "id": "l", "type": "CHARACTER", "status": "active", "characterId": "cl", "controlledBy": "llm" });
        let absent = json!({ "id": "a", "type": "CHARACTER", "status": "removed", "characterId": "ca", "controlledBy": "llm" });
        assert!(!is_llm_candidate(&user, None));
        assert!(is_llm_candidate(&llm, None));
        assert!(!is_llm_candidate(&absent, None));
        // A default controlledBy (missing) is 'llm' → candidate.
        let dflt =
            json!({ "id": "d", "type": "CHARACTER", "status": "active", "characterId": "cd" });
        assert!(is_llm_candidate(&dflt, None));
        // v4 Bug 44: an impersonated LLM seat is excluded via the overlay, even
        // though its column stays 'llm'; other LLM seats are unaffected.
        let overlay = ["l".to_string()];
        assert!(!is_llm_candidate(&llm, Some(&overlay)));
        assert!(is_llm_candidate(&dflt, Some(&overlay)));
    }
}
