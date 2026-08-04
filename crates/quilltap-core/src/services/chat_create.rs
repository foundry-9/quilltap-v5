//! The chat-creation spine — v4's `handleCreate`
//! (`app/api/v1/chats/route.ts:897-1324`) + its helpers
//! (`buildAllParticipants`/`buildCharacterParticipant`/
//! `pickWeightedByTalkativeness` L187-316, `writeSystemPromptMessage`/
//! `createInitialMessagesScenarioAndStaff` L318-551, `autoGenerateFirstMessage`
//! L578-829, `createChatSchema` L80-159).
//!
//! This is a **composition** of the seven already-landed sub-units (scenario
//! resolvers, `build_chat_context`, `compile_all_identity_stacks`,
//! `apply_outfit_selections`, `generate_greeting_message`,
//! `apply_chat_continuation`, the Green-Room `CreationProgressEmitter`) plus the
//! seed-phase whisper writers (Prospero / Host / Aurora) and the autonomous-room
//! lifecycle start — nothing is re-ported here.
//!
//! ## The two write paths (per the P4.4u2b work order)
//!
//! `main` / `mount` are **writable** connections (the caller opens
//! `Writer::open_writable` and passes `.connection()`); every direct repo write
//! (`chats.create`, the message adds, `set_equipped_outfit`, the identity-stack
//! persist, the project roster update) and every `&Connection` sub-unit read runs
//! over them. The seed-phase whisper writers, `build_first_message_context`, the
//! continuation backfill, the avatar trigger, and the greeting `logLLMCall` go
//! through the single-writer [`Db`] handle. `user_id` is always `SINGLE_USER_ID`.

use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::api::engine::SINGLE_USER_ID;
use crate::chat_timestamp;
use crate::clock::{iso_from_unix_ms, now_iso};
use crate::db::chats::{ChatCreate, ChatsRepository, CreateOptions};
use crate::db::chats_messages::{ChatEventInput, ChatMessagesRepository};
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::document_store_overlay::OverlayError;
use crate::db::projects::ProjectsRepository;
use crate::db::runtime::Db;
use crate::db::{
    api_keys, characters_read, chat_settings, chats_read, connection_profiles, image_profiles,
    projects, scenarios, wardrobe_read, DbError,
};
use crate::enclave::cron;
use crate::enclave::lifecycle::{self, LifecycleDeps, StartManualRunResult};
use crate::jsstr::js_trim;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::model::stream::StreamingCompletionProvider;
use crate::provider_manifest::Registry;
use crate::scenario_text::combine_scenario_text;
use crate::services::avatar_generation::{
    trigger_avatar_generation_if_enabled, AvatarGenerationParams,
};
use crate::services::chat_continuation::apply_chat_continuation;
use crate::services::chat_enrichment::{enrich_participant_summary, EnrichedParticipantSummary};
use crate::services::chat_initialize::{build_chat_context, ChatContext};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::creation_progress::CreationProgressEmitter;
use crate::services::dangerous_content::provider_routing::{
    resolve_provider_for_dangerous_content, ApiKeyResolver, RouteProfile,
};
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::first_message_context::{
    build_first_message_context, ChatParticipantInput, FirstMessageContextOptions,
};
use crate::services::host_notifications::{
    post_host_add_announcement, post_host_scenario_announcement,
    post_host_user_character_announcement, HostAddAnnouncement, HostCharacter,
    HostScenarioAnnouncement, HostUserCharacterAnnouncement,
};
use crate::services::initial_greeting::{
    generate_greeting_message, GreetingLog, GreetingRequest, ParticipantMemory as GreetingMemory,
    ProjectContext as GreetingProjectContext,
};
use crate::services::llm_logging::LogContext;
use crate::services::outfit_selections::{apply_outfit_selections, OutfitContext, OutfitSelection};
use crate::services::prospero_notifications::{
    load_prospero_general_context, load_prospero_project_context,
    post_prospero_context_announcement, post_prospero_group_context_whisper,
    ProsperoContextAnnouncement, ProsperoGroupContextWhisper,
};
use crate::services::system_prompt_compiler::compile_all_identity_stacks;
use crate::tools::wardrobe_shared::resolve_project_mount_point_ids_for_chat;
use crate::wardrobe::{OutfitSlotValues, Slots};

use super::aurora_notifications::post_opening_outfit_whisper;

// ============================================================================
// Request shape (v4 `createChatSchema` / `createParticipantSchema`)
// ============================================================================

/// One requested participant (v4 `createParticipantSchema`). `character_id` is
/// `Option` so v4's `if (!data.characterId)` "characterId is required" check is
/// reproduced rather than failing at deserialize.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatCreateParticipant {
    #[serde(rename = "type", default = "default_participant_type")]
    pub kind: String,
    pub character_id: Option<String>,
    pub connection_profile_id: Option<String>,
    pub image_profile_id: Option<String>,
    pub controlled_by: Option<String>,
    pub selected_system_prompt_id: Option<String>,
}

impl Default for ChatCreateParticipant {
    fn default() -> Self {
        ChatCreateParticipant {
            kind: default_participant_type(),
            character_id: None,
            connection_profile_id: None,
            image_profile_id: None,
            controlled_by: None,
            selected_system_prompt_id: None,
        }
    }
}

fn default_participant_type() -> String {
    "CHARACTER".to_string()
}

fn de_double_opt_value<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<Option<Option<Value>>, D::Error> {
    use serde::Deserialize as _;
    let v = Value::deserialize(de)?;
    Ok(Some(if v.is_null() { None } else { Some(v) }))
}

/// The double-`Option` deserializer for a `z.string().nullable().optional()`
/// field: absent → `None`, explicit `null` → `Some(None)`, a string →
/// `Some(Some(s))`. Unlike [`de_double_opt_value`]'s consumer, the `null` arm is
/// MEANINGFUL here rather than a rejection (see
/// [`ChatCreateRequest::roleplay_template_id`]).
fn de_double_opt_string<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<Option<Option<String>>, D::Error> {
    use serde::Deserialize as _;
    Ok(Some(Option::<String>::deserialize(de)?))
}

/// The chat-creation request (v4 `createChatSchema`). Every field carries a
/// serde default so a sparse dispatch payload deserializes.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatCreateRequest {
    pub participants: Vec<ChatCreateParticipant>,
    pub title: Option<String>,
    /// Free-text scenario notes (layered beneath any resolved preset body).
    pub scenario: Option<String>,
    pub scenario_id: Option<String>,
    pub project_scenario_path: Option<String>,
    pub general_scenario_path: Option<String>,
    pub group_scenario_path: Option<String>,
    pub group_scenario_group_id: Option<String>,
    /// v4 `timestampConfig: TimestampConfigSchema.optional()` — `.optional()`
    /// is NOT nullable: an explicit JSON `null` REJECTS (400). The double
    /// `Option` keeps that arm expressible (absent → `None`, null →
    /// `Some(None)`, object → `Some(Some(v))`).
    #[serde(deserialize_with = "de_double_opt_value")]
    pub timestamp_config: Option<Option<Value>>,
    pub project_id: Option<String>,
    /// Chat-level image profile (shared by all participants).
    pub image_profile_id: Option<String>,
    /// v4 [`4bbeab47`] `roleplayTemplateId: z.uuid().nullable().optional()` —
    /// the roleplay template for the new chat, chosen in the New Chat dialog.
    /// When the key is PRESENT it wins outright — including an explicit `null`,
    /// which means "no template" — over the project default and the user's
    /// global default. Omit the key entirely to fall back to that default
    /// chain. The double `Option` keeps all three arms expressible (absent →
    /// `None`, null → `Some(None)`, id → `Some(Some(id))`).
    ///
    /// ⚠ The `null` arm is the mirror-image of [`Self::timestamp_config`]'s:
    /// there `.optional()` is NOT nullable so a null REJECTS; here
    /// `.nullable().optional()` makes null a deliberate choice.
    #[serde(deserialize_with = "de_double_opt_string")]
    pub roleplay_template_id: Option<Option<String>>,
    pub outfit_selections: Option<Vec<Value>>,
    pub avatar_generation_enabled: Option<bool>,
    pub continuation_from_chat_id: Option<String>,
    pub progress_id: Option<String>,
    // Autonomous-room fields (only consulted when `chatType === 'autonomous'`).
    pub chat_type: Option<String>,
    pub schedule_cron: Option<String>,
    pub schedule_freshness_window_ms: Option<i64>,
    pub budget_max_turns: Option<i64>,
    pub budget_max_tokens: Option<i64>,
    pub budget_max_wall_clock_ms: Option<i64>,
    pub budget_estimated_spend_cap_usd: Option<f64>,
    pub run_visibility: Option<String>,
    pub run_destructive_tools_allowed: Option<bool>,
    pub budget_exclude_cache_hits: Option<bool>,
}

