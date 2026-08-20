//! Regenerate-swipe (port of v4
//! `lib/services/chat-message/regenerate-swipe.service.ts`).
//!
//! `regenerate_message_as_swipe` is the sibling entry point to
//! [`super::orchestrator::process_message`]: it generates an alternative
//! ("swipe") for an existing ASSISTANT message and persists it as a
//! properly-attributed variant of that message, grouped in place. Unlike the
//! live send path it runs a **single non-streaming** provider call (no tools, no
//! turn chain) — a swipe is one alternative line — so the streaming path stays
//! untouched.
//!
//! It composes the already-ported services: [`super::participant_resolver`]
//! (responder resolution + participant data + roleplay template),
//! [`super::user_identity_resolver`], [`super::message_context`] (the full
//! provider-ready context, continue-mode), the [`CompletionProvider`] seam (the
//! one generation), the swipe-group bookkeeping on `chat_messages`, and the
//! ported [`super::memory_service::delete_memories_by_source_message_with_vectors`]
//! cascade.
//!
//! **Tracked deferral:** the new swipe's `rawResponse` / `reasoningContent` /
//! `thoughtSignature` are set to `null`. The ported [`CompletionResponse`] is the
//! cheap-LLM subset (`content` + `usage`) and does not carry the provider's raw
//! payload / reasoning / thought signature; forwarding them awaits the richer
//! wire-decoded response the provider manifest lands (W4.7). The corpus's canned
//! completion returns none of them, so `null` is byte-faithful there.

use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::{memories_read, DbError};
use crate::model::completion::{
    CompletionAttachment, CompletionMessage, CompletionParams, CompletionProvider, CompletionRole,
};
use crate::model::embedding::EmbeddingProvider;
use crate::services::build_context::{BuildContextSeams, ConnectionProfileInput};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::memory_service::delete_memories_by_source_message_with_vectors;
use crate::services::message_context::{
    self, FormattedMsg, MessageContextParams, MessageContextSeams,
};
use crate::services::orchestrator::{
    build_context_input, effective_profile_profile, json_f64, json_str, BuildContextArgs,
};

/// The inputs `regenerate_message_as_swipe` consumes (v4 `RegenerateSwipeOptions`
/// + the injected wall-clock / model-limit the differential freezes).
pub struct RegenerateSwipeOptions {
    /// The user id.
    pub user_id: String,
    /// The chat the message lives in (already loaded by the caller — the
    /// slim-row-read + overlaid `Value`).
    pub chat: Value,
    /// The ASSISTANT message being regenerated (a `chat_messages` event `Value`).
    pub target_message: Value,
    /// All events in the chat (already loaded by the caller).
    pub all_messages: Vec<Value>,
    /// The user-controlled participant the human is "Speaking As" (optional).
    pub active_user_participant_id: Option<String>,
    /// The model's context-window limit (v4 `getModelContextLimit`; resolved above
    /// the seam).
    pub model_context_limit: i64,
    /// The optional timestamp config (v4 `chatSettings.defaultTimestampConfig`).
    pub timestamp_config: Option<crate::chat_timestamp::TimestampConfig>,
    /// The resolved IANA timezone (v4 `resolveTimezone`).
    pub timezone: Option<String>,
    /// The SERVER-LOCAL IANA zone (v4's ambient process zone) — the memory
    /// distill's TODAY line + day-reference scan resolve their calendar in it.
    /// ⚠ NOT [`Self::timezone`], the story/timestamp zone above.
    pub server_tz: Option<String>,
    /// The wall clock (v4 `Date.now()` — the buildContext timestamp base + the
    /// swipe id/… mint points; the swipe's own `createdAt` is the target's).
    pub now_ms: i64,
    /// The local UTC offset (minutes) buildContext's timestamp math reads.
    pub local_offset_minutes: i64,
    /// `Math.random()`'s value for the weighted responder fallback (only read when
    /// the target message carries no participant id — a legacy single-char row).
    pub random01: f64,
}

/// Error from a regenerate-swipe (v4 throws for a non-regenerable target).
#[derive(Debug)]
pub enum RegenError {
    /// The target is not an ASSISTANT message (v4 "Only assistant messages can be
    /// regenerated").
    NotAssistant,
    /// The target is a Staff/system-authored message (v4 "Staff and system
    /// messages cannot be regenerated").
    StaffMessage,
    /// A DB / resolution / stream error surfaced.
    Db(DbError),
}

