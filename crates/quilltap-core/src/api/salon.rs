//! The Salon-server dispatch handlers (P4.6a) — the route-logic backfill the
//! first Salon vertical consumes, composed over already-ported repos/services.
//!
//! Each handler is a differential port of a v4 route handler (the oracle) and
//! returns a [`Response`] directly (the engine arm is a one-line delegate). The
//! reads nest [`Db::read_main`] + [`Db::read_mount_index`] (separate pools → no
//! deadlock) for the vault overlay; writes go through [`Db::write`]; the two
//! model-needing branches (swipe **generate**) route through a driver seam and
//! are NOT here.
//!
//! `user_id` is a parameter (not hard-coded `SINGLE_USER_ID`) so the differential
//! harness can drive with the fixture's own user id on both sides; the engine
//! passes `SINGLE_USER_ID`.

use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_read, DbError};
use crate::services::carina_query::BRAHMA_CARINA_ANSWERER_ID;
use crate::services::{ariel_notifications, chat_enrichment, suparna_notifications};

use super::types::{ChatWrapDto, ErrorKind, Response};

/// Read helper: run `f` with BOTH a main and a mount-index connection (the vault
/// overlay needs both). Errors [`DbError::PartitionUnavailable`] if the instance
/// has no mount-index DB (never the case for a provisioned Salon instance).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

// ===========================================================================
// The chat-settings read (v4 GET /api/v1/settings/chat)
// ===========================================================================

/// v4 `GET /api/v1/settings/chat` (`handler` → `successResponse(findByUserId)`).
/// The default-injection branch (no row → `updateForUser` with defaults) is a
/// tracked deferral (needs the unported `updateForUser`); a provisioned Salon
/// instance always has a settings row.
pub fn chat_settings(db: &Db, user_id: &str) -> Response {
    let user_id = user_id.to_string();
    match db.read_main(move |conn| chat_settings::find_by_user_id(conn, &user_id)) {
        Ok(Some(settings)) => Response::ChatSettings(settings),
        Ok(None) => Response::error(
            ErrorKind::Internal,
            "chat settings not provisioned (default-injection is a tracked deferral)",
        ),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// The enriched chat list (v4 GET /api/v1/chats → handleList)
// ===========================================================================

/// v4 `handleList` + `enrichChatsForList` (the no-preloaded path).
pub fn list_chats(
    db: &Db,
    user_id: &str,
    exclude_tag_ids: &[String],
    limit: Option<i64>,
    include_autonomous: bool,
) -> Response {
    let user_id_owned = user_id.to_string();
    let all = match db.read_main(move |conn| chats_read::find_by_user_id(conn, &user_id_owned)) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };

    // v4's chatType filter: salon + legacy-null kept; help/brahma dropped;
    // autonomous only when includeAutonomous OR runVisibility ∈ {household, open}.
    let filtered: Vec<Value> = all
        .into_iter()
        .filter(|c| {
            let chat_type = c.get("chatType").and_then(Value::as_str);
            match chat_type {
                None | Some("salon") => true,
                Some("autonomous") => {
                    include_autonomous
                        || matches!(
                            c.get("runVisibility").and_then(Value::as_str),
                            Some("household") | Some("open")
                        )
                }
                _ => false,
            }
        })
        .collect();

    // enrich (sorts internally) → filter by excluded tags → limit slice → clean.
    let enriched = match read_main_mount(db, |main, mount| {
        chat_enrichment::enrich_chats_for_list(main, mount, filtered)
    }) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    let mut filtered = chat_enrichment::filter_chats_by_excluded_tags(enriched, exclude_tag_ids);
    if let Some(n) = limit {
        if n > 0 && (n as usize) < filtered.len() {
            filtered.truncate(n as usize);
        }
    }
    Response::Chats(filtered)
}

// ===========================================================================
// The single-chat GET (v4 GET /api/v1/chats/{id} → handleGet default branch)
// ===========================================================================

/// v4 `handleGet` (default branch): the fully-enriched chat + all messages
/// (minus `renderedHtml`, the locked divergence).
///
/// `is_live` is the terminal-session PTY probe (the host's terminal manager);
/// the engine passes `|_| false` (a live-PTY reconcile during a read GET is a
/// host-driver concern — a tracked deferral; a provisioned Salon read is
/// unaffected: no live sessions → reconcile no-op).
pub async fn chat_get(db: &Db, user_id: &str, chat_id: &str) -> Response {
    // 1. Ownership-free slim read (single-user).
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => return internal(e),
    };

    // 2. Pre-read side effects (both best-effort in v4).
    let is_live = |_: &str| false;
    ariel_notifications::reconcile_terminal_sessions_for_chat(db, chat_id, &is_live).await;
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    suparna_notifications::surface_operator_mail_for_chat(db, chat_id, &participants).await;

    // Everything below is a read composition (main + mount for the overlay).
    let chat_id_s = chat_id.to_string();
    let user_id_s = user_id.to_string();
    let result = read_main_mount(db, move |main, mount| {
        assemble_chat_get(main, mount, &chat, &participants, &chat_id_s, &user_id_s)
    });
    match result {
        Ok(v) => Response::Chat(ChatWrapDto { chat: v }),
        Err(e) => internal(e),
    }
}

