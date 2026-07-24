//! Courier transport service — the manual / clipboard dispatch path (port of v4
//! `lib/services/chat-message/courier-transport.service.ts` + the paste/cancel
//! resolvers in `app/api/v1/chats/[id]/messages/[messageId]/route.ts`).
//!
//! When a chat's effective connection profile uses the `courier` transport, the
//! orchestrator does NOT call an LLM. Instead the assembled request is rendered
//! as Markdown ([`crate::courier::render_markdown`]), persisted on a placeholder
//! ASSISTANT message in `pendingExternalTurn` state, the chat is paused, and SSE
//! events are emitted so the Salon can show the Courier bubble. The operator
//! pastes the reply back through [`resolve_external_turn`], which clears the pause
//! and advances the per-character delta checkpoint. [`cancel_external_turn`] drops
//! the placeholder and unpauses without chaining.
//!
//! No model calls, no tools, no host I/O — repos + string rendering only.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::courier::render_markdown::{
    render_courier_delta_as_markdown, render_courier_request_as_markdown,
    CourierAttachmentDescriptor, CourierCheckpoint, CourierDeltaEvent, CourierMessage,
    CourierMessageAttachment,
};
use crate::db::runtime::Db;
use crate::db::{chats, chats_messages_read, chats_read, connection_profiles, DbError};
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::chat_events::{ChatEvent, DonePayload, EventSink, PendingExternalTurnPayload};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::message_context::FormattedMsg;
use crate::services::message_finalizer::ProcessMessageResult;

/// v4 `[Staff: ${COURIER_SYSTEM_SENDER_LABELS[sender] ?? sender}]` — every
/// `systemSender` speaker is wrapped, with the known senders relabeled.
fn courier_system_sender_label(sender: &str) -> String {
    let label = match sender {
        "lantern" => "The Lantern",
        "aurora" => "Aurora",
        "librarian" => "The Librarian",
        "concierge" => "The Concierge",
        "prospero" => "Prospero",
        "host" => "The Host",
        "commonplaceBook" => "The Commonplace Book",
        "ariel" => "Ariel",
        other => other,
    };
    format!("[Staff: {label}]")
}

/// Everything [`dispatch_courier_transport`] needs (v4
/// `DispatchCourierTransportOptions` minus the SSE controller/encoder — the
/// `sink` carries the frames).
pub struct DispatchCourierOptions {
    pub chat_id: String,
    /// The hydrated chat (for `courierCheckpoints` + `participants`).
    pub chat: Value,
    /// The responding character (`{ id, name }`).
    pub character: Value,
    /// The responding participant (`{ id, status? }`).
    pub character_participant: Value,
    pub user_participant_id: Option<String>,
    pub is_multi_character: bool,
    /// characterId → character (the bulk-loaded participant characters).
    pub participant_characters: HashMap<String, Value>,
    /// v4 `resolvedIdentity.name` — the user speaker label in the delta bundle.
    pub resolved_identity_name: String,
    /// The provider-shaped outgoing messages (the ported `formattedMessages`).
    pub formatted_messages: Vec<FormattedMsg>,
    /// `effectiveProfile.provider` (stored on the placeholder + the done frame).
    pub effective_provider: Option<String>,
    /// `effectiveProfile.modelName` (the placeholder + the informational label).
    pub effective_model_name: Option<String>,
    /// `effectiveProfile.courierDeltaMode !== false` — delta mode when a checkpoint
    /// also exists.
    pub courier_delta_mode: bool,
    /// The injected wall clock (v4 `new Date().toISOString()` → shared by the
    /// placeholder `createdAt` and the chat `updatedAt`).
    pub now_ms: i64,
}