// ============================================================================
// Deps (the model-boundary generics + the injected clocks/seams)
// ============================================================================

/// The composed dependencies a `handle_create` needs — generic over the three
/// model boundaries (the [`ChatSpine`](../../../quilltap_host/spine) precedent),
/// with the wall clock / RNG / lifecycle + api-key seams injected.
pub struct ChatCreateDeps<'a, EMB, CMP, STR>
where
    EMB: EmbeddingProvider + Send + Sync,
    CMP: CompletionProvider + Send + Sync,
    STR: StreamingCompletionProvider + Send + Sync,
{
    /// `build_first_message_context` embedder.
    pub embedding: &'a EMB,
    /// The `llm_choose` outfit boundary.
    pub completion: &'a CMP,
    /// The greeting streamer.
    pub streaming: &'a STR,
    /// The cheap-LLM executor the outfit `llm_choose` path runs on.
    pub executor: &'a CheapLlmTaskExecutor,
    /// The greeting content-filter reroute's api-key resolver.
    pub api_keys: &'a dyn ApiKeyResolver,
    /// IANA time zone for the cron next-run evaluation.
    pub tz: String,
    /// v4 `Date.now()` — the millisecond wall clock.
    pub now_ms: i64,
    /// v4 `Math.random()` for `pickWeightedByTalkativeness` (one value shared,
    /// matching the spine's `random01` injection).
    pub random01: f64,
    /// The autonomous-room lifecycle seam (`start_autonomous_room_manually`).
    pub lifecycle: &'a LifecycleDeps<'a>,
    /// When true (and the llm-logs partition exists), wire the greeting
    /// `logLLMCall` write.
    pub greeting_log: bool,
}

// ============================================================================
// Result + errors
// ============================================================================

/// The composed result. The v4 201 body is `{ chat: { ...chat, participants:
/// enriched } }`; the driver/host merges `participants` into `chat` for the DTO.
pub struct ChatCreateResult {
    /// The full hydrated chat row (its own `participants` still present).
    pub chat: Value,
    /// The enriched participant summaries that REPLACE `chat.participants` in the
    /// 201 body.
    pub participants: Vec<EnrichedParticipantSummary>,
}

/// v4's `handleCreate` failure modes: `notFound` (404), `badRequest` (400), and
/// everything else (500) as a `Db` wrap.
#[derive(Debug)]
pub enum HandleCreateError {
    NotFound(String),
    BadRequest(String),
    Db(DbError),
}

