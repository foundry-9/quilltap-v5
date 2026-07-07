//! Writer for Salon participation announcements — the Host (v4
//! `lib/services/host-notifications/writer.ts`).
//!
//! When a character joins, leaves, or changes participation status in a chat —
//! and, more broadly, whenever the Host narrates scene-setting the per-turn
//! system prompt used to carry (scenario, user-character introduction, the
//! multi-character roster, silent-mode entry/exit, join scenarios, the
//! timestamp, off-scene-character introductions, continuation/merge bubbles) —
//! this helper injects a synthetic `ASSISTANT`-role chat message authored by the
//! Host (`systemSender: "host"`) so both the user and the other characters see
//! it as part of normal conversation history.
//!
//! Every persisted message is built as a `serde_json` object literal matching
//! v4's `MessageEvent` shape exactly, validated into a
//! [`ChatEventInput`](crate::db::chats_messages::ChatEventInput), so the stored
//! row is byte-for-byte the `db::chats_messages` insert marshaling. Errors never
//! propagate — a failed post is surfaced as `None` (matching v4's try/catch →
//! null): participant operations must never fail because an announcement could
//! not be written.
//!
//! # Callers
//! - `add` / `remove` / `status-change` / `silent-mode` / `join-scenario`: the
//!   Phase-4 participant-lifecycle routes (participant add/remove/status ops).
//! - `scenario` / `user-character` / `multi-character-roster` / `timestamp` /
//!   `off-scene-characters`: the chat-orchestration `buildContext` seams (this
//!   unit closes the POST half of W4.6a's `BuildContextSeams`).
//! - `continuation-from` / `continuation-to` / `merge-from` / `merge-to`: the
//!   Salon change-of-venue / Merge-In routes (Phase-4).
//! - `no-user-character`: the auto-memory pipeline.
//!
//! # Reused W4.6a builders
//! The off-scene content builders + the introduced-id scan live in
//! [`crate::services::off_scene`] and are re-exported here; the multi-character
//! roster section lives in [`crate::message_formatter`]. This module does NOT
//! re-transcribe them.
//!
//! # Faithful quirk — the `no-user-character` idempotence guard
//! v4's `postHostNoUserCharacterAnnouncement` scans `chat.messages` for a prior
//! whisper, but `repos.chats.findById` returns metadata **only** (messages live
//! in the separate `chat_messages` table), so on the SQLite backend the scanned
//! array is always empty and the whisper posts unconditionally after the
//! existence check. The port reproduces that observed behavior (matches the
//! oracle); it does NOT query `chat_messages` for the guard.

use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::database_store::read_database_document;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read};
use crate::jsstr::{js_trim, js_trim_end};
use crate::message_formatter::{build_multi_character_context_section, OtherParticipant};

// Reuse — do not re-transcribe (W4.6a).
pub use crate::services::off_scene::{
    apply_host_templates, build_off_scene_characters_content,
    build_off_scene_characters_opaque_content, find_introduced_off_scene_character_ids,
    OffSceneCharacterCard,
};

// ===========================================================================
// Status phrasing (v4 `STATUS_PHRASE`)
// ===========================================================================

/// v4's `STATUS_PHRASE[status] ?? status`: the narrated phrase for a
/// participation status, falling back to the raw status string when unknown.
fn status_phrase(status: &str) -> &str {
    match status {
        "active" => "present and speaking freely",
        "silent" => "present but holding their tongue — observing, not speaking aloud",
        "absent" => "stepped away from the scene for the moment",
        "removed" => "departed the Salon",
        other => other,
    }
}

// ===========================================================================
// A joining character (the subset v4's `buildAddContent` + `readVaultIdentity`
// consume from a full `Character`).
// ===========================================================================

/// The character fields the `add` announcement needs. `identity.md` is read
/// from `character_document_mount_point_id`; `description` is the DB fallback.
#[derive(Clone, Debug)]
pub struct HostCharacter {
    pub name: String,
    pub description: Option<String>,
    pub character_document_mount_point_id: Option<String>,
}

// ===========================================================================
// Content / opaqueContent builders (byte-exact transcriptions).
// ===========================================================================

/// v4 `buildAddContent` — the persona-voiced welcome. `identity` is the already
/// read + trimmed vault `identity.md` (`Some` only when non-empty), preferred
/// over `description`; never both. Pure — the DB read happens in
/// [`post_host_add_announcement`].
pub fn build_add_content(
    name: &str,
    identity: Option<&str>,
    description: Option<&str>,
    user_character_name: Option<&str>,
) -> String {
    build_add_like(
        &format!("The Host welcomes {name} to the Salon."),
        name,
        identity,
        description,
        user_character_name,
    )
}