/// The synchronous read-composition core of [`chat_get`] (steps 4–12 of v4's
/// `handleGet`). Attachment resolution is a tracked deferral (the corpus bakes no
/// message attachments; `attachments: []` on both sides).
fn assemble_chat_get(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    chat: &Value,
    participants: &[Value],
    chat_id: &str,
    user_id: &str,
) -> Result<Value, DbError> {
    use crate::db::{chats_messages_read, projects::ProjectsRepository};

    // Enriched participants (detail).
    let mut enriched_participants = Vec::with_capacity(participants.len());
    for p in participants {
        enriched_participants.push(chat_enrichment::enrich_participant_detail(
            main, mount, p, chat_id,
        )?);
    }

    // All messages, projected (minus renderedHtml + attachments).
    let events = chats_messages_read::get_messages(main, chat_id)?;
    let mut messages: Vec<Value> = Vec::new();
    for e in &events {
        if e.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        messages.push(project_message(e));
    }

    // Off-scene characters (customAnnouncer.characterId + carinaMeta.answererId
    // non-participants), resolved via getCharacterDetail (4-field subset).
    let participant_char_ids: std::collections::HashSet<String> = participants
        .iter()
        .filter_map(|p| s(p, "characterId"))
        .collect();
    let mut off_scene_ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let push_off =
        |id: &str, seen: &mut std::collections::HashSet<String>, out: &mut Vec<String>| {
            if !participant_char_ids.contains(id) && seen.insert(id.to_string()) {
                out.push(id.to_string());
            }
        };
    for m in &messages {
        if let Some(id) = m
            .get("customAnnouncer")
            .and_then(|c| c.get("characterId"))
            .and_then(Value::as_str)
        {
            push_off(id, &mut seen, &mut off_scene_ids);
        }
        if let Some(id) = m
            .get("carinaMeta")
            .and_then(|c| c.get("answererId"))
            .and_then(Value::as_str)
        {
            if id != BRAHMA_CARINA_ANSWERER_ID {
                push_off(id, &mut seen, &mut off_scene_ids);
            }
        }
    }
    let mut off_scene_characters: Vec<Value> = Vec::new();
    for cid in &off_scene_ids {
        if let Some(detail) =
            chat_enrichment::get_character_detail(main, mount, cid, Some(chat_id))?
        {
            off_scene_characters.push(json!({
                "id": detail.id,
                "name": detail.name,
                "title": detail.title,
                "avatarUrl": detail.avatar_url,
            }));
        }
    }

    // projectName + agent-mode cascade.
    let project = match s(chat, "projectId") {
        Some(pid) => ProjectsRepository::new(main, mount)
            .find_by_id(&pid)
            .map_err(|e| DbError::Key(format!("project overlay: {e}")))?,
        None => None,
    };
    let project_name = project.as_ref().and_then(|p| s(p, "name"));

    let primary_character = participants
        .iter()
        .find(|p| s(p, "type").as_deref() == Some("CHARACTER") && p.get("characterId").is_some())
        .and_then(|p| s(p, "characterId"))
        .and_then(|cid| {
            crate::db::characters_read::find_by_id(main, mount, &cid)
                .ok()
                .flatten()
        });
    let settings = chat_settings::find_by_user_id(main, user_id)?;
    let (agent_enabled, agent_source) = resolve_agent_mode(
        chat,
        project.as_ref(),
        primary_character.as_ref(),
        settings.as_ref(),
    );

    // user block ({ id, name?, image? } — nullable name/image dropped when NULL).
    let user = read_user_block(main, user_id)?;

    // Assemble the top-level chat object (v4 get.ts:517–556).
    let enriched_participants_v = serde_json::to_value(&enriched_participants)
        .map_err(|e| DbError::Key(format!("participants marshal: {e}")))?;
    let get_v = |k: &str| chat.get(k).cloned();
    let bool_or = |k: &str, d: bool| get_v(k).and_then(|v| v.as_bool()).unwrap_or(d);
    let arr_or_empty = |k: &str| {
        get_v(k)
            .filter(|v| v.is_array())
            .unwrap_or_else(|| json!([]))
    };

    let mut out = Map::new();
    out.insert("id".into(), json!(s(chat, "id")));
    out.insert("title".into(), get_v("title").unwrap_or(Value::Null));
    out.insert(
        "chatType".into(),
        json!(s(chat, "chatType").unwrap_or_else(|| "salon".to_string())),
    );
    // v4 passes these RAW (no `?? null`): the read drops them when NULL, so an
    // undefined value is omitted (present-only), unlike the `?? null` fields.
    if let Some(v) = get_v("contextSummary") {
        out.insert("contextSummary".into(), v);
    }
    if let Some(v) = get_v("roleplayTemplateId") {
        out.insert("roleplayTemplateId".into(), v);
    }
    out.insert(
        "imageProfileId".into(),
        get_v("imageProfileId").unwrap_or(Value::Null),
    );
    out.insert(
        "lastTurnParticipantId".into(),
        get_v("lastTurnParticipantId").unwrap_or(Value::Null),
    );
    out.insert("isPaused".into(), json!(bool_or("isPaused", false)));
    out.insert(
        "isManuallyRenamed".into(),
        json!(bool_or("isManuallyRenamed", false)),
    );
    out.insert(
        "updatedAt".into(),
        get_v("updatedAt").unwrap_or(Value::Null),
    );
    out.insert(
        "createdAt".into(),
        get_v("createdAt").unwrap_or(Value::Null),
    );
    out.insert("participants".into(), enriched_participants_v);
    out.insert("user".into(), user);
    out.insert("messages".into(), Value::Array(messages));
    out.insert(
        "projectId".into(),
        get_v("projectId")
            .filter(|v| !v.is_null())
            .unwrap_or(Value::Null),
    );
    out.insert("projectName".into(), json!(project_name));
    out.insert("disabledTools".into(), arr_or_empty("disabledTools"));
    out.insert(
        "disabledToolGroups".into(),
        arr_or_empty("disabledToolGroups"),
    );
    out.insert(
        "allowCrossCharacterVaultReads".into(),
        json!(bool_or("allowCrossCharacterVaultReads", false)),
    );
    out.insert(
        "coreWhisperEnabled".into(),
        get_v("coreWhisperEnabled").unwrap_or(Value::Null),
    );
    out.insert(
        "coreWhisperInterval".into(),
        get_v("coreWhisperInterval").unwrap_or(Value::Null),
    );
    out.insert(
        "turnSkippingEnabled".into(),
        get_v("turnSkippingEnabled").unwrap_or(Value::Null),
    );
    out.insert(
        "agentModeEnabled".into(),
        json!(bool_or("agentModeEnabled", false)),
    );
    out.insert("resolvedAgentModeEnabled".into(), json!(agent_enabled));
    out.insert("agentModeSource".into(), json!(agent_source));
    out.insert(
        "avatarGenerationEnabled".into(),
        get_v("avatarGenerationEnabled").unwrap_or(Value::Null),
    );
    out.insert(
        "isDangerousChat".into(),
        get_v("isDangerousChat").unwrap_or(Value::Null),
    );
    out.insert("dangerCategories".into(), arr_or_empty("dangerCategories"));
    out.insert(
        "conciergeOverride".into(),
        get_v("conciergeOverride").unwrap_or(Value::Null),
    );
    out.insert(
        "documentEditingMode".into(),
        json!(bool_or("documentEditingMode", false)),
    );
    out.insert(
        "documentMode".into(),
        json!(s(chat, "documentMode").unwrap_or_else(|| "normal".to_string())),
    );
    out.insert(
        "dividerPosition".into(),
        get_v("dividerPosition")
            .filter(|v| !v.is_null())
            .unwrap_or(json!(45)),
    );
    out.insert(
        "terminalMode".into(),
        json!(s(chat, "terminalMode").unwrap_or_else(|| "normal".to_string())),
    );
    out.insert(
        "activeTerminalSessionId".into(),
        get_v("activeTerminalSessionId").unwrap_or(Value::Null),
    );
    out.insert(
        "rightPaneVerticalSplit".into(),
        get_v("rightPaneVerticalSplit")
            .filter(|v| !v.is_null())
            .unwrap_or(json!(50)),
    );
    out.insert(
        "offSceneCharacters".into(),
        Value::Array(off_scene_characters),
    );

    Ok(Value::Object(out))
}