/// Convert a ported `FormattedMsg` into the courier renderer's message shape. The
/// attachment Values carry the v4 `LLMMessage['attachments']` shape
/// (`{ id, filename?, mimeType?, size? }`).
fn to_courier_message(m: &FormattedMsg) -> CourierMessage {
    let attachments = m
        .attachments
        .as_ref()
        .map(|atts| {
            atts.iter()
                .filter_map(|a| {
                    let id = a.get("id").and_then(Value::as_str)?.to_string();
                    Some(CourierMessageAttachment {
                        id,
                        filename: a
                            .get("filename")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        mime_type: a
                            .get("mimeType")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        size: a.get("size").and_then(Value::as_f64),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    CourierMessage {
        role: m.role.clone(),
        content: m.content.clone(),
        name: m.name.clone(),
        attachments,
    }
}

/// The descriptor as the stored `pendingExternalAttachments` JSON element
/// (`{ fileId, filename, mimeType, sizeBytes, downloadUrl }`).
fn descriptor_to_json(a: &CourierAttachmentDescriptor) -> Value {
    json!({
        "fileId": a.file_id,
        "filename": a.filename,
        "mimeType": a.mime_type,
        "sizeBytes": crate::db::js_number_to_json(a.size_bytes),
        "downloadUrl": a.download_url,
    })
}

/// v4 `dispatchCourierTransport`. Renders the assembled request as Markdown,
/// persists a placeholder assistant message, pauses the chat, and emits the
/// `pendingExternalTurn` + `done` SSE frames. Returns the standard
/// [`ProcessMessageResult`] with `is_paused = true`.
pub async fn dispatch_courier_transport<S: EventSink>(
    db: &Db,
    sink: &S,
    opts: DispatchCourierOptions,
) -> Result<ProcessMessageResult, DbError> {
    let character_id = opts
        .character
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let character_name = opts
        .character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let character_participant_id = opts
        .character_participant
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let model_label = opts
        .effective_model_name
        .as_deref()
        .filter(|s| !s.is_empty());

    // Always render the full bundle.
    let courier_messages: Vec<CourierMessage> = opts
        .formatted_messages
        .iter()
        .map(to_courier_message)
        .collect();
    let full = render_courier_request_as_markdown(&courier_messages, &character_name, model_label);

    // Delta bundle when the profile wants it AND this character already has a
    // resolved checkpoint in this chat.
    let mut delta_markdown: Option<String> = None;
    let mut delta_attachments: Vec<CourierAttachmentDescriptor> = Vec::new();
    if opts.courier_delta_mode {
        let checkpoint = opts
            .chat
            .get("courierCheckpoints")
            .and_then(Value::as_object)
            .and_then(|m| m.get(&character_id))
            .and_then(|cp| {
                let last = cp.get("lastResolvedMessageId").and_then(Value::as_str)?;
                let resolved = cp.get("resolvedAt").and_then(Value::as_str)?;
                Some(CourierCheckpoint {
                    last_resolved_message_id: last.to_string(),
                    resolved_at: resolved.to_string(),
                })
            });
        if let Some(checkpoint) = checkpoint {
            let chat_id = opts.chat_id.clone();
            let chat = opts.chat.clone();
            let participant_characters = opts.participant_characters.clone();
            let responding_character = opts.character.clone();
            let responding_participant_id = character_participant_id.clone();
            let resolved_identity_name = opts.resolved_identity_name.clone();
            let events = db.read_main(move |conn| {
                build_courier_delta_events(
                    conn,
                    &chat_id,
                    &checkpoint,
                    &chat,
                    &participant_characters,
                    &responding_character,
                    &responding_participant_id,
                    &resolved_identity_name,
                )
            })?;
            let rendered = render_courier_delta_as_markdown(&events, &character_name, model_label);
            delta_markdown = Some(rendered.markdown);
            delta_attachments = rendered.attachments;
        }
    }

    // Primary = delta when applicable, else full; full kept as a fallback only when
    // delta is primary. Attachments surface the union across both bundles.
    let full_fallback = if delta_markdown.is_some() {
        Some(full.markdown.clone())
    } else {
        None
    };
    let primary_markdown = delta_markdown.unwrap_or_else(|| full.markdown.clone());
    let mut union_attachments: Vec<CourierAttachmentDescriptor> = full.attachments.clone();
    for a in delta_attachments {
        if !union_attachments.iter().any(|x| x.file_id == a.file_id) {
            union_attachments.push(a);
        }
    }

    let placeholder_id = uuid::Uuid::new_v4().to_string();
    let now_iso = crate::clock::iso_from_unix_ms(opts.now_ms);

    let placeholder = json!({
        "id": placeholder_id,
        "type": "message",
        "role": "ASSISTANT",
        "content": "",
        "createdAt": now_iso,
        "attachments": [],
        "participantId": character_participant_id,
        "provider": opts.effective_provider.clone(),
        "modelName": opts.effective_model_name.clone(),
        "pendingExternalPrompt": primary_markdown,
        "pendingExternalPromptFull": full_fallback,
        "pendingExternalAttachments": Value::Array(
            union_attachments.iter().map(descriptor_to_json).collect(),
        ),
    });
    let event: crate::db::chats_messages::ChatEventInput = serde_json::from_value(placeholder)
        .map_err(|e| DbError::Key(format!("courier placeholder parse: {e}")))?;

    let write_chat_id = opts.chat_id.clone();
    let now_iso_write = now_iso.clone();
    db.write(move |w| {
        w.main()
            .chat_messages()
            .add_message(&write_chat_id, &event)?;
        w.main().chats().update(
            &write_chat_id,
            &chats::ChatUpdate {
                is_paused: Some(true),
                updated_at: Some(now_iso_write),
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .await?;

    // SSE frames: the `pendingExternalTurn` frame FIRST, then `done` with
    // `pendingExternalTurn: true`.
    sink.emit(ChatEvent::pending_external_turn(
        PendingExternalTurnPayload {
            message_id: placeholder_id.clone(),
            participant_id: character_participant_id.clone(),
            character_name: character_name.clone(),
        },
    ));
    sink.emit(ChatEvent::done(DonePayload {
        message_id: Some(placeholder_id.clone()),
        participant_id: Some(character_participant_id),
        usage: None,
        cache_usage: None,
        attachment_results: None,
        tools_executed: false,
        provider: opts.effective_provider,
        model_name: opts.effective_model_name,
        pending_external_turn: Some(true),
        ..Default::default()
    }));

    Ok(ProcessMessageResult {
        is_multi_character: opts.is_multi_character,
        has_content: false,
        message_id: placeholder_id,
        user_participant_id: opts.user_participant_id,
        is_paused: true,
        scene_tracking_character_ids: None,
        skipped: false,
        skipped_participant_id: None,
    })
}

/// v4 `buildCourierDeltaEvents`. Walks the chat's persisted message events from
/// after the checkpoint, resolves each speaker, loads attachment descriptors, and
/// returns a chronological delta list. Runs entirely on a read-only connection.
#[allow(clippy::too_many_arguments)]
fn build_courier_delta_events(
    conn: &Connection,
    chat_id: &str,
    checkpoint: &CourierCheckpoint,
    chat: &Value,
    participant_characters: &HashMap<String, Value>,
    responding_character: &Value,
    responding_participant_id: &str,
    resolved_identity_name: &str,
) -> Result<Vec<CourierDeltaEvent>, DbError> {
    let fresh_messages = chats_messages_read::get_messages(conn, chat_id)?;
    let responding_character_id = responding_character
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // participantId → character. From the chat participants + the bulk-loaded map,
    // plus a guaranteed entry for the responding character (single-char chats skip
    // the bulk loader).
    let mut participant_to_character: HashMap<String, &Value> = HashMap::new();
    if let Some(participants) = chat.get("participants").and_then(Value::as_array) {
        for p in participants {
            let Some(pid) = p.get("id").and_then(Value::as_str) else {
                continue;
            };
            if let Some(cid) = p
                .get("characterId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                if cid == responding_character_id {
                    participant_to_character.insert(pid.to_string(), responding_character);
                } else if let Some(c) = participant_characters.get(cid) {
                    participant_to_character.insert(pid.to_string(), c);
                }
            }
        }
    }

    let resolve_speaker = |event: &Value| -> String {
        if let Some(sender) = event
            .get("systemSender")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            return courier_system_sender_label(sender);
        }
        if let Some(ann) = event.get("customAnnouncer").and_then(Value::as_object) {
            let kind = ann.get("kind").and_then(Value::as_str).unwrap_or("");
            if kind == "character"
                && ann
                    .get("characterId")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
            {
                return ann
                    .get("displayName")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Off-scene character")
                    .to_string();
            }
            if kind == "custom" {
                if let Some(name) = ann
                    .get("displayName")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    return name.to_string();
                }
            }
        }
        let role = event.get("role").and_then(Value::as_str);
        match role {
            Some("USER") => {
                if let Some(pid) = event.get("participantId").and_then(Value::as_str) {
                    if let Some(c) = participant_to_character.get(pid) {
                        if let Some(name) = c.get("name").and_then(Value::as_str) {
                            return name.to_string();
                        }
                    }
                }
                if resolved_identity_name.is_empty() {
                    "User".to_string()
                } else {
                    resolved_identity_name.to_string()
                }
            }
            Some("TOOL") => "Tool result".to_string(),
            Some("ASSISTANT") => {
                if let Some(pid) = event.get("participantId").and_then(Value::as_str) {
                    if let Some(c) = participant_to_character.get(pid) {
                        if let Some(name) = c.get("name").and_then(Value::as_str) {
                            return name.to_string();
                        }
                    }
                }
                "Assistant".to_string()
            }
            Some(other) => other.to_string(),
            None => "Event".to_string(),
        }
    };

    let mut delta_events: Vec<CourierDeltaEvent> = Vec::new();
    for event in &fresh_messages {
        if event.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let created_at = event.get("createdAt").and_then(Value::as_str).unwrap_or("");
        // Skip anything at-or-before the checkpoint (equality matters).
        if created_at <= checkpoint.resolved_at.as_str() {
            continue;
        }
        // Skip the resolved Courier turn itself (defensive).
        if event.get("id").and_then(Value::as_str)
            == Some(checkpoint.last_resolved_message_id.as_str())
        {
            continue;
        }
        // Filter targeted whispers: surface only when the responding character is
        // sender OR a target.
        if let Some(targets) = event.get("targetParticipantIds").and_then(Value::as_array) {
            if !targets.is_empty() {
                let is_sender = event.get("participantId").and_then(Value::as_str)
                    == Some(responding_participant_id);
                let is_target = targets
                    .iter()
                    .any(|t| t.as_str() == Some(responding_participant_id));
                if !is_sender && !is_target {
                    continue;
                }
            }
        }

        // Attachment descriptors (missing files skipped).
        let mut attachments: Vec<CourierAttachmentDescriptor> = Vec::new();
        if let Some(ids) = event.get("attachments").and_then(Value::as_array) {
            for id_val in ids {
                let Some(file_id) = id_val.as_str() else {
                    continue;
                };
                if let Some(desc) = load_attachment_descriptor(conn, file_id)? {
                    attachments.push(desc);
                }
            }
        }

        delta_events.push(CourierDeltaEvent {
            speaker: resolve_speaker(event),
            created_at: created_at.to_string(),
            content: event
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            attachments: if attachments.is_empty() {
                None
            } else {
                Some(attachments)
            },
        });
    }

    Ok(delta_events)
}

/// Load a courier attachment descriptor for a file id (v4 `repos.files.findById`
/// + `descriptorFromAttachment`), or `None` if the file is gone.
fn load_attachment_descriptor(
    conn: &Connection,
    file_id: &str,
) -> Result<Option<CourierAttachmentDescriptor>, DbError> {
    let row = conn
        .query_row(
            "SELECT id, originalFilename, mimeType, size FROM files WHERE id = ?1",
            rusqlite::params![file_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row.map(
        |(id, filename, mime_type, size)| CourierAttachmentDescriptor {
            file_id: id.clone(),
            filename,
            mime_type,
            size_bytes: size,
            download_url: format!("/api/v1/files/{id}"),
        },
    ))
}

// ===========================================================================
// Paste / cancel resolvers
// ===========================================================================

/// The outcome of [`resolve_external_turn`] (the fields the differential compares;
/// v4's route returns HTTP responses, but the transport-agnostic service surfaces
/// the shape).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveExternalTurnOutcome {
    /// v4 `notFound('Chat')`.
    ChatNotFound,
    /// v4 `notFound('Message')`.
    MessageNotFound,
    /// v4 `badRequest('Message is not awaiting an external reply')`.
    NotAwaiting,
    /// v4 `badRequest('Only assistant placeholder messages can be resolved')`.
    NotAssistant,
    /// v4 `successResponse({ resolved: true, messageId, participantId })`.
    Resolved {
        message_id: String,
        participant_id: Option<String>,
    },
}

/// v4 `handleResolveExternalTurn`. Attaches the pasted reply, clears the pending
/// fields, unpauses + advances the checkpoint, then fires the three post-turn
/// triggers (memory extraction / danger classification / summary check). Awaited
/// per the watermark precedent (v4 fire-and-forgets; same DB effect once settled).
#[allow(clippy::too_many_arguments)]
pub async fn resolve_external_turn<C: CompletionProvider + Sync, E: EmbeddingProvider + Sync>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    chat_id: &str,
    message_id: &str,
    reply_content: &str,
    user_id: &str,
    now_ms: i64,
) -> Result<ResolveExternalTurnOutcome, DbError> {
    // --- read chat + message ---
    let chat_id_owned = chat_id.to_string();
    let Some(chat) = db.read_main(move |c| chats_read::find_by_id(c, &chat_id_owned))? else {
        return Ok(ResolveExternalTurnOutcome::ChatNotFound);
    };
    let chat_id_owned = chat_id.to_string();
    let messages = db.read_main(move |c| chats_messages_read::get_messages(c, &chat_id_owned))?;
    let Some(message) = messages
        .iter()
        .find(|m| m.get("id").and_then(Value::as_str) == Some(message_id))
    else {
        return Ok(ResolveExternalTurnOutcome::MessageNotFound);
    };
    if message.get("type").and_then(Value::as_str) != Some("message") {
        return Ok(ResolveExternalTurnOutcome::MessageNotFound);
    }
    if message
        .get("pendingExternalPrompt")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Ok(ResolveExternalTurnOutcome::NotAwaiting);
    }
    if message.get("role").and_then(Value::as_str) != Some("ASSISTANT") {
        return Ok(ResolveExternalTurnOutcome::NotAssistant);
    }

    let now_iso = crate::clock::iso_from_unix_ms(now_ms);
    let participant_id = message
        .get("participantId")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Character id for the checkpoint (v4 finds the participant on the chat).
    let character_id_for_checkpoint = participant_id.as_deref().and_then(|pid| {
        chat.get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| {
                ps.iter()
                    .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
            })
            .and_then(|p| p.get("characterId").and_then(Value::as_str))
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });

    // courierCheckpoints = { ...existing, [cid]: { lastResolvedMessageId, resolvedAt } }.
    let courier_checkpoints: Option<Value> = character_id_for_checkpoint.as_ref().map(|cid| {
        let mut map: Map<String, Value> = chat
            .get("courierCheckpoints")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        map.insert(
            cid.clone(),
            json!({ "lastResolvedMessageId": message_id, "resolvedAt": now_iso }),
        );
        Value::Object(map)
    });

    // --- writes: attach the reply + unpause + advance checkpoint ---
    let updates = json!({
        "content": reply_content,
        "pendingExternalPrompt": Value::Null,
        "pendingExternalPromptFull": Value::Null,
        "pendingExternalAttachments": Value::Null,
    });
    let write_chat_id = chat_id.to_string();
    let write_message_id = message_id.to_string();
    let now_iso_write = now_iso.clone();
    db.write(move |w| {
        w.main()
            .chat_messages()
            .update_message(&write_chat_id, &write_message_id, &updates)?;
        w.main().chats().update(
            &write_chat_id,
            &chats::ChatUpdate {
                is_paused: Some(false),
                last_message_at: Some(Some(now_iso_write.clone())),
                updated_at: Some(now_iso_write),
                courier_checkpoints,
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .await?;

    // --- resolve the connection profile for the triggers ---
    let connection_profile =
        resolve_trigger_connection_profile(db, &chat, participant_id.as_deref())?;

    if let Some(profile) = connection_profile {
        let profile_id = profile
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        // chatSettings gate for memory extraction + summary check.
        let user_id_owned = user_id.to_string();
        let chat_settings =
            db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &user_id_owned))?;
        let cheap_llm_present = chat_settings
            .as_ref()
            .and_then(|s| s.get("cheapLLMSettings"))
            .is_some_and(|v| !v.is_null());

        // Memory extraction — gated on cheapLLMSettings (v4
        // `triggerTurnMemoryExtraction`'s first line).
        if cheap_llm_present {
            crate::services::message_finalizer::trigger_turn_memory_extraction(
                db,
                chat_id,
                user_id,
                &profile_id,
            )
            .await?;
        }

        // Danger classification — resolve mode OFF from the global settings + chat.
        let danger_mode_off = {
            let global = chat_settings
                .as_ref()
                .and_then(|s| s.get("dangerousContentSettings").cloned())
                .and_then(|v| {
                    serde_json::from_value::<crate::db::chat_settings::DangerousContentSettings>(v)
                        .ok()
                });
            let resolved =
                crate::services::dangerous_content::resolver::resolve_dangerous_content_settings(
                    global,
                    Some(&chat),
                );
            resolved.settings.mode == "OFF"
        };
        crate::services::message_finalizer::trigger_chat_danger_classification(
            db,
            chat_id,
            user_id,
            &profile_id,
            danger_mode_off,
        )
        .await?;

        // Context-summary check — gated on cheapLLMSettings. Below cadence in the
        // corpus → a no-op (no fold, no title enqueue). The REAL stored settings +
        // the whole chat are threaded so the cheap-LLM selection honours the user's
        // configured cheap profile (finding #27).
        if cheap_llm_present {
            run_summary_check(
                db,
                completion,
                embedding,
                executor,
                chat_id,
                user_id,
                &profile,
                chat_settings.as_ref(),
                &chat,
            )
            .await?;
        }
    }

    Ok(ResolveExternalTurnOutcome::Resolved {
        message_id: message_id.to_string(),
        participant_id,
    })
}

/// Resolve the connection profile the paste-resolver threads through the triggers
/// (v4: participant → character → `connectionProfileId || defaultConnectionProfileId`
/// → `repos.connections.findById`). `None` when nothing resolves — the triggers
/// are then skipped (v4 `if (connectionProfile)`).
fn resolve_trigger_connection_profile(
    db: &Db,
    chat: &Value,
    participant_id: Option<&str>,
) -> Result<Option<Value>, DbError> {
    let Some(pid) = participant_id else {
        return Ok(None);
    };
    let Some(participant) = chat
        .get("participants")
        .and_then(Value::as_array)
        .and_then(|ps| {
            ps.iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
        })
    else {
        return Ok(None);
    };
    let Some(character_id) = participant
        .get("characterId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    let character_id = character_id.to_string();
    let character =
        db.read_main(move |c| crate::db::characters_read::find_by_id_raw(c, &character_id))?;
    let profile_id = participant
        .get("connectionProfileId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            character
                .as_ref()
                .and_then(|c| c.get("defaultConnectionProfileId").and_then(Value::as_str))
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        });
    let Some(profile_id) = profile_id else {
        return Ok(None);
    };
    db.read_main(move |c| connection_profiles::find_by_id(c, &profile_id))
}

/// The paste-resolver's context-summary check (v4 `triggerContextSummaryCheck` →
/// `checkAndGenerateSummaryIfNeeded`). Constructs a cheap profile/settings from the
/// resolved connection profile.
///
/// The fold-time episode pass runs LIVE here via
/// [`crate::services::context_summary::FoldEpisodePassSeams`] — v4's
/// `checkAndGenerateSummaryIfNeeded` → `generateContextSummary` calls the real
/// `runFoldEpisodePass` on this path too (the courier differential's at-cadence
/// resolve case pins the fold's + the episode pass's cheap-LLM calls); the
/// cross-subsystem arms stay no-ops per the orchestrator-oracle mock set the
/// courier oracle reuses (the same tracked deferral as the orchestrator's
/// in-loop check).
/// Parse the stored `cheapLLMSettings` sub-object into the summary service's
/// [`context_summary::CheapLlmSettings`], field-by-field (v4 passes
/// `chatSettings.cheapLLMSettings` straight through). The stored object always
/// carries the Zod-defaulted sub-keys, so the per-field defaults here fire only
/// when the object is absent entirely; `PROVIDER_CHEAPEST` is the real
/// `CheapLLMStrategyEnum` default (the enum has no "AUTO").
fn cheap_llm_settings_from_stored(
    settings: Option<&Value>,
) -> crate::services::context_summary::CheapLlmSettings {
    let cheap = settings.and_then(|s| s.get("cheapLLMSettings"));
    crate::services::context_summary::CheapLlmSettings {
        strategy: cheap
            .and_then(|c| c.get("strategy"))
            .and_then(Value::as_str)
            .unwrap_or("PROVIDER_CHEAPEST")
            .to_string(),
        user_defined_profile_id: cheap
            .and_then(|c| c.get("userDefinedProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        default_cheap_profile_id: cheap
            .and_then(|c| c.get("defaultCheapProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        fallback_to_local: cheap
            .and_then(|c| c.get("fallbackToLocal"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_summary_check<C: CompletionProvider + Sync, E: EmbeddingProvider + Sync>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    chat_id: &str,
    user_id: &str,
    profile: &Value,
    settings: Option<&Value>,
    chat: &Value,
) -> Result<(), DbError> {
    // The `current` connection profile is the responder's (v4 passes
    // `connectionProfile` as the selection's basis).
    let cheap_profile = crate::services::orchestrator::cheap_llm_profile_from_value(profile);

    // finding #27: v4's route path (`triggerContextSummaryCheck` →
    // `checkAndGenerateSummaryIfNeeded`, `memory-trigger.service.ts:104-135`)
    // hands the REAL stored `cheapLLMSettings` + ALL the user's connection profiles
    // (`repos.connections.findByUserId`), so a configured `defaultCheapProfileId` /
    // `USER_DEFINED` / `isCheap` profile wins — not a hard-coded config over the
    // responder profile alone.
    let cheap_settings = cheap_llm_settings_from_stored(settings);

    let uid = user_id.to_string();
    let available = db.read_main(move |conn| connection_profiles::find_by_user_id(conn, &uid))?;
    let available_profiles: Vec<_> = available
        .iter()
        .map(crate::services::orchestrator::cheap_llm_profile_from_value)
        .collect();

    // v4's `generateContextSummary` resolves the user's danger settings internally
    // (only consulted on an active-dangerous chat); the ported service takes them
    // injected. Mirror the enclave step (`enclave/step.rs`): resolve from the global
    // sub-object + the chat, so a dangerous room does the uncensored swap on both
    // sides. A non-dangerous chat resolves to mode OFF (no swap).
    let global_danger: Option<crate::db::chat_settings::DangerousContentSettings> = settings
        .and_then(|s| s.get("dangerousContentSettings"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let resolved = crate::services::dangerous_content::resolver::resolve_dangerous_content_settings(
        global_danger,
        Some(chat),
    );
    let danger = crate::cheap_llm::DangerousContentSettings {
        mode: resolved.settings.mode.clone(),
        uncensored_text_profile_id: resolved.settings.uncensored_text_profile_id.clone(),
    };

    // The seams carry the same executor as the fold itself (the route path has
    // no autonomous-run scope, so both stay UNtagged).
    let seams = crate::services::context_summary::FoldEpisodePassSeams {
        db,
        embedding,
        completion,
        executor,
    };
    crate::services::context_summary::check_and_generate_summary_if_needed_with_seams(
        db,
        completion,
        executor,
        chat_id,
        &cheap_profile,
        &cheap_settings,
        &available_profiles,
        user_id,
        Some(&danger),
        None,
        true,
        &seams,
    )
    .await?;
    Ok(())
}

/// The outcome of [`cancel_external_turn`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CancelExternalTurnOutcome {
    ChatNotFound,
    MessageNotFound,
    NotAwaiting,
    Cancelled { message_id: String },
}

/// v4 `handleCancelExternalTurn`. Deletes the placeholder message and unpauses the
/// chat. Does not chain a next turn.
pub async fn cancel_external_turn(
    db: &Db,
    chat_id: &str,
    message_id: &str,
    now_ms: i64,
) -> Result<CancelExternalTurnOutcome, DbError> {
    let chat_id_owned = chat_id.to_string();
    let Some(_chat) = db.read_main(move |c| chats_read::find_by_id(c, &chat_id_owned))? else {
        return Ok(CancelExternalTurnOutcome::ChatNotFound);
    };
    let chat_id_owned = chat_id.to_string();
    let messages = db.read_main(move |c| chats_messages_read::get_messages(c, &chat_id_owned))?;
    let Some(message) = messages
        .iter()
        .find(|m| m.get("id").and_then(Value::as_str) == Some(message_id))
    else {
        return Ok(CancelExternalTurnOutcome::MessageNotFound);
    };
    if message.get("type").and_then(Value::as_str) != Some("message") {
        return Ok(CancelExternalTurnOutcome::MessageNotFound);
    }
    if message
        .get("pendingExternalPrompt")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Ok(CancelExternalTurnOutcome::NotAwaiting);
    }

    let now_iso = crate::clock::iso_from_unix_ms(now_ms);
    let write_chat_id = chat_id.to_string();
    let doomed = vec![message_id.to_string()];
    db.write(move |w| {
        w.main()
            .chat_messages()
            .delete_messages_by_ids(&write_chat_id, &doomed)?;
        w.main().chats().update(
            &write_chat_id,
            &chats::ChatUpdate {
                is_paused: Some(false),
                updated_at: Some(now_iso),
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .await?;

    Ok(CancelExternalTurnOutcome::Cancelled {
        message_id: message_id.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// finding #27: a stored `cheapLLMSettings` with a configured
    /// `defaultCheapProfileId` (and `USER_DEFINED` strategy) is parsed straight
    /// through — the field-by-field parse the courier resolver now feeds the
    /// cheap-LLM selection, so a configured profile wins over the responder's.
    #[test]
    fn cheap_settings_carry_the_configured_default_profile() {
        let settings = json!({
            "cheapLLMSettings": {
                "strategy": "USER_DEFINED",
                "userDefinedProfileId": "cp-user-defined",
                "defaultCheapProfileId": "cp-default-cheap",
                "fallbackToLocal": true,
            }
        });
        let cheap = cheap_llm_settings_from_stored(Some(&settings));
        assert_eq!(cheap.strategy, "USER_DEFINED");
        assert_eq!(
            cheap.user_defined_profile_id.as_deref(),
            Some("cp-user-defined")
        );
        assert_eq!(
            cheap.default_cheap_profile_id.as_deref(),
            Some("cp-default-cheap")
        );
        assert!(cheap.fallback_to_local);
    }

    /// An absent `strategy` sub-key defaults to `PROVIDER_CHEAPEST` (the real Zod
    /// default; the enum has no "AUTO"), never a phantom strategy that would match
    /// no selection branch.
    #[test]
    fn cheap_settings_absent_strategy_defaults_to_provider_cheapest() {
        let settings = json!({ "cheapLLMSettings": { "fallbackToLocal": false } });
        let cheap = cheap_llm_settings_from_stored(Some(&settings));
        assert_eq!(cheap.strategy, "PROVIDER_CHEAPEST");
        assert_eq!(cheap.user_defined_profile_id, None);
        assert_eq!(cheap.default_cheap_profile_id, None);
        assert!(!cheap.fallback_to_local);
    }

    /// A wholly-absent settings object (`None`) also defaults cleanly — the caller
    /// only reaches `run_summary_check` when the presence gate already fired, but
    /// the parse stays total.
    #[test]
    fn cheap_settings_from_none_defaults() {
        let cheap = cheap_llm_settings_from_stored(None);
        assert_eq!(cheap.strategy, "PROVIDER_CHEAPEST");
        assert_eq!(cheap.default_cheap_profile_id, None);
        assert!(!cheap.fallback_to_local);
    }
}