/// v4 `buildAddOpaqueContent` — persona-free variant for opaque-anywhere chats.
pub fn build_add_opaque_content(
    name: &str,
    identity: Option<&str>,
    description: Option<&str>,
    user_character_name: Option<&str>,
) -> String {
    build_add_like(
        &format!("{name} has joined the scene."),
        name,
        identity,
        description,
        user_character_name,
    )
}

/// Shared body for the two `add` builders — they differ only in the opening line.
fn build_add_like(
    first_line: &str,
    name: &str,
    identity: Option<&str>,
    description: Option<&str>,
    user_character_name: Option<&str>,
) -> String {
    let mut lines: Vec<String> = vec![first_line.to_string(), String::new()];
    if let Some(identity) = identity {
        // `identity` is already trimmed + non-empty (see `read_vault_identity`).
        lines.push("**Identity:**".to_string());
        lines.push(String::new());
        lines.push(apply_host_templates(
            identity,
            Some(name),
            user_character_name,
        ));
    } else {
        let description = description.map(js_trim).unwrap_or("");
        if !description.is_empty() {
            lines.push("**Description:**".to_string());
            lines.push(String::new());
            lines.push(apply_host_templates(
                description,
                Some(name),
                user_character_name,
            ));
        }
    }
    js_trim_end(&lines.join("\n")).to_string()
}

/// v4 `buildRemoveContent`.
pub fn build_remove_content(character_name: &str) -> String {
    format!("The Host bids {character_name} adieu — they have departed the Salon.")
}

/// v4 `buildRemoveOpaqueContent`.
pub fn build_remove_opaque_content(character_name: &str) -> String {
    format!("{character_name} has left the scene.")
}

/// v4 `buildStatusChangeContent`.
pub fn build_status_change_content(
    character_name: &str,
    old_status: &str,
    new_status: &str,
) -> String {
    let before = status_phrase(old_status);
    let after = status_phrase(new_status);
    format!("The Host notes that {character_name} is now {after} (previously {before}).")
}

/// v4 `buildStatusChangeOpaqueContent`.
pub fn build_status_change_opaque_content(
    character_name: &str,
    old_status: &str,
    new_status: &str,
) -> String {
    let before = status_phrase(old_status);
    let after = status_phrase(new_status);
    format!("{character_name} is now {after} (previously {before}).")
}

/// v4 `buildScenarioContent`.
pub fn build_scenario_content(scenario_text: &str) -> String {
    format!(
        "The Host sets the scene for the proceedings:\n\n{}",
        js_trim(scenario_text)
    )
}

/// v4 `buildScenarioOpaqueContent`.
pub fn build_scenario_opaque_content(scenario_text: &str) -> String {
    format!("Scene:\n\n{}", js_trim(scenario_text))
}

/// v4 `buildUserCharacterContent`.
pub fn build_user_character_content(
    user_character_name: &str,
    user_character_description: Option<&str>,
) -> String {
    let desc = user_character_description.map(js_trim).unwrap_or("");
    if desc.is_empty() {
        return format!(
            "The Host introduces {user_character_name} to the assembled company — they will be the user's voice in this conversation."
        );
    }
    format!(
        "The Host introduces {user_character_name}, who will be the user's voice in this conversation:\n\n{}",
        apply_host_templates(desc, None, Some(user_character_name))
    )
}

/// v4 `buildUserCharacterOpaqueContent`.
pub fn build_user_character_opaque_content(
    user_character_name: &str,
    user_character_description: Option<&str>,
) -> String {
    let desc = user_character_description.map(js_trim).unwrap_or("");
    if desc.is_empty() {
        return format!("{user_character_name} is the user's voice in this conversation.");
    }
    format!(
        "{user_character_name} is the user's voice in this conversation:\n\n{}",
        apply_host_templates(desc, None, Some(user_character_name))
    )
}

/// v4 `buildMultiCharacterRosterContent`. Empty roster → the "stands alone"
/// fallback (reuses [`build_multi_character_context_section`]).
pub fn build_multi_character_roster_content(
    responding_character_name: &str,
    others: &[OtherParticipant],
) -> String {
    let section = build_multi_character_context_section(others, responding_character_name);
    if section.is_empty() {
        return format!(
            "The Host notes that, for the moment, {responding_character_name} stands alone in the Salon."
        );
    }
    format!("The Host outlines the company present in the Salon:\n\n{section}")
}