impl std::fmt::Display for HandleCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandleCreateError::NotFound(what) => write!(f, "{what} not found"),
            HandleCreateError::BadRequest(msg) => write!(f, "{msg}"),
            HandleCreateError::Db(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for HandleCreateError {}

impl From<DbError> for HandleCreateError {
    fn from(e: DbError) -> Self {
        HandleCreateError::Db(e)
    }
}
impl From<OverlayError> for HandleCreateError {
    fn from(e: OverlayError) -> Self {
        // A degraded store on `projects.findById`/`update` is v4's throw → 500.
        HandleCreateError::Db(DbError::Key(e.to_string()))
    }
}

// ============================================================================
// The spine
// ============================================================================

/// v4 `handleCreate` — the full chat-creation sequence.
#[allow(clippy::too_many_arguments)]
pub async fn handle_create<EMB, CMP, STR>(
    db: &Db,
    main: &Connection,
    mount: &Connection,
    llm_logs: Option<&Connection>,
    deps: &ChatCreateDeps<'_, EMB, CMP, STR>,
    req: &ChatCreateRequest,
    emitter: &CreationProgressEmitter,
) -> Result<ChatCreateResult, HandleCreateError>
where
    EMB: EmbeddingProvider + Send + Sync,
    CMP: CompletionProvider + Send + Sync,
    STR: StreamingCompletionProvider + Send + Sync,
{
    let user_id = SINGLE_USER_ID;
    let is_autonomous = req.chat_type.as_deref() == Some("autonomous");

    emitter.status("Assembling the cast\u{2026}");

    // 1. Continuation ownership pre-check (before any create work).
    if let Some(source_id) = req.continuation_from_chat_id.as_deref() {
        let sid = source_id.to_string();
        if chats_read::find_by_id(main, &sid)?.is_none() {
            return Err(HandleCreateError::NotFound("Source chat".to_string()));
        }
    }

    // 2. Autonomous-room preconditions (fail fast, before participant build).
    let mut autonomous_next_run_at: Option<String> = None;
    if is_autonomous {
        if req
            .participants
            .iter()
            .any(|p| p.controlled_by.as_deref() == Some("user"))
        {
            return Err(HandleCreateError::BadRequest(
                "Autonomous rooms cannot include user-controlled participants".to_string(),
            ));
        }
        let llm_char_count = req
            .participants
            .iter()
            .filter(|p| {
                p.kind == "CHARACTER" && matches!(p.controlled_by.as_deref(), None | Some("llm"))
            })
            .count();
        if llm_char_count < 2 {
            return Err(HandleCreateError::BadRequest(
                "Autonomous rooms require at least two LLM-controlled characters".to_string(),
            ));
        }
        if let Some(cron_raw) = req.schedule_cron.as_deref() {
            let expr = cron_raw.trim();
            if !expr.is_empty() {
                match cron::try_next_occurrence(expr, deps.now_ms, &deps.tz) {
                    Ok(next) => autonomous_next_run_at = next.map(iso_from_unix_ms),
                    Err(_) => {
                        return Err(HandleCreateError::BadRequest(format!(
                            "Invalid cron expression: {expr}"
                        )))
                    }
                }
            }
            // else: all-whitespace = "no schedule" (manual-only).
        }
    }

    // 3. Build participants (validation + weighted opener selection).
    let built = build_all_participants(main, mount, &req.participants, deps.random01)?;

    // Fetch the primary character for defaults resolution.
    let primary_character = characters_read::find_by_id(main, mount, &built.first_character_id)?;

    // 4. Resolve the chosen preset scenario body, by precedence.
    let mut resolved_scenario: Option<String> = None;
    if resolved_scenario.is_none() {
        if let Some(scenario_id) = req.scenario_id.as_deref() {
            resolved_scenario = primary_character
                .as_ref()
                .and_then(|c| c.get("scenarios"))
                .and_then(Value::as_array)
                .and_then(|arr| {
                    arr.iter()
                        .find(|s| s.get("id").and_then(Value::as_str) == Some(scenario_id))
                })
                .and_then(|s| s.get("content").and_then(Value::as_str))
                .map(str::to_string);
        }
    }
    if resolved_scenario.is_none() {
        if let Some(path) = req.project_scenario_path.as_deref() {
            if let Some(project_id) = req.project_id.as_deref() {
                // Only the store pointer is needed — the slim raw read.
                if let Some(Some(mpid)) =
                    projects::find_official_mount_point_id_raw(main, project_id)?
                {
                    resolved_scenario =
                        scenarios::resolve_project_scenario_body(mount, &mpid, path);
                }
            }
        }
    }
    if resolved_scenario.is_none() {
        if let Some(path) = req.group_scenario_path.as_deref() {
            if let Some(group_id) = req.group_scenario_group_id.as_deref() {
                if let Some(Some(mpid)) =
                    crate::db::groups::find_official_mount_point_id_raw(main, group_id)?
                {
                    resolved_scenario = scenarios::resolve_group_scenario_body(mount, &mpid, path);
                }
            }
        }
    }
    if resolved_scenario.is_none() {
        if let Some(path) = req.general_scenario_path.as_deref() {
            resolved_scenario = scenarios::resolve_general_scenario_body(main, mount, path)?;
        }
    }

    // Append the free-text notes (beneath a preset, or as the whole scenario).
    let preset_body = resolved_scenario.clone();
    let resolved_scenario = combine_scenario_text(preset_body.as_deref(), req.scenario.as_deref());

    // 5. Build the chat context (system prompt + first message + characters).
    let chat_context = build_chat_context(
        main,
        mount,
        &built.first_character_id,
        built.first_user_character_id.as_deref(),
        resolved_scenario.as_deref(),
        built.first_selected_system_prompt_id.as_deref(),
    )?;

    let chat_settings = chat_settings::find_by_user_id(main, user_id)?;
    let now = now_iso();

    // Mint participant ids + timestamps.
    let mut participants: Vec<Value> = built.participants;
    for p in &mut participants {
        if let Value::Object(map) = p {
            map.insert("id".into(), Value::String(uuid::Uuid::new_v4().to_string()));
            map.insert("createdAt".into(), Value::String(now.clone()));
            map.insert("updatedAt".into(), Value::String(now.clone()));
        }
    }

    // 6. Project defaults + roster (v4 L1077-1113).
    let mut project_disabled_tools: Vec<Value> = Vec::new();
    let mut project_disabled_tool_groups: Vec<Value> = Vec::new();
    let mut project_avatar_default: Option<bool> = None;
    let mut project_default_image_profile_id: Option<String> = None;
    let mut project_default_roleplay_template_id: Option<String> = None;

    if let Some(project_id) = req.project_id.as_deref() {
        let repo = ProjectsRepository::new(main, mount);
        let Some(project) = repo.find_by_id(project_id)? else {
            return Err(HandleCreateError::NotFound("Project".to_string()));
        };
        project_disabled_tools = project
            .get("defaultDisabledTools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        project_disabled_tool_groups = project
            .get("defaultDisabledToolGroups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        project_avatar_default = project
            .get("defaultAvatarGenerationEnabled")
            .and_then(Value::as_bool);
        project_default_image_profile_id = project
            .get("defaultImageProfileId")
            .and_then(Value::as_str)
            .map(str::to_string);
        project_default_roleplay_template_id = project
            .get("defaultRoleplayTemplateId")
            .and_then(Value::as_str)
            .map(str::to_string);

        let allow_any = project
            .get("allowAnyCharacter")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !allow_any {
            let roster: Vec<String> = project
                .get("characterRoster")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let mut new_ids: Vec<String> = Vec::new();
            for p in &participants {
                if p.get("type").and_then(Value::as_str) == Some("CHARACTER") {
                    if let Some(cid) = p.get("characterId").and_then(Value::as_str) {
                        if !roster.iter().any(|r| r == cid) && !new_ids.iter().any(|n| n == cid) {
                            new_ids.push(cid.to_string());
                        }
                    }
                }
            }
            if !new_ids.is_empty() {
                let mut merged = roster.clone();
                merged.extend(new_ids);
                let mut patch = Map::new();
                patch.insert(
                    "characterRoster".into(),
                    Value::Array(merged.into_iter().map(Value::String).collect()),
                );
                repo.update(project_id, &patch)?;
            }
        }
    }

    // 7. Resolve the fallback chains.
    // Anchor a fictional clock to now as it lands on the chat — this is the
    // moment the config stops being a default and starts being a running clock,
    // and without the anchor it never advances (v4 `e3a9654f`). Salon- and
    // character-level DEFAULTS stay unanchored: a default saved months ago
    // carries no meaningful anchor, and the stamp is applied here whether the
    // config was requested outright or inherited.
    // v4 `createChatSchema`: `timestampConfig: TimestampConfigSchema.optional()`
    // — the REQUEST config is Zod-parsed (defaults materialized, unknown keys
    // stripped) before the fallback chain; a bad value is the middleware's 400
    // (`Validation error`). The stored defaults in the chain re-normalize at
    // the repository write (db/chats.rs), as v4's `_create` validate does.
    let request_timestamp_config: Option<Value> = match &req.timestamp_config {
        None => None,
        // v4's `.optional()` (not nullable) — an explicit null REJECTS.
        Some(None) => {
            return Err(HandleCreateError::BadRequest(
                "Validation error".to_string(),
            ))
        }
        Some(Some(v)) => Some(
            chat_timestamp::parse_timestamp_config(v)
                .map_err(|_| HandleCreateError::BadRequest("Validation error".to_string()))?,
        ),
    };
    let resolved_timestamp_config: Value = chat_timestamp::ensure_fictional_base_real_time(
        &request_timestamp_config
            .clone()
            .or_else(|| {
                primary_character
                    .as_ref()
                    .and_then(|c| c.get("defaultTimestampConfig").cloned())
                    .filter(|v| !v.is_null())
            })
            .or_else(|| {
                chat_settings
                    .as_ref()
                    .and_then(|s| s.get("defaultTimestampConfig").cloned())
                    .filter(|v| !v.is_null())
            })
            .unwrap_or(Value::Null),
        deps.now_ms,
    );

    let chat_image_profile_id: Option<String> = req
        .image_profile_id
        .clone()
        .or(project_default_image_profile_id)
        .or(built.first_image_profile_id.clone());

    // Resolve the roleplay template (v4 `4bbeab47`): explicit request (including
    // a deliberate null) > project default > user/global default > null. Baked
    // onto the chat at creation so the choice — or the project's preference —
    // sticks. A truthy id must resolve or the whole create is a 400, checked
    // BEFORE the chain so an unresolvable id never silently falls back.
    if let Some(Some(requested)) = req.roleplay_template_id.as_ref() {
        if !requested.is_empty()
            && crate::db::roleplay_templates::find_full_json_by_id(main, requested)?.is_none()
        {
            return Err(HandleCreateError::BadRequest(
                "Roleplay template not found".to_string(),
            ));
        }
    }
    let user_default_roleplay_template_id: Option<String> = chat_settings
        .as_ref()
        .and_then(|s| s.get("defaultRoleplayTemplateId").and_then(Value::as_str))
        .map(str::to_string);
    let default_roleplay_template_id: Option<String> = match &req.roleplay_template_id {
        // v4 `typeof validatedData.roleplayTemplateId !== 'undefined'` — the key
        // was present, so its value (id or null) wins over both defaults.
        Some(explicit) => explicit.clone(),
        None => project_default_roleplay_template_id
            .clone()
            .or_else(|| user_default_roleplay_template_id.clone()),
    };
    // v4's `logger.debug('[Chats v1] Resolved roleplay template for new chat')`.
    // Log output is explicitly outside the differential contract (the P4.18
    // ruling), so this is an analog, not an obligation.
    tracing::debug!(
        requested = ?req.roleplay_template_id.as_ref().and_then(Option::as_deref),
        requested_explicitly = req.roleplay_template_id.is_some(),
        project_default = ?project_default_roleplay_template_id,
        user_default = ?user_default_roleplay_template_id,
        resolved = ?default_roleplay_template_id,
        "[Chats v1] Resolved roleplay template for new chat"
    );

    let composition_mode_default = chat_settings
        .as_ref()
        .and_then(|s| s.get("compositionModeDefault").and_then(Value::as_bool))
        .unwrap_or(false);

    // 8. Create the chat (build the camelCase object, deserialize `ChatCreate`).
    let chat_id = uuid::Uuid::new_v4().to_string();
    let character_name = chat_context
        .character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let mut create_obj = json!({
        "userId": user_id,
        "participants": Value::Array(participants.clone()),
        "title": req.title.clone().unwrap_or_else(|| format!("Chat with {character_name}")),
        "contextSummary": resolved_scenario.clone(),
        "tags": Value::Array(built.tags.iter().cloned().map(Value::String).collect()),
        "roleplayTemplateId": default_roleplay_template_id,
        "timestampConfig": resolved_timestamp_config,
        "messageCount": 0,
        "lastMessageAt": Value::Null,
        "lastRenameCheckInterchange": 0,
        "projectId": req.project_id.clone(),
        "scenarioText": resolved_scenario.clone(),
        "disabledTools": Value::Array(project_disabled_tools),
        "disabledToolGroups": Value::Array(project_disabled_tool_groups),
        "imageProfileId": chat_image_profile_id,
        "avatarGenerationEnabled": if is_autonomous {
            Value::Bool(false)
        } else {
            match req.avatar_generation_enabled.or(project_avatar_default) {
                Some(b) => Value::Bool(b),
                None => Value::Null,
            }
        },
        "documentEditingMode": composition_mode_default,
    });

    if is_autonomous {
        let obj = create_obj.as_object_mut().expect("json! object");
        let cron_val = req
            .schedule_cron
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null);
        obj.insert("chatType".into(), Value::String("autonomous".into()));
        obj.insert("scheduleCron".into(), cron_val);
        obj.insert(
            "scheduleFreshnessWindowMs".into(),
            opt_i64(req.schedule_freshness_window_ms),
        );
        obj.insert(
            "scheduleNextRunAt".into(),
            match &autonomous_next_run_at {
                Some(s) => Value::String(s.clone()),
                None => Value::Null,
            },
        );
        obj.insert("budgetMaxTurns".into(), opt_i64(req.budget_max_turns));
        obj.insert("budgetMaxTokens".into(), opt_i64(req.budget_max_tokens));
        obj.insert(
            "budgetMaxWallClockMs".into(),
            opt_i64(req.budget_max_wall_clock_ms),
        );
        obj.insert(
            "budgetEstimatedSpendCapUSD".into(),
            opt_f64(req.budget_estimated_spend_cap_usd),
        );
        obj.insert(
            "runVisibility".into(),
            match &req.run_visibility {
                Some(s) => Value::String(s.clone()),
                None => Value::Null,
            },
        );
        obj.insert(
            "runDestructiveToolsAllowed".into(),
            json!(if req.run_destructive_tools_allowed.unwrap_or(false) {
                1
            } else {
                0
            }),
        );
        // Default to excluding cache hits (1); only an explicit `false` opts in.
        obj.insert(
            "budgetExcludeCacheHits".into(),
            json!(if req.budget_exclude_cache_hits == Some(false) {
                0
            } else {
                1
            }),
        );
        obj.insert("runState".into(), Value::String("idle".into()));
        obj.insert("currentRunId".into(), Value::Null);
        obj.insert("runTurnsConsumed".into(), json!(0));
        obj.insert("runTokensConsumed".into(), json!(0));
    }

    let create_data: ChatCreate = serde_json::from_value(create_obj)
        .map_err(|e| HandleCreateError::Db(DbError::Key(format!("chat create marshal: {e}"))))?;
    ChatsRepository::new(main).create(
        &create_data,
        &CreateOptions {
            id: chat_id.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        },
    )?;

    // Re-read the hydrated chat (v4's `chat` object).
    let chat = chats_read::find_by_id(main, &chat_id)?.ok_or_else(|| {
        HandleCreateError::Db(DbError::Key("created chat vanished on re-read".to_string()))
    })?;

    // 9. Outfit selections (never fatal — v4's create-handler try/catch).
    let cheap_settings = chat_settings
        .as_ref()
        .and_then(|s| s.get("cheapLLMSettings"))
        .filter(|v| !v.is_null());
    // Shared wardrobe tiers in scope for this chat's project — General is always
    // folded in by the pool read; these add the project stores.
    let project_mount_point_ids = crate::tools::wardrobe_shared::resolve_project_mount_point_ids(
        mount,
        req.project_id.as_deref(),
    );
    let outfit_ctx = OutfitContext {
        user_id,
        project_mount_point_ids: &project_mount_point_ids,
        scenario_text: resolved_scenario.as_deref(),
        cheap_settings,
        source_chat_id: req.continuation_from_chat_id.as_deref(),
    };
    let selections = build_outfit_selections(req, &participants);
    emitter.status("Consulting the wardrobe\u{2026}");
    if !selections.is_empty() {
        let _ = apply_outfit_selections(
            main,
            mount,
            deps.completion,
            deps.executor,
            &chat_id,
            &selections,
            &outfit_ctx,
            emitter,
        )
        .await;
    }

    // 10. Precompile the per-participant identity stacks (non-fatal).
    emitter.status("Committing everyone\u{2019}s particulars to memory\u{2026}");
    let _ = compile_all_identity_stacks(main, mount, &chat);

    // 11. Seed the opening messages (branch on continuation / autonomous / normal).
    if let Some(source_id) = req.continuation_from_chat_id.as_deref() {
        emitter.status("Recalling the previous chapter\u{2026}");
        write_system_prompt_message(main, &chat_context, &chat_id)?;
        let _ = apply_chat_continuation(db, &chat_id, source_id).await;
        create_initial_messages_scenario_and_staff(
            db,
            main,
            mount,
            llm_logs,
            deps,
            &chat_context,
            &participants,
            &chat_id,
            req.project_id.as_deref(),
            resolved_scenario.as_deref(),
            true,
        )
        .await;
    } else if is_autonomous {
        write_system_prompt_message(main, &chat_context, &chat_id)?;
        create_initial_messages_scenario_and_staff(
            db,
            main,
            mount,
            llm_logs,
            deps,
            &chat_context,
            &participants,
            &chat_id,
            req.project_id.as_deref(),
            resolved_scenario.as_deref(),
            true,
        )
        .await;
    } else {
        emitter.status("Setting the opening scene\u{2026}");
        write_system_prompt_message(main, &chat_context, &chat_id)?;
        create_initial_messages_scenario_and_staff(
            db,
            main,
            mount,
            llm_logs,
            deps,
            &chat_context,
            &participants,
            &chat_id,
            req.project_id.as_deref(),
            resolved_scenario.as_deref(),
            false,
        )
        .await;
    }

    // 12. Enrich participants for the response body.
    let mut enriched: Vec<EnrichedParticipantSummary> = Vec::new();
    if let Some(chat_participants) = chat.get("participants").and_then(Value::as_array) {
        for p in chat_participants {
            enriched.push(enrich_participant_summary(main, mount, p)?);
        }
    }

    // 13. Ad-hoc autonomous rooms (no cron schedule) start immediately.
    let has_cron = chat
        .get("scheduleCron")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if is_autonomous && !has_cron {
        emitter.status("Ringing up the room to begin\u{2026}");
        // Swallow both the declined result and any error (v4 logs + continues).
        let _: Result<StartManualRunResult, DbError> =
            lifecycle::start_autonomous_room_manually(db, deps.lifecycle, &chat_id, user_id).await;
    }

    emitter.status("The players are ready.");
    emitter.finish();

    Ok(ChatCreateResult {
        chat,
        participants: enriched,
    })
}

// ============================================================================
// build_all_participants (v4 L187-316)
// ============================================================================

struct BuiltParticipants {
    /// Participant JSON objects WITHOUT id/createdAt/updatedAt (minted later).
    participants: Vec<Value>,
    tags: Vec<String>,
    first_character_id: String,
    first_user_character_id: Option<String>,
    first_selected_system_prompt_id: Option<String>,
    first_image_profile_id: Option<String>,
}

struct LlmCandidate {
    character_id: String,
    selected_system_prompt_id: Option<String>,
    talkativeness: f64,
}

fn build_all_participants(
    main: &Connection,
    mount: &Connection,
    participants_data: &[ChatCreateParticipant],
    random01: f64,
) -> Result<BuiltParticipants, HandleCreateError> {
    let mut built: Vec<Value> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    let mut candidates: Vec<LlmCandidate> = Vec::new();
    let mut first_user_character_id: Option<String> = None;
    let mut first_image_profile_id: Option<String> = None;

    for (i, data) in participants_data.iter().enumerate() {
        // v4 `buildCharacterParticipant`.
        let Some(character_id) = data.character_id.as_deref() else {
            return Err(HandleCreateError::BadRequest(
                "characterId is required for CHARACTER participants".to_string(),
            ));
        };
        let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
            return Err(HandleCreateError::BadRequest(
                "Character not found".to_string(),
            ));
        };

        let controlled_by = data
            .controlled_by
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                character
                    .get("controlledBy")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "llm".to_string());
        let is_user_controlled = controlled_by == "user";

        if !is_user_controlled && data.connection_profile_id.is_none() {
            return Err(HandleCreateError::BadRequest(
                "connectionProfileId is required for LLM-controlled CHARACTER participants"
                    .to_string(),
            ));
        }
        if let Some(cpid) = data.connection_profile_id.as_deref() {
            if connection_profiles::find_by_id(main, cpid)?.is_none() {
                return Err(HandleCreateError::BadRequest(
                    "Connection profile not found".to_string(),
                ));
            }
        }
        if let Some(ipid) = data.image_profile_id.as_deref() {
            if image_profiles::find_by_id(main, ipid)?.is_none() {
                return Err(HandleCreateError::BadRequest(
                    "Image profile not found".to_string(),
                ));
            }
        }

        let participant = json!({
            "type": "CHARACTER",
            "characterId": character_id,
            "controlledBy": controlled_by,
            "connectionProfileId": if is_user_controlled {
                Value::Null
            } else {
                match data.connection_profile_id.as_deref() {
                    Some(id) => Value::String(id.to_string()),
                    None => Value::Null,
                }
            },
            "imageProfileId": match data.image_profile_id.as_deref() {
                Some(id) => Value::String(id.to_string()),
                None => Value::Null,
            },
            "selectedSystemPromptId": match data.selected_system_prompt_id.as_deref() {
                Some(id) => Value::String(id.to_string()),
                None => Value::Null,
            },
            "displayOrder": i as i64,
            "isActive": true,
        });
        built.push(participant);

        // Collect tags (first-seen order, v4's Set).
        if let Some(char_tags) = character.get("tags").and_then(Value::as_array) {
            for t in char_tags {
                if let Some(tag) = t.as_str() {
                    if !tags.iter().any(|x| x == tag) {
                        tags.push(tag.to_string());
                    }
                }
            }
        }

        // First imageProfileId (legacy support).
        if first_image_profile_id.is_none() {
            if let Some(ipid) = data.image_profile_id.as_deref() {
                first_image_profile_id = Some(ipid.to_string());
            }
        }

        if !is_user_controlled {
            candidates.push(LlmCandidate {
                character_id: character_id.to_string(),
                selected_system_prompt_id: data.selected_system_prompt_id.clone(),
                talkativeness: character
                    .get("talkativeness")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.5),
            });
        } else if first_user_character_id.is_none() {
            first_user_character_id = Some(character_id.to_string());
        }
    }

    if candidates.is_empty() {
        return Err(HandleCreateError::BadRequest(
            "At least one LLM-controlled CHARACTER participant is required".to_string(),
        ));
    }

    let chosen = pick_weighted_by_talkativeness(&candidates, random01);

    Ok(BuiltParticipants {
        participants: built,
        tags,
        first_character_id: chosen.character_id.clone(),
        first_user_character_id,
        first_selected_system_prompt_id: chosen.selected_system_prompt_id.clone(),
        first_image_profile_id,
    })
}