impl std::fmt::Display for RegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegenError::NotAssistant => write!(f, "Only assistant messages can be regenerated"),
            RegenError::StaffMessage => {
                write!(f, "Staff and system messages cannot be regenerated")
            }
            RegenError::Db(e) => write!(f, "regenerate-swipe failed: {e:?}"),
        }
    }
}
impl From<DbError> for RegenError {
    fn from(e: DbError) -> Self {
        RegenError::Db(e)
    }
}

/// v4 `regenerateMessageAsSwipe`. Generate a fresh response for an existing
/// ASSISTANT message and persist it as a swipe variant. Returns the new swipe
/// message event.
#[allow(clippy::too_many_arguments)]
pub async fn regenerate_message_as_swipe<EMB, CMP, BCS, MCS>(
    db: &Db,
    embedding: &EMB,
    completion: &CMP,
    executor: &CheapLlmTaskExecutor,
    bc_seams: &BCS,
    mc_seams: &MCS,
    opts: RegenerateSwipeOptions,
) -> Result<Value, RegenError>
where
    EMB: EmbeddingProvider,
    CMP: CompletionProvider,
    BCS: BuildContextSeams,
    MCS: MessageContextSeams,
{
    let RegenerateSwipeOptions {
        user_id,
        chat,
        target_message,
        all_messages,
        active_user_participant_id,
        model_context_limit,
        timestamp_config,
        timezone,
        server_tz,
        now_ms,
        local_offset_minutes,
        random01,
    } = opts;

    // --- Guards (v4 lines 60–67) ---
    if json_str(&target_message, "role").as_deref() != Some("ASSISTANT") {
        return Err(RegenError::NotAssistant);
    }
    // Staff/system-authored messages (Lantern, Host, Prospero, …) are not
    // character turns — they have no responder to regenerate from.
    if target_message
        .get("systemSender")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Err(RegenError::StaffMessage);
    }

    let chat_id = json_str(&chat, "id").unwrap_or_default();
    let target_message_id = json_str(&target_message, "id").unwrap_or_default();

    // --- Resolve the responder from the message's own participant (v4 68–86) ---
    // A null participant (legacy/single-character message) falls back to weighted
    // first-responder selection; a present one pins the responder (continueMode =
    // true → "throw if not found").
    let requested_participant_id = json_str(&target_message, "participantId");
    let resolution = super::participant_resolver::resolve_responding_participant(
        db,
        &chat,
        &user_id,
        requested_participant_id.as_deref(),
        requested_participant_id.is_some(),
        None,
        random01,
    )
    .await
    .map_err(|e| DbError::Internal(format!("participant resolution failed: {e:?}")))?;

    let character_participant = resolution.character_participant.clone();
    let character = resolution.character.clone();
    let connection_profile = resolution.connection_profile.clone();
    let is_multi_character = resolution.is_multi_character;

    let character_id = json_str(&character, "id").unwrap_or_default();
    let character_name = json_str(&character, "name").unwrap_or_default();
    let character_participant_id = json_str(&character_participant, "id").unwrap_or_default();

    // --- User identity (honors "Speaking As") + per-char map + template (v4 88–98) ---
    let speaking_as_id = active_user_participant_id.clone().or_else(|| {
        chat.get("activeTypingParticipantId")
            .and_then(Value::as_str)
            .map(String::from)
    });
    let identity = super::user_identity_resolver::resolve_user_identity(
        db,
        &user_id,
        &chat,
        speaking_as_id.as_deref(),
    )
    .await?;
    let user_character = Some(crate::system_prompt::UserCharacter {
        name: identity.name.clone(),
        description: identity.description.clone(),
    });

    let participant_characters =
        super::participant_resolver::load_all_participant_data(db, &chat, &character)?;

    let uid = user_id.clone();
    let chat_settings =
        db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid))?;
    let default_roleplay_template_id = chat_settings
        .as_ref()
        .and_then(|s| s.get("defaultRoleplayTemplateId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let roleplay_template = super::participant_resolver::get_roleplay_template(
        db,
        &chat,
        default_roleplay_template_id.as_deref(),
    )
    .await
    .map_err(|e| DbError::Internal(format!("roleplay template resolution failed: {e:?}")))?
    .map(|r| r.system_prompt);

    // --- Context = everything strictly BEFORE the target (v4 100–106) ---
    // Sibling swipes share the original's timestamp, so a strict `<` also drops
    // them.
    let target_time =
        crate::clock::iso_to_ms(&json_str(&target_message, "createdAt").unwrap_or_default())
            .unwrap_or(i64::MAX);
    let previous_messages: Vec<Value> = all_messages
        .iter()
        .filter(|m| {
            m.get("type").and_then(Value::as_str) == Some("message")
                && json_str(m, "createdAt")
                    .and_then(|c| crate::clock::iso_to_ms(&c))
                    .map(|ms| ms < target_time)
                    .unwrap_or(false)
        })
        .cloned()
        .collect();

    // --- Build the full provider-ready context (continue mode, no new user msg) ---
    let cp_input: ConnectionProfileInput = effective_profile_profile(&connection_profile);
    let build_input = build_context_input(BuildContextArgs {
        user_id: &user_id,
        model_context_limit,
        // v4's `f933ba9c` reserves the turn extras on the orchestrator path only;
        // the regenerate/swipe path builds no tool schemas and splices nothing,
        // so it reserves nothing here either.
        reserved_outgoing_tokens: None,
        timestamp_config: timestamp_config.clone(),
        timezone: timezone.clone(),
        server_tz: server_tz.clone(),
        is_continue_mode: true,
        now_ms,
        local_offset_minutes,
        chat: &chat,
        character: &character,
        character_participant: &character_participant,
        connection_profile: &cp_input,
        user_character,
        roleplay_template,
        is_multi_character,
        participant_characters: &participant_characters,
        existing_messages: &previous_messages,
        // continue mode → no new user message.
        final_user_message: None,
        speaking_as: speaking_as_id.clone(),
        tool_instructions: None,
        compression_enabled: false,
        bypass_compression: false,
        // Regenerate-swipe rebuilds context fresh; no async pre-compression cache.
        cached_compression_result: None,
        cached_compression_message_count: None,
        // Regenerate-swipe does not resolve a cheap-LLM selection (its rebuild is a
        // single continue-mode generation; the recap/distill feeders stay inert, as
        // in its differential).
        cheap_llm_selection: None,
        uncensored_fallback: None,
        // Not an autonomous turn — no per-turn context cap (v4's regenerate path
        // never carries `autonomousContextCap`).
        autonomous_context_cap: None,
        // Regenerate-as-swipe never offers the "nothing to add" pass (v4 passes
        // no `turnSkip`).
        turn_skip: None,
        // Regenerate-swipe does not run the proactive pre-compute distill (P4.19);
        // its rebuild falls through to buildContext's own fallback path, as before.
        pre_searched_memories: None,
        recall_signals: None,
        pre_searched_query_embedding: None,
    });

    // The per-character opaque-anywhere transparency map (v4's wrapper reads each
    // present character's `systemTransparency`; the responder from `character`, the
    // rest from `participant_characters`).
    let mut participant_transparency: std::collections::HashMap<String, Option<bool>> =
        std::collections::HashMap::new();
    if is_multi_character {
        if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
            for p in parts {
                if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                    continue;
                }
                let Some(cid) = p
                    .get("characterId")
                    .and_then(Value::as_str)
                    .filter(|c| !c.is_empty())
                else {
                    continue;
                };
                let status = p.get("status").and_then(Value::as_str).unwrap_or("active");
                if status != "active" && status != "silent" {
                    continue;
                }
                let transp = if cid == character_id {
                    character.get("systemTransparency").and_then(Value::as_bool)
                } else {
                    participant_characters
                        .get(cid)
                        .and_then(|c| c.get("systemTransparency").and_then(Value::as_bool))
                };
                participant_transparency.insert(cid.to_string(), transp);
            }
        }
    }
    let cp_created_at = json_str(&character_participant, "createdAt");
    let empty_participants: Vec<Value> = Vec::new();
    let mc_params = MessageContextParams {
        is_multi_character,
        provider: &json_str(&connection_profile, "provider").unwrap_or_default(),
        responding_character_name: &character_name,
        responding_participant_id: &character_participant_id,
        has_history_access: character_participant
            .get("hasHistoryAccess")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        character_participant_created_at: cp_created_at.as_deref(),
        character_system_transparency: character.get("systemTransparency").and_then(Value::as_bool),
        // v4 `23af7146` — the anchor route lives inside `buildMessageContext`,
        // so the swipe path resolves it from the same profile the send path
        // does.
        use_prefill: crate::services::multi_character_prefill::profile_uses_name_prefill_value(
            &connection_profile,
        ),
        participants: chat
            .get("participants")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&empty_participants),
        participant_transparency: &participant_transparency,
    };

    let no_attachments: Vec<Value> = Vec::new();
    let mc_result = message_context::build_message_context(
        db,
        embedding,
        completion,
        executor,
        bc_seams,
        mc_seams,
        &mc_params,
        build_input,
        &previous_messages,
        &no_attachments,
    )
    .await
    .map_err(|e| DbError::Internal(format!("buildMessageContext failed: {e:?}")))?;

    // --- Single non-streaming generation (v4 132–153) ---
    // v4 `d9c5a1c7`: `const params = profileParams(connectionProfile) ?? {}`,
    // which REPLACED `(connectionProfile.parameters || {})`. Two behaviour
    // changes ride the conversion, both v4's: a non-object `parameters` cell
    // (a number, a string) now collapses to `{}` instead of being forwarded
    // verbatim, and an Ollama profile's Max Context is injected as `num_ctx`.
    let params_value =
        crate::cheap_llm::profile_params_value(&connection_profile).unwrap_or_else(|| json!({}));
    let messages: Vec<CompletionMessage> = mc_result
        .formatted_messages
        .iter()
        .map(|m: &FormattedMsg| CompletionMessage {
            role: match m.role.as_str() {
                "system" => CompletionRole::System,
                "assistant" => CompletionRole::Assistant,
                _ => CompletionRole::User,
            },
            content: m.content.clone(),
        })
        .collect();
    // The stamped attachments (last user message — the only carrier) thread into
    // `CompletionParams.attachments`, which `request_input_from_params` stamps
    // back onto the last user message for the builders (P4.21 drop site 3 — v4
    // forwards `attachments: m.attachments` into `provider.sendMessage` here).
    //
    // NAMED DIVERGENCE (§3 review, recorded not fixed): this funnel narrows the
    // bag to 4 fields and so drops `url`. A mount-file bag carries BOTH `url`
    // and `data`, and Z.AI/OpenRouter prefer `url` — so a regenerate with a
    // mount-file attachment on those providers sends the data URL where v4
    // sends its relative `/api/v1/mount-points/…` url (itself a v4 bug: the
    // provider cannot fetch it). Widening `CompletionAttachment` would disturb
    // the frozen canned-key serialization the tier-3 oracles record, so the
    // narrowing stays until that key is versioned.
    let attachments: Vec<CompletionAttachment> = mc_result
        .formatted_messages
        .iter()
        .rev()
        .find_map(|m| m.attachments.as_ref())
        .map(|bags| {
            bags.iter()
                .map(|a| CompletionAttachment {
                    id: json_str(a, "id").unwrap_or_default(),
                    filename: json_str(a, "filename").unwrap_or_default(),
                    mime_type: json_str(a, "mimeType").unwrap_or_default(),
                    data: json_str(a, "data").unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    // P4.D83 (v4 `d89babc4`): `...resolveSamplingParams(params)` replaced three
    // hand-rolled casts here. Two changes ride the conversion, both v4's: `top_p`
    // is now read at all (it never was on this path), and a knob the profile
    // stores under the camelCase spelling is honoured. The absent-`max_tokens`
    // arm also stops lying — v5 collapsed it to `0`, which reached the wire as a
    // literal `max_tokens: 0` where v4 leaves the key to the provider's default.
    let sampling = crate::sampling_params::resolve_sampling_params(Some(&params_value));
    let temperature = sampling.temperature;
    let max_tokens = sampling.max_tokens.map(|f| f as i64);
    let top_p = sampling.top_p;
    let provider = json_str(&connection_profile, "provider").unwrap_or_default();
    let base_url = json_str(&connection_profile, "baseUrl");
    let model = json_str(&connection_profile, "modelName").unwrap_or_default();
    let response = completion
        .send_message(
            &provider,
            base_url.as_deref(),
            &CompletionParams {
                messages,
                model,
                temperature,
                max_tokens,
                strict_max_tokens: false,
                top_p,
                cache_key: Some(character_id.clone()),
                profile_parameters: Some(params_value),
                attachments,
                // A visible regeneration, not cheap-LLM work: v4 sets no
                // `requestTimeoutMs` here.
                request_timeout_ms: None,
            },
        )
        .await
        .map_err(|e| DbError::Internal(format!("swipe generation failed: {}", e.message)))?;

    // --- Persist the grouping (v4 155–194) ---
    let target_swipe_group_id = json_str(&target_message, "swipeGroupId");
    let swipe_group_id = match &target_swipe_group_id {
        Some(g) if !g.is_empty() => g.clone(),
        _ => format!("swipe-{target_message_id}"),
    };
    // The original anchors the group at index 0; on the first regeneration its
    // swipeGroupId must be written back (else the variant renders as a stray,
    // mis-attributed message).
    if target_swipe_group_id.as_deref().unwrap_or("").is_empty() {
        let write_chat_id = chat_id.clone();
        let write_target_id = target_message_id.clone();
        let updates = json!({ "swipeGroupId": swipe_group_id, "swipeIndex": 0 });
        db.write(move |w| {
            w.main()
                .chat_messages()
                .update_message(&write_chat_id, &write_target_id, &updates)
                .map(|_| ())
        })
        .await?;
    }

    let new_swipe_index = all_messages
        .iter()
        .filter(|m| {
            m.get("type").and_then(Value::as_str) == Some("message")
                && (json_str(m, "swipeGroupId").as_deref() == Some(swipe_group_id.as_str())
                    || json_str(m, "id").as_deref() == Some(target_message_id.as_str()))
        })
        .map(|m| json_f64(m, "swipeIndex").unwrap_or(0.0) as i64)
        .max()
        .unwrap_or(0)
        + 1;

    let new_swipe_id = uuid::Uuid::new_v4().to_string();
    let usage = response.usage;
    let mut swipe = Map::new();
    swipe.insert("type".into(), json!("message"));
    swipe.insert("id".into(), json!(new_swipe_id));
    swipe.insert("role".into(), json!("ASSISTANT"));
    swipe.insert("content".into(), json!(response.content));
    // Attribute to the same participant that authored the original (the fix for
    // the regenerated-message-shows-the-wrong-character bug).
    swipe.insert("participantId".into(), json!(character_participant_id));
    swipe.insert("swipeGroupId".into(), json!(swipe_group_id));
    swipe.insert("swipeIndex".into(), json!(new_swipe_index));
    swipe.insert(
        "tokenCount".into(),
        usage.map_or(Value::Null, |u| json!(u.total_tokens)),
    );
    swipe.insert(
        "promptTokens".into(),
        usage.map_or(Value::Null, |u| json!(u.prompt_tokens)),
    );
    swipe.insert(
        "completionTokens".into(),
        usage.map_or(Value::Null, |u| json!(u.completion_tokens)),
    );
    // Tracked deferral: the cheap-LLM CompletionResponse carries none of these.
    swipe.insert("rawResponse".into(), Value::Null);
    swipe.insert("reasoningContent".into(), Value::Null);
    swipe.insert("thoughtSignature".into(), Value::Null);
    swipe.insert("provider".into(), json!(provider));
    swipe.insert(
        "modelName".into(),
        json!(json_str(&connection_profile, "modelName")),
    );
    swipe.insert("attachments".into(), json!([]));
    // Keep the original's timestamp so the group stays in place in the transcript.
    swipe.insert(
        "createdAt".into(),
        json!(json_str(&target_message, "createdAt")),
    );
    let new_swipe = Value::Object(swipe);

    let write_chat_id = chat_id.clone();
    let event: crate::db::chats_messages::ChatEventInput =
        serde_json::from_value(new_swipe.clone())
            .map_err(|e| DbError::Internal(format!("swipe message marshal: {e}")))?;
    db.write(move |w| w.main().chat_messages().add_message(&write_chat_id, &event))
        .await?;
    // v4 calls `repos.chats.update(chat.id, {})` (an empty patch — a no-op SET; the
    // metadata bump already happened inside `addMessage`).
    let write_chat_id = chat_id.clone();
    db.write(move |w| {
        w.main()
            .chats()
            .update(&write_chat_id, &crate::db::chats::ChatUpdate::default())
            .map(|_| ())
    })
    .await?;

    // --- Memory cascade (v4 196–212): the replaced variant may have seeded
    // memories. Error-swallowed (v4 logs a warn, keeps the swipe). ---
    let cascade_action = chat_settings
        .as_ref()
        .and_then(|s| s.get("memoryCascadePreferences"))
        .and_then(|m| m.get("onSwipeRegenerate"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("DELETE_MEMORIES")
        .to_string();
    if cascade_action != "KEEP_MEMORIES" {
        let cascade: Result<(), DbError> = async {
            let tid = target_message_id.clone();
            let count =
                db.read_main(move |c| memories_read::count_by_source_message_id(c, &tid))?;
            if count > 0 {
                delete_memories_by_source_message_with_vectors(db, &target_message_id).await?;
            }
            Ok(())
        }
        .await;
        // v4 swallows + logs a warn; the swipe is kept regardless.
        let _ = cascade;
    }

    Ok(new_swipe)
}