/// The per-message projection (v4 get.ts:373–435, minus `renderedHtml` +
/// attachment resolution). Reproduces v4's `X || null` (falsy→null) vs
/// `X ?? null`/`?? undefined` (nullish) operators exactly.
fn project_message(e: &Value) -> Value {
    // `X || null` — falsy (null/false/0/"") collapses to null.
    let or_null = |k: &str| -> Value {
        match e.get(k) {
            Some(v) if is_truthy(v) => v.clone(),
            _ => Value::Null,
        }
    };
    // `X ?? null` — only null/absent → null (false/0/"" preserved).
    let coalesce_null = |k: &str| -> Value {
        match e.get(k) {
            Some(v) if !v.is_null() => v.clone(),
            _ => Value::Null,
        }
    };

    let mut m = Map::new();
    m.insert("id".into(), e.get("id").cloned().unwrap_or(Value::Null));
    m.insert("role".into(), e.get("role").cloned().unwrap_or(Value::Null));
    m.insert(
        "content".into(),
        e.get("content").cloned().unwrap_or(Value::Null),
    );
    m.insert("tokenCount".into(), or_null("tokenCount"));
    m.insert("promptTokens".into(), or_null("promptTokens"));
    m.insert("completionTokens".into(), or_null("completionTokens"));
    m.insert(
        "createdAt".into(),
        e.get("createdAt").cloned().unwrap_or(Value::Null),
    );
    m.insert("swipeGroupId".into(), or_null("swipeGroupId"));
    m.insert("swipeIndex".into(), or_null("swipeIndex"));
    m.insert("participantId".into(), or_null("participantId"));
    // Attachment resolution deferred (corpus bakes none) → v4's `attachments`
    // default `[]`.
    m.insert("attachments".into(), json!([]));
    // debugMemoryLogs — `|| undefined` (omit when falsy).
    if let Some(v) = e.get("debugMemoryLogs") {
        if is_truthy(v) {
            m.insert("debugMemoryLogs".into(), v.clone());
        }
    }
    m.insert("provider".into(), or_null("provider"));
    m.insert("modelName".into(), or_null("modelName"));
    m.insert(
        "targetParticipantIds".into(),
        or_null("targetParticipantIds"),
    );
    m.insert("isSilentMessage".into(), or_null("isSilentMessage"));
    m.insert("systemSender".into(), or_null("systemSender"));
    m.insert("systemKind".into(), or_null("systemKind"));
    m.insert("hostEvent".into(), or_null("hostEvent"));
    m.insert("customAnnouncer".into(), or_null("customAnnouncer"));
    m.insert("carinaMeta".into(), or_null("carinaMeta"));
    m.insert(
        "pendingExternalPrompt".into(),
        or_null("pendingExternalPrompt"),
    );
    m.insert(
        "pendingExternalPromptFull".into(),
        or_null("pendingExternalPromptFull"),
    );
    m.insert(
        "pendingExternalAttachments".into(),
        or_null("pendingExternalAttachments"),
    );
    m.insert("reasoningContent".into(), or_null("reasoningContent"));
    m.insert("reasoningSegments".into(), or_null("reasoningSegments"));
    // confirmation family — `?? undefined` (omit) / `?? null` (emit null).
    if let Some(v) = e.get("confirmed") {
        if !v.is_null() {
            m.insert("confirmed".into(), v.clone());
        }
    }
    if let Some(v) = e.get("confirmationChecked") {
        if !v.is_null() {
            m.insert("confirmationChecked".into(), v.clone());
        }
    }
    if let Some(v) = e.get("confirmationRevised") {
        if !v.is_null() {
            m.insert("confirmationRevised".into(), v.clone());
        }
    }
    m.insert(
        "confirmationNotes".into(),
        coalesce_null("confirmationNotes"),
    );
    m.insert(
        "confirmationOriginalContent".into(),
        coalesce_null("confirmationOriginalContent"),
    );
    Value::Object(m)
}