/// v4 `pickWeightedByTalkativeness` — weighted-random opener selection. A single
/// injected `random01` is reused for both the zero-total-weight uniform pick and
/// the weighted scan (the spine's `random01` convention).
fn pick_weighted_by_talkativeness(candidates: &[LlmCandidate], random01: f64) -> &LlmCandidate {
    let total: f64 = candidates.iter().map(|c| c.talkativeness).sum();
    if total <= 0.0 {
        let idx = ((random01 * candidates.len() as f64).floor() as usize)
            .min(candidates.len().saturating_sub(1));
        return &candidates[idx];
    }
    let r = random01 * total;
    let mut cumulative = 0.0;
    for c in candidates {
        cumulative += c.talkativeness;
        if r < cumulative {
            return c;
        }
    }
    &candidates[candidates.len() - 1]
}

// ============================================================================
// Scenario / staff seed phase (v4 `createInitialMessagesScenarioAndStaff`)
// ============================================================================

/// v4 `writeSystemPromptMessage`: the SYSTEM prompt at the head of the chat.
fn write_system_prompt_message(
    main: &Connection,
    context: &ChatContext,
    chat_id: &str,
) -> Result<(), DbError> {
    let event: ChatEventInput = serde_json::from_value(json!({
        "type": "message",
        "id": uuid::Uuid::new_v4().to_string(),
        "role": "SYSTEM",
        "content": context.system_prompt,
        "attachments": [],
        "createdAt": now_iso(),
    }))
    .map_err(|e| DbError::Key(format!("system message marshal: {e}")))?;
    ChatMessagesRepository::new(main).add_message(chat_id, &event)
}