/// v4 `buildMultiCharacterRosterOpaqueContent`.
pub fn build_multi_character_roster_opaque_content(
    responding_character_name: &str,
    others: &[OtherParticipant],
) -> String {
    let section = build_multi_character_context_section(others, responding_character_name);
    if section.is_empty() {
        return format!("For the moment, {responding_character_name} stands alone in the scene.");
    }
    format!("The company present in the scene:\n\n{section}")
}

const SILENT_MODE_ENTRY_BODY: &str = "You have entered SILENT mode. You are present in the scene but MUST NOT speak out loud — no dialogue that others can hear. You may:\n- Have inner thoughts and internal monologue (use *italics* or describe as thoughts)\n- Take physical actions (gestures, movements, facial expressions)\n- React emotionally or physically to what others say and do\n\nYou MUST NOT:\n- Speak any dialogue out loud\n- Whisper, murmur, or make any vocal sounds others could hear\n- Communicate verbally in any way\n";

/// v4 `buildSilentModeEntryContent`.
pub fn build_silent_mode_entry_content(character_name: &str) -> String {
    format!(
        "The Host whispers a private note to {character_name} alone:\n\n{SILENT_MODE_ENTRY_BODY}\nThis rule remains in force until the Host whispers that you are no longer silent."
    )
}

/// v4 `buildSilentModeEntryOpaqueContent`.
pub fn build_silent_mode_entry_opaque_content(_character_name: &str) -> String {
    format!(
        "{SILENT_MODE_ENTRY_BODY}\nThis rule remains in force until you are notified that you are no longer silent."
    )
}

/// v4 `buildSilentModeExitContent`.
pub fn build_silent_mode_exit_content(character_name: &str) -> String {
    format!(
        "The Host whispers a private note to {character_name} alone: silence is lifted. You may speak aloud again."
    )
}

/// v4 `buildSilentModeExitOpaqueContent`.
pub fn build_silent_mode_exit_opaque_content(_character_name: &str) -> String {
    "Silence is lifted. You may speak aloud again.".to_string()
}

/// v4 `buildJoinScenarioContent`.
pub fn build_join_scenario_content(
    character_name: &str,
    join_scenario: &str,
    user_character_name: Option<&str>,
) -> String {
    format!(
        "The Host whispers a private note to {character_name} alone, recounting how they came to be here:\n\n{}",
        apply_host_templates(js_trim(join_scenario), Some(character_name), user_character_name)
    )
}

/// v4 `buildJoinScenarioOpaqueContent`.
pub fn build_join_scenario_opaque_content(
    character_name: &str,
    join_scenario: &str,
    user_character_name: Option<&str>,
) -> String {
    format!(
        "How you came to be here:\n\n{}",
        apply_host_templates(
            js_trim(join_scenario),
            Some(character_name),
            user_character_name
        )
    )
}

/// v4 `buildTimestampContent`.
pub fn build_timestamp_content(formatted: &str) -> String {
    format!("The Host marks the time as {formatted}.")
}

/// v4 `buildTimestampOpaqueContent`.
pub fn build_timestamp_opaque_content(formatted: &str) -> String {
    format!("Current time: {formatted}.")
}

/// v4 `buildContinuationFromContent` (private in v4).
pub fn build_continuation_from_content(source_chat_id: &str, source_title: Option<&str>) -> String {
    let link_text = continuation_link_text(source_title, "an earlier chapter");
    format!(
        "The Host raises a hand for attention: this conversation continues from [{link_text}](/salon/{source_chat_id}).\n\nThe thread that brought us here is preserved below. Carry on."
    )
}

/// v4 `buildContinuationFromOpaqueContent` (private in v4).
pub fn build_continuation_from_opaque_content(
    source_chat_id: &str,
    source_title: Option<&str>,
) -> String {
    let link_text = continuation_link_text(source_title, "an earlier chapter");
    format!(
        "This conversation continues from [{link_text}](/salon/{source_chat_id}).\n\nThe thread that brought us here is preserved below. Carry on."
    )
}

/// v4 `buildContinuationToContent` (private in v4).
pub fn build_continuation_to_content(new_chat_id: &str, new_title: Option<&str>) -> String {
    let link_text = continuation_link_text(new_title, "a new venue");
    format!(
        "The Host clears their throat: the conversation has moved to [{link_text}](/salon/{new_chat_id}). The proceedings continue there."
    )
}

/// v4 `buildContinuationToOpaqueContent` (private in v4).
pub fn build_continuation_to_opaque_content(new_chat_id: &str, new_title: Option<&str>) -> String {
    let link_text = continuation_link_text(new_title, "a new venue");
    format!(
        "The conversation has moved to [{link_text}](/salon/{new_chat_id}). The proceedings continue there."
    )
}