/// v4 `resolveAgentModeSetting`'s `(enabled, enabledSource)` cascade
/// (Global → Character → Project → Chat). The ported
/// [`resolve_agent_mode_setting`](crate::services::agent_mode) drops
/// `enabledSource` (host diagnostics), so the GET handler re-derives both here.
fn resolve_agent_mode(
    chat: &Value,
    project: Option<&Value>,
    character: Option<&Value>,
    settings: Option<&Value>,
) -> (bool, &'static str) {
    let opt_bool = |v: Option<&Value>, k: &str| -> Option<bool> {
        v.and_then(|o| o.get(k)).and_then(Value::as_bool)
    };
    let global_default = settings
        .and_then(|s| s.get("agentModeSettings"))
        .and_then(|a| a.get("defaultEnabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut enabled = global_default;
    let mut source = "global";
    if let Some(c) = opt_bool(character, "defaultAgentModeEnabled") {
        enabled = c;
        source = "character";
    }
    if let Some(p) = opt_bool(project, "defaultAgentModeEnabled") {
        enabled = p;
        source = "project";
    }
    if let Some(ch) = chat.get("agentModeEnabled").and_then(Value::as_bool) {
        enabled = ch;
        source = "chat";
    }
    (enabled, source)
}

/// JS truthiness for a JSON value (`|| null` semantics): null/false/0/"" are
/// falsy; everything else (incl. arrays/objects) is truthy.
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// The `user` block: `{ id, name?, image? }` — name/image dropped when SQL NULL
/// (v4 hydrateRow → undefined → JSON.stringify drops the key).
fn read_user_block(main: &rusqlite::Connection, user_id: &str) -> Result<Value, DbError> {
    use rusqlite::OptionalExtension;
    let row = main
        .query_row(
            "SELECT id, name, image FROM users WHERE id = ?1",
            rusqlite::params![user_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    let mut u = Map::new();
    match row {
        Some((id, name, image)) => {
            u.insert("id".into(), json!(id));
            if let Some(n) = name {
                u.insert("name".into(), json!(n));
            }
            if let Some(i) = image {
                u.insert("image".into(), json!(i));
            }
        }
        None => {
            u.insert("id".into(), json!(user_id));
        }
    }
    Ok(Value::Object(u))
}

// ===========================================================================
// Shared helpers for the mutation handlers
// ===========================================================================

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

/// v4 `isParticipantPresent` — status ∈ {active, silent}.
fn participant_present(status: Option<&str>) -> bool {
    matches!(status, Some("active") | Some("silent") | None)
}

/// Find a participant Value by id in a chat's participants array.
fn find_participant<'a>(chat: &'a Value, participant_id: &str) -> Option<&'a Value> {
    chat.get("participants")
        .and_then(Value::as_array)?
        .iter()
        .find(|p| p.get("id").and_then(Value::as_str) == Some(participant_id))
}

/// Resolve a character's name (overlaid) for the impersonation-verb responses.
fn character_name(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    participant: &Value,
) -> String {
    participant
        .get("characterId")
        .and_then(Value::as_str)
        .and_then(|cid| {
            crate::db::characters_read::find_by_id(main, mount, cid)
                .ok()
                .flatten()
        })
        .and_then(|c| s(&c, "name"))
        .unwrap_or_else(|| "Unknown".to_string())
}

// ===========================================================================
// The turn action (v4 handleTurnAction)
// ===========================================================================

/// v4 `handleTurnAction`: validate the participant, the all-others-skipped guard,
/// then the ported [`handle_turn_action`](crate::services::turn_orchestrator) core
/// (RMW + persist + selection), the Host turn-pass announcement (skip only), and
/// v4's response envelope.
pub async fn turn_action(
    db: &Db,
    chat_id: &str,
    action_str: &str,
    participant_id: Option<&str>,
    random01: f64,
) -> Response {
    use crate::services::turn_orchestrator::{handle_turn_action, TurnAction};

    let action = match action_str {
        "nudge" => TurnAction::Nudge,
        "queue" => TurnAction::Queue,
        "dequeue" => TurnAction::Dequeue,
        "query" => TurnAction::Query,
        "skipUserTurn" => TurnAction::SkipUserTurn,
        _ => return bad_request("Invalid turn action"),
    };

    // Load the chat for validation.
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    // Non-query actions require a valid, present participant.
    if action != TurnAction::Query {
        let pid = match participant_id {
            Some(p) => p,
            None => return bad_request("Participant id is required"),
        };
        let Some(participant) = find_participant(&chat, pid) else {
            return not_found("Participant");
        };
        if !participant_present(participant.get("status").and_then(Value::as_str)) {
            return bad_request("Participant is not active");
        }
        if action == TurnAction::SkipUserTurn
            && participant.get("controlledBy").and_then(Value::as_str) != Some("user")
        {
            return bad_request("Only user-controlled participants can be skipped");
        }
    }

    // Skip guard: refuse a human skip when every other active character has passed.
    let mut turn_skipping_applies = false;
    let mut skip_char_name: Option<String> = None;
    if action == TurnAction::SkipUserTurn {
        if let Some(pid) = participant_id {
            let participants = chat
                .get("participants")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            turn_skipping_applies = crate::skip_signal::qualifies_for_turn_skipping(&participants);
            let skip_char = find_participant(&chat, pid)
                .and_then(|p| s(p, "characterId"))
                .and_then(|cid| {
                    read_main_mount(db, |main, mount| {
                        crate::db::characters_read::find_by_id(main, mount, &cid)
                    })
                    .ok()
                    .flatten()
                });
            if let Some(sc) = &skip_char {
                skip_char_name = s(sc, "name");
            }
            if turn_skipping_applies {
                if let Some(sc) = &skip_char {
                    let chat_id_owned = chat_id.to_string();
                    let events = match db.read_main(move |conn| {
                        crate::db::chats_messages_read::get_messages(conn, &chat_id_owned)
                    }) {
                        Ok(v) => v,
                        Err(e) => return internal(e),
                    };
                    let responding = crate::skip_signal::RespondingCharacter {
                        id: s(sc, "id").unwrap_or_default(),
                        name: s(sc, "name").unwrap_or_default(),
                        aliases: sc
                            .get("aliases")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    };
                    let turn_skipping_enabled =
                        chat.get("turnSkippingEnabled").and_then(Value::as_bool) != Some(false);
                    let eligibility = crate::skip_signal::compute_skip_eligibility(
                        &crate::skip_signal::ComputeSkipEligibilityOptions {
                            events: &events,
                            participants: &participants,
                            responding_participant_id: pid,
                            responding_character: &responding,
                            summoned: false,
                            turn_skipping_enabled,
                        },
                    );
                    if eligibility.must_speak_reason
                        == Some(crate::skip_signal::MustSpeakReason::AllOthersSkipped)
                    {
                        let name = skip_char_name
                            .clone()
                            .unwrap_or_else(|| "the player".to_string());
                        return bad_request(format!(
                            "Everyone else has passed — it falls to {name} to say something."
                        ));
                    }
                }
            }
        }
    }

    // The ported core: RMW + persist + selection.
    let result = match handle_turn_action(db, chat_id, action, participant_id, random01).await {
        Ok(r) => r,
        Err(e) => return internal(e),
    };

    // Host turn-pass announcement (skip only, when turn-skipping applies).
    if action == TurnAction::SkipUserTurn && turn_skipping_applies {
        if let Some(pid) = participant_id {
            use crate::services::host_notifications::{
                post_host_turn_pass_announcement, HostTurnPassAnnouncement, TurnPassSource,
            };
            let _ = post_host_turn_pass_announcement(
                db,
                HostTurnPassAnnouncement {
                    chat_id: chat_id.to_string(),
                    character_name: skip_char_name.unwrap_or_else(|| "The player".to_string()),
                    participant_id: pid.to_string(),
                    source: TurnPassSource::User,
                },
            )
            .await;
        }
    }

    // v4's response envelope.
    let mut resp = Map::new();
    resp.insert("success".into(), json!(true));
    resp.insert("action".into(), json!(action_str));
    resp.insert(
        "turn".into(),
        json!({
            "nextSpeakerId": result.next_speaker_id,
            "nextSpeakerName": result.next_speaker_name,
            "nextSpeakerControlledBy": result.next_speaker_controlled_by,
            "reason": result.reason,
            "explanation": result.explanation,
            "cycleComplete": result.cycle_complete,
            "isUsersTurn": result.is_users_turn,
        }),
    );
    resp.insert("state".into(), json!({ "queue": result.queue }));
    if let Some(pid) = participant_id {
        let name = read_main_mount(db, |main, mount| {
            Ok(find_participant(&chat, pid)
                .map(|p| character_name(main, mount, p))
                .unwrap_or_else(|| "Unknown".to_string()))
        })
        .unwrap_or_else(|_| "Unknown".to_string());
        resp.insert(
            "participant".into(),
            json!({
                "id": pid,
                "name": name,
                "queuePosition": result.queue_position,
            }),
        );
    }
    Response::TurnAction(Value::Object(resp))
}

// ===========================================================================
// Message mutations (v4 messages/[id]/route.ts)
// ===========================================================================

/// Resolve the owning chat for a message + the ownership gate (v4
/// `findMessageInUserChats`). Returns `(chat, messages, message, index)` or the
/// `null` collapse (→ `notFound('Message')`).
fn resolve_message(
    db: &Db,
    user_id: &str,
    message_id: &str,
) -> Result<Option<(Value, Vec<Value>, Value)>, DbError> {
    let mid = message_id.to_string();
    let chat_id = db.read_main(move |conn| {
        crate::db::chats_messages_read::find_chat_id_for_message(conn, &mid)
    })?;
    let Some(chat_id) = chat_id else {
        return Ok(None);
    };
    let cid = chat_id.clone();
    let chat = db.read_main(move |conn| chats_read::find_by_id(conn, &cid))?;
    let Some(chat) = chat else { return Ok(None) };
    if s(&chat, "userId").as_deref() != Some(user_id) {
        return Ok(None);
    }
    let cid = chat_id.clone();
    let messages =
        db.read_main(move |conn| crate::db::chats_messages_read::get_messages(conn, &cid))?;
    let message = messages.iter().find(|m| {
        m.get("type").and_then(Value::as_str) == Some("message")
            && m.get("id").and_then(Value::as_str) == Some(message_id)
    });
    match message {
        Some(m) => Ok(Some((chat.clone(), messages.clone(), m.clone()))),
        None => Ok(None),
    }
}

/// v4 message EDIT (PUT). `{content}` → updateMessage → touch → summary invalidate.
pub async fn message_edit(db: &Db, user_id: &str, message_id: &str, content: &str) -> Response {
    if content.is_empty() {
        return bad_request("Content is required");
    }
    let (chat, _messages, _message) = match resolve_message(db, user_id, message_id) {
        Ok(Some(v)) => v,
        Ok(None) => return not_found("Message"),
        Err(e) => return internal(e),
    };
    let chat_id = s(&chat, "id").unwrap_or_default();

    let updates = json!({ "content": content });
    let (cid, mid) = (chat_id.clone(), message_id.to_string());
    let updated = db
        .write(move |w| {
            w.main()
                .chat_messages()
                .update_message(&cid, &mid, &updates)
        })
        .await;
    match updated {
        Ok(true) => {}
        Ok(false) => return not_found("Message"),
        Err(e) => return internal(e),
    }

    // Touch chat (no updatedAt mint — v4's chats.update preserves it).
    let cid = chat_id.clone();
    if let Err(e) = db
        .write(move |w| {
            w.main()
                .chats()
                .update(&cid, &crate::db::chats::ChatUpdate::default())
        })
        .await
    {
        return internal(e);
    }
    crate::services::context_summary::invalidate_context_summary_if_message_covered(
        db,
        &chat_id,
        std::slice::from_ref(&message_id.to_string()),
    )
    .await;

    // Re-read the merged message (updateMessage returns bool in the port).
    let cid = chat_id.clone();
    let mid = message_id.to_string();
    let messages =
        match db.read_main(move |conn| crate::db::chats_messages_read::get_messages(conn, &cid)) {
            Ok(v) => v,
            Err(e) => return internal(e),
        };
    let merged = messages
        .into_iter()
        .find(|m| m.get("id").and_then(Value::as_str) == Some(mid.as_str()));
    match merged {
        Some(m) => Response::Message(json!({ "message": m })),
        None => not_found("Message"),
    }
}

/// v4 message DELETE — the memory-cascade confirmation protocol.
pub async fn message_delete(
    db: &Db,
    user_id: &str,
    message_id: &str,
    memory_action: Option<&str>,
    skip_confirmation: bool,
) -> Response {
    let (chat, messages, message) = match resolve_message(db, user_id, message_id) {
        Ok(Some(v)) => v,
        Ok(None) => return not_found("Message"),
        Err(e) => return internal(e),
    };
    let chat_id = s(&chat, "id").unwrap_or_default();
    let swipe_group_id = message.get("swipeGroupId").and_then(Value::as_str);

    // Collect message ids to delete (the whole swipe group when set).
    let ids_to_delete: Vec<String> = match swipe_group_id {
        Some(sg) => messages
            .iter()
            .filter(|m| {
                m.get("type").and_then(Value::as_str) == Some("message")
                    && m.get("swipeGroupId").and_then(Value::as_str) == Some(sg)
            })
            .filter_map(|m| s(m, "id"))
            .collect(),
        None => vec![message_id.to_string()],
    };

    // Count memories.
    let del_ids = ids_to_delete.clone();
    let memory_count = match db.read_main(move |conn| {
        crate::db::memories_read::count_by_source_message_ids(conn, &del_ids)
    }) {
        Ok(n) => n,
        Err(e) => return internal(e),
    };

    // Confirmation early return.
    if memory_count > 0 && memory_action.is_none() && !skip_confirmation {
        return Response::MessageDelete(json!({
            "requiresConfirmation": true,
            "memoryCount": memory_count,
            "messageIds": ids_to_delete,
            "isSwipeGroup": swipe_group_id.is_some(),
        }));
    }

    // Cascade.
    let mut memories_deleted: i64 = 0;
    if memory_count > 0 {
        if let Some(action) = memory_action {
            if action == "DELETE_MEMORIES" || action == "REGENERATE_MEMORIES" {
                match crate::services::memory_service::delete_memories_by_source_messages_with_vectors(
                    db,
                    &ids_to_delete,
                )
                .await
                {
                    Ok(r) => memories_deleted = r.deleted,
                    Err(e) => return internal(e),
                }
            }
        }
    }

    // Delete the messages, touch, invalidate.
    let (cid, del) = (chat_id.clone(), ids_to_delete.clone());
    if let Err(e) = db
        .write(move |w| {
            w.main()
                .chat_messages()
                .delete_messages_by_ids(&cid, &del)
                .map(|_| ())
        })
        .await
    {
        return internal(e);
    }
    let cid = chat_id.clone();
    if let Err(e) = db
        .write(move |w| {
            w.main()
                .chats()
                .update(&cid, &crate::db::chats::ChatUpdate::default())
        })
        .await
    {
        return internal(e);
    }
    crate::services::context_summary::invalidate_context_summary_if_message_covered(
        db,
        &chat_id,
        &ids_to_delete,
    )
    .await;

    Response::MessageDelete(json!({ "success": true, "memoriesDeleted": memories_deleted }))
}

/// v4 message SWIPE — the SWITCH branch (`swipeIndex` present). The GENERATE
/// branch (absent) needs the model driver and is a tracked deferral (returns a
/// typed error).
pub fn message_swipe_switch(
    db: &Db,
    user_id: &str,
    message_id: &str,
    swipe_index: i64,
) -> Response {
    let (_chat, messages, message) = match resolve_message(db, user_id, message_id) {
        Ok(Some(v)) => v,
        Ok(None) => return not_found("Message"),
        Err(e) => return internal(e),
    };
    let Some(sg) = message.get("swipeGroupId").and_then(Value::as_str) else {
        return bad_request("Message is not part of a swipe group");
    };
    let target = messages.iter().find(|m| {
        m.get("type").and_then(Value::as_str) == Some("message")
            && m.get("swipeGroupId").and_then(Value::as_str) == Some(sg)
            && m.get("swipeIndex").and_then(Value::as_i64) == Some(swipe_index)
    });
    match target {
        Some(m) => Response::Message(json!({ "message": m })),
        None => not_found("Swipe"),
    }
}

// ===========================================================================
// The chat PUT (v4 processChatUpdates, Salon-minimal) + impersonation verbs
// ===========================================================================

/// v4 `PUT /api/v1/chats/{id}` → `processChatUpdates` (the Salon pause/resume +
/// title path). Only the plain `chats.update` field family is ported here (the
/// fields with a `ChatUpdate` setter); the roster/participant/conciergeState
/// families are tracked deferrals (named in the report). The response is v4's
/// `{ chat: {...chat, participants: enrichParticipantDetail[]} }`.
pub async fn chat_update(db: &Db, chat_id: &str, chat_bag: &Value) -> Response {
    let chat_id_owned = chat_id.to_string();
    let existing = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };

    let patch = match build_chat_update(chat_bag) {
        Ok(p) => p,
        Err(msg) => return bad_request(msg),
    };
    let cid = chat_id.to_string();
    if let Err(e) = db
        .write(move |w| w.main().chats().update(&cid, &patch))
        .await
    {
        return internal(e);
    }

    // Re-read + enrich participants (detail).
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let _ = existing;
    let chat_id_s = chat_id.to_string();
    let out = read_main_mount(db, move |main, mount| {
        let participants = chat
            .get("participants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut enriched = Vec::with_capacity(participants.len());
        for p in &participants {
            enriched.push(chat_enrichment::enrich_participant_detail(
                main, mount, p, &chat_id_s,
            )?);
        }
        let mut obj = chat.as_object().cloned().unwrap_or_default();
        obj.insert(
            "participants".into(),
            serde_json::to_value(&enriched).map_err(|e| DbError::Key(e.to_string()))?,
        );
        Ok(Value::Object(obj))
    });
    match out {
        Ok(v) => Response::Chat(ChatWrapDto { chat: v }),
        Err(e) => internal(e),
    }
}

/// Build a `ChatUpdate` patch from the validated `chat` field bag — only the
/// fields with an existing setter (Salon-minimal). Unknown/unported field
/// families are ignored (tracked deferral); a value with the wrong type is a
/// bad-request.
fn build_chat_update(bag: &Value) -> Result<crate::db::chats::ChatUpdate, String> {
    use crate::db::chats::ChatUpdate;
    let mut patch = ChatUpdate::default();
    let Some(obj) = bag.as_object() else {
        return Ok(patch);
    };
    if let Some(v) = obj.get("title") {
        patch.title = Some(v.as_str().ok_or("Invalid title")?.to_string());
    }
    if let Some(v) = obj.get("isPaused") {
        patch.is_paused = Some(v.as_bool().ok_or("Invalid isPaused")?);
    }
    if let Some(v) = obj.get("isManuallyRenamed") {
        patch.is_manually_renamed = Some(v.as_bool().ok_or("Invalid isManuallyRenamed")?);
    }
    if let Some(v) = obj.get("contextSummary") {
        patch.context_summary = Some(v.as_str().map(String::from));
    }
    if let Some(v) = obj.get("turnSkippingEnabled") {
        patch.turn_skipping_enabled = Some(if v.is_null() {
            None
        } else {
            Some(v.as_bool().ok_or("Invalid turnSkippingEnabled")?)
        });
    }
    if let Some(v) = obj.get("documentMode") {
        patch.document_mode = Some(v.as_str().ok_or("Invalid documentMode")?.to_string());
    }
    Ok(patch)
}

/// v4 `?action=impersonate` (`handleImpersonate`).
pub async fn chat_impersonate(db: &Db, chat_id: &str, participant_id: &str) -> Response {
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let Some(participant) = find_participant(&chat, participant_id).cloned() else {
        return not_found("Participant");
    };
    if !participant_present(participant.get("status").and_then(Value::as_str)) {
        return bad_request("Participant is not active or silent");
    }
    let (cid, pid) = (chat_id.to_string(), participant_id.to_string());
    let ok = match db
        .write(move |w| {
            crate::db::chats_impersonation::ChatImpersonationRepository::new(w.main().connection())
                .add_impersonation(&cid, &pid)
        })
        .await
    {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    if !ok {
        return internal("Failed to start impersonation");
    }
    // Re-read to source impersonatingParticipantIds/activeTypingParticipantId.
    let chat_id_owned = chat_id.to_string();
    let updated = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(v) => v.unwrap_or(Value::Null),
        Err(e) => return internal(e),
    };
    let name = read_main_mount(db, |main, mount| {
        Ok(character_name(main, mount, &participant))
    })
    .unwrap_or_else(|_| "Unknown".to_string());
    Response::ChatImpersonation(json!({
        "success": true,
        "participantId": participant_id,
        "characterName": name,
        "impersonatingParticipantIds": updated.get("impersonatingParticipantIds").cloned().unwrap_or(json!([])),
        "activeTypingParticipantId": updated.get("activeTypingParticipantId").cloned().unwrap_or(Value::Null),
    }))
}

/// v4 `?action=stop-impersonate` (`handleStopImpersonate`).
pub async fn chat_stop_impersonate(
    db: &Db,
    chat_id: &str,
    participant_id: &str,
    new_connection_profile_id: Option<&str>,
) -> Response {
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let Some(participant) = find_participant(&chat, participant_id).cloned() else {
        return not_found("Participant");
    };
    let (cid, pid) = (chat_id.to_string(), participant_id.to_string());
    let ok = match db
        .write(move |w| {
            crate::db::chats_impersonation::ChatImpersonationRepository::new(w.main().connection())
                .remove_impersonation(&cid, &pid)
        })
        .await
    {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    if !ok {
        return internal("Failed to stop impersonation");
    }

    // Optional new connection profile → flip controlledBy:'llm'.
    if let Some(new_cp) = new_connection_profile_id {
        let ncp = new_cp.to_string();
        let exists = match db
            .read_main(move |conn| crate::db::connection_profiles::find_by_id(conn, &ncp))
        {
            Ok(v) => v.is_some(),
            Err(e) => return internal(e),
        };
        if !exists {
            return not_found("Connection profile");
        }
        let (cid, pid, ncp) = (
            chat_id.to_string(),
            participant_id.to_string(),
            new_cp.to_string(),
        );
        if let Err(e) = db
            .write(move |w| {
                crate::db::chats_participants::ChatParticipantsRepository::new(
                    w.main().connection(),
                )
                .update_participant(
                    &cid,
                    &pid,
                    &json!({ "connectionProfileId": ncp, "controlledBy": "llm" }),
                )
            })
            .await
        {
            return internal(e);
        }
    }

    let chat_id_owned = chat_id.to_string();
    let updated = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(v) => v.unwrap_or(Value::Null),
        Err(e) => return internal(e),
    };
    let name = read_main_mount(db, |main, mount| {
        Ok(character_name(main, mount, &participant))
    })
    .unwrap_or_else(|_| "Unknown".to_string());
    Response::ChatImpersonation(json!({
        "success": true,
        "participantId": participant_id,
        "characterName": name,
        "impersonatingParticipantIds": updated.get("impersonatingParticipantIds").cloned().unwrap_or(json!([])),
        "activeTypingParticipantId": updated.get("activeTypingParticipantId").cloned().unwrap_or(Value::Null),
        "newConnectionProfileId": new_connection_profile_id,
    }))
}

/// v4 `?action=set-active-speaker` (`handleSetActiveSpeaker`).
pub async fn chat_set_active_speaker(db: &Db, chat_id: &str, participant_id: &str) -> Response {
    let chat_id_owned = chat_id.to_string();
    let chat = match db.read_main(move |conn| chats_read::find_by_id(conn, &chat_id_owned)) {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("Chat"),
        Err(e) => return internal(e),
    };
    let Some(participant) = find_participant(&chat, participant_id).cloned() else {
        return not_found("Participant");
    };
    let mut impersonating: Vec<String> = chat
        .get("impersonatingParticipantIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if !impersonating.iter().any(|id| id == participant_id) {
        if participant.get("controlledBy").and_then(Value::as_str) == Some("user") {
            impersonating.push(participant_id.to_string());
            let (cid, imp) = (chat_id.to_string(), impersonating.clone());
            if let Err(e) = db
                .write(move |w| {
                    w.main().chats().update(
                        &cid,
                        &crate::db::chats::ChatUpdate {
                            impersonating_participant_ids: Some(imp),
                            ..Default::default()
                        },
                    )
                })
                .await
            {
                return internal(e);
            }
        } else {
            return bad_request("Participant is not being impersonated");
        }
    }
    let (cid, pid) = (chat_id.to_string(), participant_id.to_string());
    let ok = match db
        .write(move |w| {
            crate::db::chats_impersonation::ChatImpersonationRepository::new(w.main().connection())
                .set_active_typing_participant(&cid, Some(&pid))
        })
        .await
    {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    if !ok {
        return internal("Failed to set active speaker");
    }
    let name = read_main_mount(db, |main, mount| {
        Ok(character_name(main, mount, &participant))
    })
    .unwrap_or_else(|_| "Unknown".to_string());
    Response::ChatImpersonation(json!({
        "success": true,
        "activeTypingParticipantId": participant_id,
        "characterName": name,
        "impersonatingParticipantIds": impersonating,
    }))
}