/// v4 `createInitialMessagesScenarioAndStaff`: the Prospero / Host / Aurora seed
/// whispers + (unless skipped) the auto-generated first character message. Every
/// step is best-effort (v4's per-block try/catch) — a failure never breaks
/// creation.
#[allow(clippy::too_many_arguments)]
async fn create_initial_messages_scenario_and_staff<EMB, CMP, STR>(
    db: &Db,
    main: &Connection,
    mount: &Connection,
    llm_logs: Option<&Connection>,
    deps: &ChatCreateDeps<'_, EMB, CMP, STR>,
    context: &ChatContext,
    participants: &[Value],
    chat_id: &str,
    project_id: Option<&str>,
    scenario_text: Option<&str>,
    skip_first_message: bool,
) where
    EMB: EmbeddingProvider + Send + Sync,
    CMP: CompletionProvider + Send + Sync,
    STR: StreamingCompletionProvider + Send + Sync,
{
    let user_id = SINGLE_USER_ID;

    // Prospero project-and-general context whisper.
    let project_context =
        project_id.and_then(|pid| load_prospero_project_context(main, mount, pid));
    let general_context = load_prospero_general_context(main, mount);
    if project_context.is_some() || general_context.is_some() {
        let _ = post_prospero_context_announcement(
            db,
            ProsperoContextAnnouncement {
                chat_id: chat_id.to_string(),
                project: project_context,
                general: general_context,
            },
        )
        .await;
    }

    // Prospero group-context whispers — reload the chat for persisted ids.
    if let Ok(Some(seeded)) = chats_read::find_by_id(main, chat_id) {
        if let Some(chat_participants) = seeded.get("participants").and_then(Value::as_array) {
            for participant in chat_participants {
                if participant.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                    continue;
                }
                let Some(cid) = participant.get("characterId").and_then(Value::as_str) else {
                    continue;
                };
                if participant.get("status").and_then(Value::as_str) == Some("removed") {
                    continue;
                }
                let Some(pid) = participant.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let _ = post_prospero_group_context_whisper(
                    db,
                    ProsperoGroupContextWhisper {
                        chat_id: chat_id.to_string(),
                        target_participant_id: pid.to_string(),
                        character_id: cid.to_string(),
                    },
                )
                .await;
            }
        }
    }

    // Host scenario announcement.
    if let Some(text) = scenario_text.filter(|s| !js_trim(s).is_empty()) {
        let _ = post_host_scenario_announcement(
            db,
            HostScenarioAnnouncement {
                chat_id: chat_id.to_string(),
                scenario_text: text.to_string(),
            },
        )
        .await;
    }

    // Host user-character announcement.
    let has_user_participant = participants.iter().any(|p| {
        p.get("type").and_then(Value::as_str) == Some("CHARACTER")
            && p.get("controlledBy").and_then(Value::as_str) == Some("user")
    });
    if let Some(uc) = context.user_character.as_ref() {
        if has_user_participant {
            let _ = post_host_user_character_announcement(
                db,
                HostUserCharacterAnnouncement {
                    chat_id: chat_id.to_string(),
                    user_character_name: uc.name.clone(),
                    user_character_description: uc.description.clone(),
                },
            )
            .await;
        }
    }

    // Host add announcements (multi-character only).
    let llm_char_participants: Vec<&Value> = participants
        .iter()
        .filter(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("controlledBy").and_then(Value::as_str) != Some("user")
                && p.get("characterId").and_then(Value::as_str).is_some()
        })
        .collect();
    if llm_char_participants.len() > 1 {
        for participant in &llm_char_participants {
            let cid = participant
                .get("characterId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let character = characters_read::find_by_id(main, mount, cid).ok().flatten();
            let (Some(character), Some(pid)) =
                (character, participant.get("id").and_then(Value::as_str))
            else {
                continue;
            };
            let _ = post_host_add_announcement(
                db,
                HostAddAnnouncement {
                    chat_id: chat_id.to_string(),
                    character: HostCharacter {
                        name: character
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        description: character
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        character_document_mount_point_id: character
                            .get("characterDocumentMountPointId")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    },
                    participant_id: pid.to_string(),
                    initial_status: participant
                        .get("status")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                },
            )
            .await;
        }
    }

    // Aurora opening outfit whispers + avatar generation, per character.
    let equipped_project_mount_point_ids =
        resolve_project_mount_point_ids_for_chat(main, mount, chat_id);
    let outfits = ChatOutfitsRepository::new(main);
    let docs = DocMountDocumentsRepository::new(mount);
    for participant in participants {
        if participant.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            continue;
        }
        let Some(character_id) = participant.get("characterId").and_then(Value::as_str) else {
            continue;
        };
        // v4's per-character try/catch — a failure only skips this character.
        let outcome = post_opening_outfit_and_avatar(
            db,
            main,
            mount,
            &outfits,
            &docs,
            chat_id,
            character_id,
            &equipped_project_mount_point_ids,
        )
        .await;
        let _ = outcome;
    }

    if skip_first_message {
        return;
    }

    // Auto-generated first character message.
    let mut first_message_content = js_trim(&context.first_message).to_string();
    if first_message_content.is_empty() {
        first_message_content = auto_generate_first_message(
            db,
            main,
            deps,
            context,
            participants,
            chat_id,
            project_id,
            llm_logs,
        )
        .await;
    }
    if first_message_content.is_empty() {
        first_message_content = match context.user_character.as_ref() {
            Some(uc) => format!(
                "Hello, {}! I'm {}. What's on your mind today?",
                uc.name,
                context
                    .character
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ),
            None => format!(
                "Hello there! I'm {}. It's great to meet you. What's on your mind today?",
                context
                    .character
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ),
        };
    }

    let character_id = context.character.get("id").and_then(Value::as_str);
    let first_participant = participants
        .iter()
        .find(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("characterId").and_then(Value::as_str) == character_id
                && p.get("controlledBy").and_then(Value::as_str) != Some("user")
        })
        .or_else(|| {
            participants.iter().find(|p| {
                p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                    && p.get("controlledBy").and_then(Value::as_str) != Some("user")
            })
        });
    let participant_id = first_participant
        .and_then(|p| p.get("id").and_then(Value::as_str))
        .map(str::to_string);

    let mut msg = json!({
        "type": "message",
        "id": uuid::Uuid::new_v4().to_string(),
        "role": "ASSISTANT",
        "content": first_message_content,
        "attachments": [],
        "createdAt": now_iso(),
    });
    if let Some(pid) = participant_id {
        msg.as_object_mut()
            .unwrap()
            .insert("participantId".into(), Value::String(pid));
    }
    if let Ok(event) = serde_json::from_value::<ChatEventInput>(msg) {
        let _ = ChatMessagesRepository::new(main).add_message(chat_id, &event);
    }
    let _ = user_id;
}