/// v4's `const linkText = trimmedTitle.length > 0 ? \`"${trimmedTitle}"\` : fallback`.
fn continuation_link_text(title: Option<&str>, fallback: &str) -> String {
    let trimmed = title.map(js_trim).unwrap_or("");
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        format!("\"{trimmed}\"")
    }
}

/// v4 `buildMergeFromContent` (private in v4).
pub fn build_merge_from_content(
    source_chat_id: &str,
    source_title: Option<&str>,
    summary_text: Option<&str>,
) -> String {
    let link_text = continuation_link_text(source_title, "another conversation");
    let trimmed_summary = summary_text.map(js_trim).unwrap_or("");
    let mut lines: Vec<String> = vec![
        format!(
            "The Host raises a hand for attention: the company of [{link_text}](/salon/{source_chat_id}) joins these proceedings, their thread woven into ours."
        ),
        String::new(),
    ];
    if !trimmed_summary.is_empty() {
        lines.push("Where they left off:".to_string());
        lines.push(String::new());
        lines.push(trimmed_summary.to_string());
    } else {
        lines.push(
            "Their earlier thread carries no recorded summary as yet — carry on regardless."
                .to_string(),
        );
    }
    lines.join("\n")
}

/// v4 `buildMergeFromOpaqueContent` (private in v4).
pub fn build_merge_from_opaque_content(
    source_chat_id: &str,
    source_title: Option<&str>,
    summary_text: Option<&str>,
) -> String {
    let link_text = continuation_link_text(source_title, "another conversation");
    let trimmed_summary = summary_text.map(js_trim).unwrap_or("");
    let mut lines: Vec<String> = vec![
        format!(
            "The company of [{link_text}](/salon/{source_chat_id}) has been merged into this conversation. Where they left off:"
        ),
        String::new(),
    ];
    if !trimmed_summary.is_empty() {
        lines.push(trimmed_summary.to_string());
    } else {
        lines.push("(No recorded summary from that conversation yet.)".to_string());
    }
    lines.join("\n")
}

/// v4 `buildMergeToContent` (private in v4).
pub fn build_merge_to_content(target_chat_id: &str, target_title: Option<&str>) -> String {
    let link_text = continuation_link_text(target_title, "another conversation");
    format!(
        "The Host clears their throat: this conversation's company and thread have been merged into [{link_text}](/salon/{target_chat_id}). The proceedings continue there."
    )
}

/// v4 `buildMergeToOpaqueContent` (private in v4).
pub fn build_merge_to_opaque_content(target_chat_id: &str, target_title: Option<&str>) -> String {
    let link_text = continuation_link_text(target_title, "another conversation");
    format!(
        "This conversation's company and thread have been merged into [{link_text}](/salon/{target_chat_id}). The proceedings continue there."
    )
}

/// v4 `buildNoUserCharacterContent` (private in v4).
pub fn build_no_user_character_content() -> String {
    "The Host clears their throat with the gentlest of coughs.\n\nNo user-controlled character has been attached to this conversation, so the Commonplace Book cannot record what your characters come to know about *you* — only what they come to know about themselves.\n\nTo begin gathering memories about the user, attach (or create) a user-controlled character via the Participants sidebar.".to_string()
}

// ===========================================================================
// Private DB helpers (mirror v4's async internals)
// ===========================================================================