/// The per-character Aurora opening-outfit whisper + avatar trigger (v4 L456-517
/// inner body). Errors are swallowed by the caller's per-character loop.
#[allow(clippy::too_many_arguments)]
async fn post_opening_outfit_and_avatar(
    db: &Db,
    main: &Connection,
    mount: &Connection,
    outfits: &ChatOutfitsRepository<'_>,
    docs: &DocMountDocumentsRepository<'_>,
    chat_id: &str,
    character_id: &str,
    equipped_project_mount_point_ids: &[String],
) -> Result<(), DbError> {
    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Ok(());
    };
    let Some(equipped_slots) = outfits.get_equipped_outfit_for_character(chat_id, character_id)?
    else {
        return Ok(());
    };

    let mut item_ids: Vec<String> = Vec::new();
    for slot in ["top", "bottom", "footwear", "accessories"] {
        for id in slot_ids(&equipped_slots, slot) {
            item_ids.push(id);
        }
    }
    let equipped_items = if item_ids.is_empty() {
        Vec::new()
    } else {
        wardrobe_read::find_by_ids_for_character(
            main,
            docs,
            character_id,
            &item_ids,
            equipped_project_mount_point_ids,
        )?
    };
    let mut title_by_id: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for item in &equipped_items {
        if let (Some(id), Some(title)) = (
            item.get("id").and_then(Value::as_str),
            item.get("title").and_then(Value::as_str),
        ) {
            title_by_id.insert(id.to_string(), title.to_string());
        }
    }
    let titles_for = |slot: &str| -> Vec<String> {
        slot_ids(&equipped_slots, slot)
            .into_iter()
            .filter_map(|id| title_by_id.get(&id).cloned())
            .collect()
    };
    let outfit = OutfitSlotValues {
        top: titles_for("top"),
        bottom: titles_for("bottom"),
        footwear: titles_for("footwear"),
        accessories: titles_for("accessories"),
    };

    let character_name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let _ = post_opening_outfit_whisper(db, chat_id, &character_name, &outfit).await;

    trigger_avatar_generation_if_enabled(
        db,
        &AvatarGenerationParams {
            user_id: SINGLE_USER_ID.to_string(),
            chat_id: chat_id.to_string(),
            character_id: character_id.to_string(),
            image_profile_id_override: None,
            equipped_slots_override: None,
        },
    )
    .await;
    Ok(())
}

// ============================================================================
// autoGenerateFirstMessage (v4 L578-829)
// ============================================================================