/// v4 `resolveUserCharacterName`: find the user-controlled CHARACTER participant
/// and return the overlaid character's name (the `{{user}}` binding), or `None`.
async fn resolve_user_character_name(db: &Db, participants: &Value) -> Option<String> {
    let arr = participants.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let user_participant = arr.iter().find(|p| {
        p.get("type").and_then(Value::as_str) == Some("CHARACTER")
            && p.get("controlledBy").and_then(Value::as_str) == Some("user")
            && p.get("characterId")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
    })?;
    let character_id = user_participant
        .get("characterId")
        .and_then(Value::as_str)?;
    // v4 wraps in try/catch → null; the overlaid read spans both connections.
    let character = db
        .read_main(|main| {
            db.read_mount_index(|mount| characters_read::find_by_id(main, mount, character_id))
        })
        .ok()
        .flatten()?;
    character
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// v4 `readVaultIdentity`: read + trim the character's vault `identity.md`,
/// returning `Some` only when non-empty. Any failure (no mount point, no
/// mount-index DB, not found) → `None` (v4's try/catch → null).
async fn read_vault_identity(db: &Db, mount_point_id: Option<&str>) -> Option<String> {
    let mount_point_id = mount_point_id?;
    let content = db
        .read_mount_index(|mount| {
            match read_database_document(mount, mount_point_id, "identity.md") {
                Ok(doc) => Ok(Some(doc.content)),
                Err(_) => Ok(None),
            }
        })
        .ok()
        .flatten()?;
    let trimmed = js_trim(&content);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ===========================================================================
// Post primitives
// ===========================================================================

/// A persisted Host message, returned so callers can splice it into the current
/// turn's in-memory context (v4 returns the `MessageEvent`).
#[derive(Debug, Clone)]
pub struct PostedHostMessage {
    pub id: String,
    /// The full `MessageEvent` object literal that was validated + inserted.
    pub message: Value,
}

/// v4 `postHostMessage`: assemble the public Host `MessageEvent` (with an
/// optional `hostEvent` payload), validate, and insert. Best-effort → `None`.
///
/// `system_kind` normalization mirrors v4: a `status:<from>-><to>` label is
/// stored as the stable bucket `"status-change"`.
async fn post_host_message(
    db: &Db,
    chat_id: &str,
    content: String,
    opaque_content: Option<String>,
    kind_label: &str,
    host_event: Option<Value>,
) -> Option<PostedHostMessage> {
    // v4: findById → if !chat return null.
    let chat_id_owned = chat_id.to_string();
    db.read_main(|main| chats_read::find_by_id(main, &chat_id_owned))
        .ok()
        .flatten()?;

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let system_kind = if kind_label.starts_with("status:") {
        "status-change".to_string()
    } else {
        kind_label.to_string()
    };

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": opaque_content,
        "attachments": [],
        "createdAt": now,
        "participantId": Value::Null,
        "systemSender": "host",
        "systemKind": system_kind,
        "hostEvent": host_event.unwrap_or(Value::Null),
    });

    insert_host_message(db, chat_id, message_id, message).await
}

/// v4 `postHostMessageWithTargets`: assemble a (possibly targeted) Host
/// `MessageEvent` and insert. `system_kind` normalization: a
/// `silent-mode:<transition>` label is stored as `silent-mode-<transition>`.
async fn post_host_message_with_targets(
    db: &Db,
    chat_id: &str,
    content: String,
    opaque_content: Option<String>,
    kind_label: &str,
    target_participant_ids: Option<Vec<String>>,
) -> Option<PostedHostMessage> {
    let chat_id_owned = chat_id.to_string();
    db.read_main(|main| chats_read::find_by_id(main, &chat_id_owned))
        .ok()
        .flatten()?;

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let system_kind = match kind_label.strip_prefix("silent-mode:") {
        Some(transition) => format!("silent-mode-{transition}"),
        None => kind_label.to_string(),
    };
    let targets: Value = match target_participant_ids {
        Some(ids) if !ids.is_empty() => Value::from(ids),
        _ => Value::Null,
    };

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": opaque_content,
        "attachments": [],
        "createdAt": now,
        "participantId": Value::Null,
        "systemSender": "host",
        "systemKind": system_kind,
        "targetParticipantIds": targets,
    });

    insert_host_message(db, chat_id, message_id, message).await
}

/// Validate the assembled object into a [`ChatEventInput`] and insert it on the
/// writer thread; a validation or persistence failure → `None`.
async fn insert_host_message(
    db: &Db,
    chat_id: &str,
    message_id: String,
    message: Value,
) -> Option<PostedHostMessage> {
    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    let chat_id_owned = chat_id.to_string();
    match db
        .write(move |writers| {
            writers
                .main()
                .chat_messages()
                .add_message(&chat_id_owned, &event)
        })
        .await
    {
        Ok(()) => Some(PostedHostMessage {
            id: message_id,
            message,
        }),
        Err(_) => None,
    }
}

// ===========================================================================
// Post functions (the public surface)
// ===========================================================================

/// v4 `HostAddAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostAddAnnouncement {
    pub chat_id: String,
    pub character: HostCharacter,
    /// Participant id of the joining character (stamped into `hostEvent`).
    pub participant_id: String,
    /// Initial participation status (defaults to `"active"`).
    pub initial_status: Option<String>,
}

/// v4 `postHostAddAnnouncement`.
pub async fn post_host_add_announcement(
    db: &Db,
    params: HostAddAnnouncement,
) -> Option<PostedHostMessage> {
    // Resolve the user-character name for `{{user}}` binding (chat may not exist).
    let user_character_name = match db
        .read_main(|main| chats_read::find_by_id(main, &params.chat_id))
        .ok()
        .flatten()
    {
        Some(chat) => match chat.get("participants") {
            Some(p) => resolve_user_character_name(db, p).await,
            None => None,
        },
        None => None,
    };

    let identity = read_vault_identity(
        db,
        params
            .character
            .character_document_mount_point_id
            .as_deref(),
    )
    .await;
    let content = build_add_content(
        &params.character.name,
        identity.as_deref(),
        params.character.description.as_deref(),
        user_character_name.as_deref(),
    );
    let opaque_content = build_add_opaque_content(
        &params.character.name,
        identity.as_deref(),
        params.character.description.as_deref(),
        user_character_name.as_deref(),
    );

    let to_status = params.initial_status.as_deref().unwrap_or("active");
    let host_event = json!({
        "participantId": params.participant_id,
        "toStatus": to_status,
    });
    post_host_message(
        db,
        &params.chat_id,
        content,
        Some(opaque_content),
        "add",
        Some(host_event),
    )
    .await
}