#[allow(clippy::too_many_arguments)]
async fn auto_generate_first_message<EMB, CMP, STR>(
    db: &Db,
    main: &Connection,
    deps: &ChatCreateDeps<'_, EMB, CMP, STR>,
    context: &ChatContext,
    participants: &[Value],
    chat_id: &str,
    project_id: Option<&str>,
    llm_logs: Option<&Connection>,
) -> String
where
    EMB: EmbeddingProvider + Send + Sync,
    CMP: CompletionProvider + Send + Sync,
    STR: StreamingCompletionProvider + Send + Sync,
{
    let user_id = SINGLE_USER_ID;
    let character_id = context
        .character
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // Find the responding participant (matching char, lowest displayOrder; else
    // first CHARACTER by displayOrder).
    let display_order = |p: &&Value| p.get("displayOrder").and_then(Value::as_i64).unwrap_or(0);
    let mut matching: Vec<&Value> = participants
        .iter()
        .filter(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("characterId").and_then(Value::as_str) == Some(character_id.as_str())
        })
        .collect();
    matching.sort_by_key(|p| display_order(p));
    let participant = matching.first().copied().or_else(|| {
        let mut all: Vec<&Value> = participants
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
            .collect();
        all.sort_by_key(|p| display_order(p));
        all.first().copied()
    });

    let Some(participant) = participant else {
        return String::new();
    };
    let Some(connection_profile_id) = participant
        .get("connectionProfileId")
        .and_then(Value::as_str)
    else {
        return String::new();
    };

    let connection_profile = match connection_profiles::find_by_id(main, connection_profile_id) {
        Ok(Some(p)) => p,
        _ => return String::new(),
    };

    let mut api_key = String::new();
    if let Some(api_key_id) = connection_profile.get("apiKeyId").and_then(Value::as_str) {
        match api_keys::find_by_id(main, api_key_id) {
            Ok(Some(k)) => api_key = k.key_value,
            _ => return String::new(),
        }
    }

    let parameters = connection_profile
        .get("parameters")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));

    // Build the first-message context (swallow errors).
    let mut greeting_memories: Vec<GreetingMemory> = Vec::new();
    let mut greeting_project: Option<GreetingProjectContext> = None;
    let participant_inputs: Vec<ChatParticipantInput> = participants
        .iter()
        .map(|p| ChatParticipantInput {
            participant_type: p
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("CHARACTER")
                .to_string(),
            character_id: p
                .get("characterId")
                .and_then(Value::as_str)
                .map(str::to_string),
            controlled_by: p
                .get("controlledBy")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect();
    if let Ok(fmc) = build_first_message_context(
        db,
        deps.embedding,
        &character_id,
        &participant_inputs,
        &FirstMessageContextOptions {
            user_id: user_id.to_string(),
            project_id: project_id.map(str::to_string),
            embedding_profile_id: None,
            now_ms: deps.now_ms as f64,
        },
    )
    .await
    {
        greeting_memories = fmc
            .participant_memories
            .into_iter()
            .map(|m| GreetingMemory {
                about_character_name: m.about_character_name,
                summary: m.summary,
            })
            .collect();
        greeting_project = fmc.project_context.map(|pc| GreetingProjectContext {
            name: pc.name,
            description: pc.description,
            instructions: pc.instructions,
        });
    }

    // Recent-conversations block (swallow errors).
    let provider = connection_profile
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let model_name = connection_profile
        .get("modelName")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let max_context = connection_profile
        .get("maxContext")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| {
            // v4 `getModelContextLimit(provider, modelName)`. The built-in registry
            // carries no per-model context rows (only the hardcoded override table +
            // the provider default), so `model_info`/`fallback_pricing` are empty
            // here (a documented minor seam for this best-effort ramp).
            crate::model_context::get_model_context_limit(
                &provider,
                &model_name,
                &[],
                &[],
                Registry::built_in().default_context_window(&provider),
            )
        });
    let limit = calculate_recent_conversations_limit(Some(max_context));
    let recent_conversations_block =
        build_recent_conversations_block(main, &character_id, Some(chat_id), limit);

    let temperature = extract_number(parameters.get("temperature"));
    let max_tokens = extract_number(parameters.get("maxTokens")).map(|n| n as i64);
    let top_p = extract_number(parameters.get("topP"));
    let base_url = connection_profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::to_string);
    let profile_parameters = connection_profile
        .get("parameters")
        .filter(|v| v.is_object())
        .cloned();

    let make_log = || -> Option<GreetingLog<'_>> {
        if deps.greeting_log && llm_logs.is_some() {
            Some(GreetingLog {
                db,
                user_id,
                chat_id: Some(chat_id),
                log_context: LogContext::none(),
            })
        } else {
            None
        }
    };

    let base_request = || GreetingRequest {
        system_prompt: context.system_prompt.clone(),
        character_name: context
            .character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        provider: provider.clone(),
        model_name: model_name.clone(),
        api_key: api_key.clone(),
        base_url: base_url.clone(),
        temperature,
        max_tokens,
        top_p,
        profile_parameters: profile_parameters.clone(),
        participant_memories: Vec::new(),
        project_context: None,
        recent_conversations_block: None,
        character_id: Some(character_id.clone()),
    };

    let recent_block_opt = |block: &str| -> Option<String> {
        if block.is_empty() {
            None
        } else {
            Some(block.to_string())
        }
    };

    let mut content_filter_hit = false;

    // Attempt 1: full context (memories + project + recent block).
    {
        let mut r = base_request();
        r.participant_memories = greeting_memories.clone();
        r.project_context = greeting_project.clone();
        r.recent_conversations_block = recent_block_opt(&recent_conversations_block);
        match generate_greeting_message(deps.streaming, &r, make_log().as_ref()).await {
            Ok(res) => {
                if !res.content.is_empty() {
                    return res.content;
                }
                if res.content_filter_detected {
                    content_filter_hit = true;
                }
            }
            Err(_) => { /* swallowed */ }
        }
    }

    // Attempt 2: strip memories (they may be triggering the content filter).
    if !greeting_memories.is_empty() {
        let mut r = base_request();
        r.project_context = greeting_project.clone();
        r.recent_conversations_block = recent_block_opt(&recent_conversations_block);
        match generate_greeting_message(deps.streaming, &r, make_log().as_ref()).await {
            Ok(res) => {
                if !res.content.is_empty() {
                    return res.content;
                }
                if res.content_filter_detected {
                    content_filter_hit = true;
                }
            }
            Err(_) => { /* swallowed */ }
        }
    }

    // Attempt 3: content-filter → Concierge uncensored reroute.
    if content_filter_hit {
        if let Some(content) = attempt_uncensored_reroute(
            main,
            deps,
            context,
            &connection_profile,
            &api_key,
            &parameters,
            &character_id,
            &greeting_memories,
            greeting_project.as_ref(),
            recent_block_opt(&recent_conversations_block).as_deref(),
            make_log().as_ref(),
        )
        .await
        {
            if !content.is_empty() {
                return content;
            }
        }
    }

    // Attempt 4: final plain retry (v4's 1s delay is host-timing — skipped).
    {
        let r = base_request();
        if let Ok(res) = generate_greeting_message(deps.streaming, &r, make_log().as_ref()).await {
            if !res.content.is_empty() {
                return res.content;
            }
        }
    }

    String::new()
}

/// v4 attempt-3 body (L748-804): resolve the Concierge uncensored provider and
/// retry the greeting through it. Every failure resolves to `None` (v4's
/// try/catch swallow).
#[allow(clippy::too_many_arguments)]
async fn attempt_uncensored_reroute<EMB, CMP, STR>(
    main: &Connection,
    deps: &ChatCreateDeps<'_, EMB, CMP, STR>,
    context: &ChatContext,
    connection_profile: &Value,
    api_key: &str,
    parameters: &Value,
    character_id: &str,
    memories: &[GreetingMemory],
    project: Option<&GreetingProjectContext>,
    recent_block: Option<&str>,
    log: Option<&GreetingLog<'_>>,
) -> Option<String>
where
    EMB: EmbeddingProvider + Send + Sync,
    CMP: CompletionProvider + Send + Sync,
    STR: StreamingCompletionProvider + Send + Sync,
{
    let user_id = SINGLE_USER_ID;
    let chat_settings = chat_settings::find_by_user_id(main, user_id).ok().flatten();
    let global_settings = chat_settings
        .as_ref()
        .and_then(|s| s.get("dangerousContentSettings"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let resolved = resolve_dangerous_content_settings(global_settings, None);
    if resolved.settings.mode != "AUTO_ROUTE" {
        return None;
    }

    let original = route_profile_from_value(connection_profile);
    // `resolve_provider_for_dangerous_content` is generic over a `Sized`
    // resolver; wrap the `&dyn` so it monomorphizes.
    let resolver = DynResolver(deps.api_keys);
    let route = resolve_provider_for_dangerous_content(
        main,
        &resolver,
        &original,
        api_key,
        "AUTO_ROUTE",
        resolved.settings.uncensored_text_profile_id.as_deref(),
        user_id,
    );
    if !route.rerouted {
        return None;
    }

    // v4 reads the rerouted profile's own parameters — re-fetch by id.
    let uncensored_params = connection_profiles::find_by_id(main, &route.connection_profile.id)
        .ok()
        .flatten()
        .and_then(|p| p.get("parameters").cloned())
        .filter(|v| v.is_object());

    let ext =
        |obj: &Option<Value>, key: &str| extract_number(obj.as_ref().and_then(|o| o.get(key)));
    let temperature = ext(&uncensored_params, "temperature")
        .or_else(|| extract_number(parameters.get("temperature")));
    let max_tokens = ext(&uncensored_params, "maxTokens")
        .or_else(|| extract_number(parameters.get("maxTokens")))
        .map(|n| n as i64);
    let top_p = ext(&uncensored_params, "topP").or_else(|| extract_number(parameters.get("topP")));

    let request = GreetingRequest {
        system_prompt: context.system_prompt.clone(),
        character_name: context
            .character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        provider: route.connection_profile.provider.clone(),
        model_name: route.connection_profile.model_name.clone(),
        api_key: route.api_key.clone(),
        base_url: route.connection_profile.base_url.clone(),
        temperature,
        max_tokens,
        top_p,
        profile_parameters: None,
        participant_memories: memories.to_vec(),
        project_context: project.cloned(),
        recent_conversations_block: recent_block.map(str::to_string),
        character_id: Some(character_id.to_string()),
    };

    match generate_greeting_message(deps.streaming, &request, log).await {
        Ok(res) if !res.content.is_empty() => Some(res.content),
        _ => None,
    }
}

// ============================================================================
// Recent-conversations helpers (v4 lib/memory/memory-recap.ts:34-77)
// ============================================================================

/// v4 `rampLimit`: linear ramp `min`→`max` over `min_tokens`→`max_tokens` of
/// context, rounded. `None` context yields `max`.
fn ramp_limit(
    max_context: Option<i64>,
    min: i64,
    max: i64,
    min_tokens: i64,
    max_tokens: i64,
) -> i64 {
    let Some(ctx) = max_context else {
        return max;
    };
    if ctx <= min_tokens {
        return min;
    }
    if ctx >= max_tokens {
        return max;
    }
    let ratio = (ctx - min_tokens) as f64 / (max_tokens - min_tokens) as f64;
    // JS `Math.round` is half-up; for the positive range here Rust's
    // half-away-from-zero `round` is identical.
    (min as f64 + ratio * (max - min) as f64).round() as i64
}

/// v4 `calculateRecentConversationsLimit` — ramp 5→20 over 4K→32K.
pub fn calculate_recent_conversations_limit(max_context: Option<i64>) -> i64 {
    ramp_limit(max_context, 5, 20, 4000, 32000)
}

/// v4 `buildRecentConversationsBlock`.
pub fn build_recent_conversations_block(
    main: &Connection,
    character_id: &str,
    current_chat_id: Option<&str>,
    limit: i64,
) -> String {
    if limit <= 0 {
        return String::new();
    }
    let eligible =
        chats_read::find_recent_summarized_by_character(main, character_id, limit, current_chat_id)
            .unwrap_or_default();
    if eligible.is_empty() {
        return String::new();
    }
    let entries: Vec<String> = eligible
        .iter()
        .map(|c| {
            let title = c.get("title").and_then(Value::as_str).unwrap_or_default();
            let id = c.get("id").and_then(Value::as_str).unwrap_or_default();
            let summary = c
                .get("contextSummary")
                .and_then(Value::as_str)
                .unwrap_or_default();
            format!("#### {title} (`{id}`)\n{summary}")
        })
        .collect();
    format!("### Recent Conversations\n\n{}", entries.join("\n\n"))
}

// ============================================================================
// Small helpers
// ============================================================================

/// v4 `extractNumber`: a JSON number → itself; a JSON string → `Number(x)`
/// (`NaN` → `None`); else `None`.
fn extract_number(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(n)) => n.as_f64().filter(|f| !f.is_nan()),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok().filter(|f| !f.is_nan()),
        _ => None,
    }
}