/// v4 `HostRemoveAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostRemoveAnnouncement {
    pub chat_id: String,
    pub character_name: String,
    pub participant_id: String,
}

/// v4 `postHostRemoveAnnouncement`.
pub async fn post_host_remove_announcement(
    db: &Db,
    params: HostRemoveAnnouncement,
) -> Option<PostedHostMessage> {
    let content = build_remove_content(&params.character_name);
    let opaque_content = build_remove_opaque_content(&params.character_name);
    let host_event = json!({
        "participantId": params.participant_id,
        "toStatus": "removed",
    });
    post_host_message(
        db,
        &params.chat_id,
        content,
        Some(opaque_content),
        "remove",
        Some(host_event),
    )
    .await
}

/// v4 `HostStatusChangeAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostStatusChangeAnnouncement {
    pub chat_id: String,
    pub character_name: String,
    pub participant_id: String,
    pub old_status: String,
    pub new_status: String,
}

/// v4 `postHostStatusChangeAnnouncement`.
pub async fn post_host_status_change_announcement(
    db: &Db,
    params: HostStatusChangeAnnouncement,
) -> Option<PostedHostMessage> {
    let content = build_status_change_content(
        &params.character_name,
        &params.old_status,
        &params.new_status,
    );
    let opaque_content = build_status_change_opaque_content(
        &params.character_name,
        &params.old_status,
        &params.new_status,
    );
    let kind_label = format!("status:{}->{}", params.old_status, params.new_status);
    let host_event = json!({
        "participantId": params.participant_id,
        "toStatus": params.new_status,
    });
    post_host_message(
        db,
        &params.chat_id,
        content,
        Some(opaque_content),
        &kind_label,
        Some(host_event),
    )
    .await
}

/// v4 `HostScenarioAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostScenarioAnnouncement {
    pub chat_id: String,
    pub scenario_text: String,
}

/// v4 `postHostScenarioAnnouncement`. Empty/blank scenario → `None`.
pub async fn post_host_scenario_announcement(
    db: &Db,
    params: HostScenarioAnnouncement,
) -> Option<PostedHostMessage> {
    if js_trim(&params.scenario_text).is_empty() {
        return None;
    }
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_scenario_content(&params.scenario_text),
        Some(build_scenario_opaque_content(&params.scenario_text)),
        "scenario",
        None,
    )
    .await
}

/// v4 `HostUserCharacterAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostUserCharacterAnnouncement {
    pub chat_id: String,
    pub user_character_name: String,
    pub user_character_description: Option<String>,
}

/// v4 `postHostUserCharacterAnnouncement`.
pub async fn post_host_user_character_announcement(
    db: &Db,
    params: HostUserCharacterAnnouncement,
) -> Option<PostedHostMessage> {
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_user_character_content(
            &params.user_character_name,
            params.user_character_description.as_deref(),
        ),
        Some(build_user_character_opaque_content(
            &params.user_character_name,
            params.user_character_description.as_deref(),
        )),
        "user-character",
        None,
    )
    .await
}

/// v4 `HostMultiCharacterRosterAnnouncement` (implicit — the roster post).
#[derive(Clone, Debug)]
pub struct HostMultiCharacterRosterAnnouncement {
    pub chat_id: String,
    pub responding_character_name: String,
    pub others: Vec<OtherParticipant>,
}

/// v4 `postHostMultiCharacterRosterAnnouncement`.
pub async fn post_host_multi_character_roster_announcement(
    db: &Db,
    params: HostMultiCharacterRosterAnnouncement,
) -> Option<PostedHostMessage> {
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_multi_character_roster_content(&params.responding_character_name, &params.others),
        Some(build_multi_character_roster_opaque_content(
            &params.responding_character_name,
            &params.others,
        )),
        "multi-character-roster",
        None,
    )
    .await
}