/// A `Sized` adapter over a `&dyn ApiKeyResolver`, so the generic
/// `resolve_provider_for_dangerous_content<A: ApiKeyResolver>` (which takes
/// `&A`, `A: Sized`) accepts the deps' trait object.
struct DynResolver<'a>(&'a dyn ApiKeyResolver);
impl ApiKeyResolver for DynResolver<'_> {
    fn resolve(&self, api_key_id: &str, user_id: &str) -> Option<String> {
        self.0.resolve(api_key_id, user_id)
    }
}

/// Build a [`RouteProfile`] from a connection-profile Value (the private
/// `route_profile_from_value` in `provider_routing` is not exported).
fn route_profile_from_value(v: &Value) -> RouteProfile {
    let s = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    RouteProfile {
        id: s("id"),
        name: s("name"),
        provider: s("provider"),
        model_name: s("modelName"),
        base_url: v.get("baseUrl").and_then(Value::as_str).map(str::to_string),
    }
}

/// The string ids of one equipped-outfit slot (v4's `equippedSlots[slot]`),
/// dropping non-string / empty entries.
fn slot_ids(v: &Value, slot: &str) -> Vec<String> {
    v.get(slot)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// A JSON number-or-null from an `Option<i64>`.
fn opt_i64(v: Option<i64>) -> Value {
    match v {
        Some(n) => json!(n),
        None => Value::Null,
    }
}

/// A JSON number-or-null from an `Option<f64>`.
fn opt_f64(v: Option<f64>) -> Value {
    match v {
        Some(n) => json!(n),
        None => Value::Null,
    }
}

/// Parse `Slots` out of a manual-mode selection's `slots` Value.
fn slots_from_value(v: &Value) -> Slots {
    let arr = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    Slots {
        top: arr("top"),
        bottom: arr("bottom"),
        footwear: arr("footwear"),
        accessories: arr("accessories"),
    }
}

/// v4 L1178-1207: build the outfit-selection list — explicit selections plus a
/// `default` backfill for uncovered character participants, or (no explicit)
/// `default` for every character participant.
fn build_outfit_selections(
    req: &ChatCreateRequest,
    participants: &[Value],
) -> Vec<OutfitSelection> {
    let char_ids: Vec<String> = participants
        .iter()
        .filter(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
        .filter_map(|p| {
            p.get("characterId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();

    match req.outfit_selections.as_ref().filter(|v| !v.is_empty()) {
        Some(explicit) => {
            let mut selections: Vec<OutfitSelection> = Vec::new();
            let mut covered: Vec<String> = Vec::new();
            for sel in explicit {
                let character_id = sel
                    .get("characterId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                covered.push(character_id.clone());
                selections.push(OutfitSelection {
                    character_id,
                    mode: sel
                        .get("mode")
                        .and_then(Value::as_str)
                        .unwrap_or("default")
                        .to_string(),
                    slots: sel
                        .get("slots")
                        .filter(|v| v.is_object())
                        .map(slots_from_value),
                });
            }
            for cid in &char_ids {
                if !covered.iter().any(|c| c == cid) {
                    selections.push(OutfitSelection {
                        character_id: cid.clone(),
                        mode: "default".to_string(),
                        slots: None,
                    });
                }
            }
            selections
        }
        None => char_ids
            .into_iter()
            .map(|character_id| OutfitSelection {
                character_id,
                mode: "default".to_string(),
                slots: None,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_limit_boundaries() {
        // null context → max.
        assert_eq!(calculate_recent_conversations_limit(None), 20);
        // <= min_tokens → min.
        assert_eq!(calculate_recent_conversations_limit(Some(4000)), 5);
        assert_eq!(calculate_recent_conversations_limit(Some(1000)), 5);
        // >= max_tokens → max.
        assert_eq!(calculate_recent_conversations_limit(Some(32000)), 20);
        assert_eq!(calculate_recent_conversations_limit(Some(64000)), 20);
        // Midpoint (18000) → 5 + 0.5*(15) = 12.5 → round → 13 (half-up).
        assert_eq!(calculate_recent_conversations_limit(Some(18000)), 13);
    }

    #[test]
    fn pick_weighted_extremes() {
        let candidates = vec![
            LlmCandidate {
                character_id: "a".into(),
                selected_system_prompt_id: None,
                talkativeness: 1.0,
            },
            LlmCandidate {
                character_id: "b".into(),
                selected_system_prompt_id: None,
                talkativeness: 1.0,
            },
            LlmCandidate {
                character_id: "c".into(),
                selected_system_prompt_id: None,
                talkativeness: 1.0,
            },
        ];
        // random01 = 0.0 → r = 0 < first cumulative (1.0) → first.
        assert_eq!(
            pick_weighted_by_talkativeness(&candidates, 0.0).character_id,
            "a"
        );
        // random01 = 0.99 → r = 2.97, cumulative reaches 3.0 → last.
        assert_eq!(
            pick_weighted_by_talkativeness(&candidates, 0.99).character_id,
            "c"
        );
    }

    #[test]
    fn pick_weighted_zero_total_uses_uniform() {
        let candidates = vec![
            LlmCandidate {
                character_id: "a".into(),
                selected_system_prompt_id: None,
                talkativeness: 0.0,
            },
            LlmCandidate {
                character_id: "b".into(),
                selected_system_prompt_id: None,
                talkativeness: 0.0,
            },
        ];
        // total <= 0 → uniform index = floor(random01 * len).
        assert_eq!(
            pick_weighted_by_talkativeness(&candidates, 0.0).character_id,
            "a"
        );
        assert_eq!(
            pick_weighted_by_talkativeness(&candidates, 0.75).character_id,
            "b"
        );
        // A random01 of 1.0 would floor to len — clamped to the last.
        assert_eq!(
            pick_weighted_by_talkativeness(&candidates, 1.0).character_id,
            "b"
        );
    }

    #[test]
    fn extract_number_forms() {
        assert_eq!(extract_number(Some(&json!(0.7))), Some(0.7));
        assert_eq!(extract_number(Some(&json!("0.5"))), Some(0.5));
        assert_eq!(extract_number(Some(&json!("nope"))), None);
        assert_eq!(extract_number(Some(&Value::Null)), None);
        assert_eq!(extract_number(None), None);
    }
}