/// v4 `HostSilentModeAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostSilentModeAnnouncement {
    pub chat_id: String,
    pub character_name: String,
    /// Participant id — the whisper is private to this character.
    pub target_participant_id: String,
    /// `"enter"` or `"exit"`.
    pub transition: String,
}

/// v4 `postHostSilentModeAnnouncement`.
pub async fn post_host_silent_mode_announcement(
    db: &Db,
    params: HostSilentModeAnnouncement,
) -> Option<PostedHostMessage> {
    let is_enter = params.transition == "enter";
    let content = if is_enter {
        build_silent_mode_entry_content(&params.character_name)
    } else {
        build_silent_mode_exit_content(&params.character_name)
    };
    let opaque_content = if is_enter {
        build_silent_mode_entry_opaque_content(&params.character_name)
    } else {
        build_silent_mode_exit_opaque_content(&params.character_name)
    };
    let kind_label = format!("silent-mode:{}", params.transition);
    post_host_message_with_targets(
        db,
        &params.chat_id,
        content,
        Some(opaque_content),
        &kind_label,
        Some(vec![params.target_participant_id]),
    )
    .await
}

/// v4 `HostJoinScenarioAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostJoinScenarioAnnouncement {
    pub chat_id: String,
    pub character_name: String,
    /// Participant id of the joining character (whisper is private to them).
    pub target_participant_id: String,
    pub join_scenario: String,
}

/// v4 `postHostJoinScenarioAnnouncement`. Empty/blank scenario → `None`.
pub async fn post_host_join_scenario_announcement(
    db: &Db,
    params: HostJoinScenarioAnnouncement,
) -> Option<PostedHostMessage> {
    if js_trim(&params.join_scenario).is_empty() {
        return None;
    }
    let user_character_name = match db
        .read_main(|main| chats_read::find_by_id(main, &params.chat_id))
        .ok()
        .flatten()
    {
        Some(chat) => match chat.get("participants") {
            Some(p) => resolve_user_character_name(db, p).await,
            None => None,
        },
        None => None,
    };
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_join_scenario_content(
            &params.character_name,
            &params.join_scenario,
            user_character_name.as_deref(),
        ),
        Some(build_join_scenario_opaque_content(
            &params.character_name,
            &params.join_scenario,
            user_character_name.as_deref(),
        )),
        "join-scenario",
        Some(vec![params.target_participant_id]),
    )
    .await
}

/// v4 `HostTimestampAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostTimestampAnnouncement {
    pub chat_id: String,
    pub formatted: String,
}

/// v4 `postHostTimestampAnnouncement`. Empty/blank formatted string → `None`.
pub async fn post_host_timestamp_announcement(
    db: &Db,
    params: HostTimestampAnnouncement,
) -> Option<PostedHostMessage> {
    if js_trim(&params.formatted).is_empty() {
        return None;
    }
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_timestamp_content(&params.formatted),
        Some(build_timestamp_opaque_content(&params.formatted)),
        "timestamp",
        None,
    )
    .await
}

/// v4 `HostOffSceneCharactersAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostOffSceneCharactersAnnouncement {
    pub chat_id: String,
    pub characters: Vec<OffSceneCharacterCard>,
}

/// v4 `postHostOffSceneCharactersAnnouncement`. Empty input → `None`. Stamps the
/// introduced character ids into `hostEvent.introducedCharacterIds` (public,
/// untargeted).
pub async fn post_host_off_scene_characters_announcement(
    db: &Db,
    params: HostOffSceneCharactersAnnouncement,
) -> Option<PostedHostMessage> {
    if params.characters.is_empty() {
        return None;
    }
    let chat = db
        .read_main(|main| chats_read::find_by_id(main, &params.chat_id))
        .ok()
        .flatten()?;
    let user_character_name = match chat.get("participants") {
        Some(p) => resolve_user_character_name(db, p).await,
        None => None,
    };
    let content =
        build_off_scene_characters_content(&params.characters, user_character_name.as_deref());
    let opaque_content = build_off_scene_characters_opaque_content(
        &params.characters,
        user_character_name.as_deref(),
    );
    let introduced_character_ids: Vec<String> =
        params.characters.iter().map(|c| c.id.clone()).collect();
    let host_event = json!({ "introducedCharacterIds": introduced_character_ids });
    post_host_message(
        db,
        &params.chat_id,
        content,
        Some(opaque_content),
        "off-scene-characters",
        Some(host_event),
    )
    .await
}

/// v4 `HostContinuationFromAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostContinuationFromAnnouncement {
    pub chat_id: String,
    pub source_chat_id: String,
    pub source_title: Option<String>,
}

/// v4 `postHostContinuationFromAnnouncement`.
pub async fn post_host_continuation_from_announcement(
    db: &Db,
    params: HostContinuationFromAnnouncement,
) -> Option<PostedHostMessage> {
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_continuation_from_content(&params.source_chat_id, params.source_title.as_deref()),
        Some(build_continuation_from_opaque_content(
            &params.source_chat_id,
            params.source_title.as_deref(),
        )),
        "continuation-from",
        None,
    )
    .await
}

/// v4 `HostContinuationToAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostContinuationToAnnouncement {
    pub chat_id: String,
    pub new_chat_id: String,
    pub new_title: Option<String>,
}

/// v4 `postHostContinuationToAnnouncement`.
pub async fn post_host_continuation_to_announcement(
    db: &Db,
    params: HostContinuationToAnnouncement,
) -> Option<PostedHostMessage> {
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_continuation_to_content(&params.new_chat_id, params.new_title.as_deref()),
        Some(build_continuation_to_opaque_content(
            &params.new_chat_id,
            params.new_title.as_deref(),
        )),
        "continuation-to",
        None,
    )
    .await
}

/// v4 `HostMergeFromAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostMergeFromAnnouncement {
    /// The receiving chat (where newcomers and their summary land).
    pub chat_id: String,
    pub source_chat_id: String,
    pub source_title: Option<String>,
    /// The source chat's rolling context summary, embedded as the recap.
    pub summary_text: Option<String>,
}

/// v4 `postHostMergeFromAnnouncement`.
pub async fn post_host_merge_from_announcement(
    db: &Db,
    params: HostMergeFromAnnouncement,
) -> Option<PostedHostMessage> {
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_merge_from_content(
            &params.source_chat_id,
            params.source_title.as_deref(),
            params.summary_text.as_deref(),
        ),
        Some(build_merge_from_opaque_content(
            &params.source_chat_id,
            params.source_title.as_deref(),
            params.summary_text.as_deref(),
        )),
        "merge-from",
        None,
    )
    .await
}

/// v4 `HostMergeToAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostMergeToAnnouncement {
    /// The source chat (whose company/thread were merged away).
    pub chat_id: String,
    pub target_chat_id: String,
    pub target_title: Option<String>,
}

/// v4 `postHostMergeToAnnouncement`.
pub async fn post_host_merge_to_announcement(
    db: &Db,
    params: HostMergeToAnnouncement,
) -> Option<PostedHostMessage> {
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_merge_to_content(&params.target_chat_id, params.target_title.as_deref()),
        Some(build_merge_to_opaque_content(
            &params.target_chat_id,
            params.target_title.as_deref(),
        )),
        "merge-to",
        None,
    )
    .await
}

/// v4 `HostNoUserCharacterAnnouncement`.
#[derive(Clone, Debug)]
pub struct HostNoUserCharacterAnnouncement {
    pub chat_id: String,
}

/// v4 `postHostNoUserCharacterAnnouncement`. See the module doc's "faithful
/// quirk" note: on the SQLite backend the idempotence guard's scanned array is
/// always empty (metadata-only `findById`), so this posts unconditionally after
/// the existence check. No opaque variant (operator-only whisper).
pub async fn post_host_no_user_character_announcement(
    db: &Db,
    params: HostNoUserCharacterAnnouncement,
) -> Option<PostedHostMessage> {
    // v4: findById → if !chat return null. The `chat.messages` scan is a no-op
    // on the metadata-only read (always empty → never already-posted).
    db.read_main(|main| chats_read::find_by_id(main, &params.chat_id))
        .ok()
        .flatten()?;
    post_host_message_with_targets(
        db,
        &params.chat_id,
        build_no_user_character_content(),
        None,
        "no-user-character",
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_content_identity_branch() {
        // The identity branch is not reachable by the DB-free tier-1 oracle
        // (v4 reads `identity.md` from the DB), so pin the exact string here.
        let got = build_add_content(
            "Ada",
            Some("A brilliant analyst named {{char}} who knows {{user}}."),
            Some("ignored when identity present"),
            Some("Charlie"),
        );
        assert_eq!(
            got,
            "The Host welcomes Ada to the Salon.\n\n**Identity:**\n\nA brilliant analyst named Ada who knows Charlie."
        );
    }

    #[test]
    fn add_opaque_identity_branch() {
        let got = build_add_opaque_content("Ada", Some("Just Ada."), None, None);
        assert_eq!(
            got,
            "Ada has joined the scene.\n\n**Identity:**\n\nJust Ada."
        );
    }

    #[test]
    fn status_phrase_fallback() {
        assert_eq!(status_phrase("active"), "present and speaking freely");
        assert_eq!(status_phrase("mystery"), "mystery");
    }
}
