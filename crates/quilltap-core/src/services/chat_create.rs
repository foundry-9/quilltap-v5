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
    wardrobe_read, DbError,
};
use crate::enclave::cron;
use crate::enclave::lifecycle::{self, LifecycleDeps, StartManualRunResult};
use crate::jsstr::js_trim;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::model::stream::{StreamError, StreamingCompletionProvider};
use crate::provider_manifest::Registry;
use crate::services::avatar_generation::{
    trigger_avatar_generation_if_enabled, AvatarGenerationParams,
};
use crate::services::chat_continuation::apply_chat_continuation;
use crate::services::chat_enrichment::{enrich_participant_summary, EnrichedParticipantSummary};
use crate::services::chat_initialize::{build_chat_context, ChatContext};
use crate::services::chat_participants::VALIDATION_ERROR;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::creation_progress::CreationProgressEmitter;
use crate::services::dangerous_content::chat_override::{
    should_use_uncensored_route, ConciergeState,
};
use crate::services::dangerous_content::manual_flip::{
    apply_concierge_flip, RealConciergeAnnouncer,
};
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
use crate::services::scenario_selection::{
    resolve_scenario_selection, ResolveScenarioSelectionOptions, ScenarioSelectionFields,
};
use crate::services::system_prompt_compiler::compile_all_identity_stacks;
use crate::tools::wardrobe_shared::resolve_project_mount_point_ids_for_chat;
use crate::wardrobe::Slots;

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

impl ChatCreateParticipant {
    /// The LENIENT read of one raw participant object — see
    /// [`ChatCreateRequest::from_raw`] for why nothing here may fail.
    fn from_raw(v: &Value) -> Self {
        let Some(obj) = v.as_object() else {
            return Self::default();
        };
        let s = |k: &str| obj.get(k).and_then(|v| v.as_str()).map(str::to_string);
        ChatCreateParticipant {
            kind: s("type").unwrap_or_else(default_participant_type),
            character_id: s("characterId"),
            connection_profile_id: s("connectionProfileId"),
            image_profile_id: s("imageProfileId"),
            controlled_by: s("controlledBy"),
            selected_system_prompt_id: s("selectedSystemPromptId"),
        }
    }
}

fn default_participant_type() -> String {
    "CHARACTER".to_string()
}

/// The tri-state read of a `.optional()` / `.nullable().optional()` key:
/// absent → `None`, explicit null → `Some(None)`, a value → `Some(Some(v))`.
fn raw_tri_state(obj: &Map<String, Value>, key: &str) -> Option<Option<Value>> {
    obj.get(key)
        .map(|v| if v.is_null() { None } else { Some(v.clone()) })
}

/// A JS-integer read: `1` and `1.0` are both integers to Zod's `z.number()
/// .int()`, so an integral float must reach the typed view rather than being
/// dropped (`vb_ok_freshness_window_integral_float` stores `1`). Anything
/// non-integral, out of the safe range, or of the wrong type is left `None` —
/// the validation stage has already refused every such body.
fn raw_js_int(obj: &Map<String, Value>, key: &str) -> Option<i64> {
    let n = obj.get(key)?.as_f64()?;
    if !n.is_finite() || n.fract() != 0.0 || n.abs() > MAX_SAFE_INTEGER {
        return None;
    }
    Some(n as i64)
}

/// The chat-creation request (v4 `createChatSchema`).
///
/// ⚠ The typed fields below are a LENIENT view over [`Self::raw`], built by
/// [`Self::from_raw`] — the decode can never fail. That is deliberate and
/// load-bearing: v4 Zod-parses the whole body inside the route handler, so a
/// wrong-typed field must answer the route's `400 Validation error` envelope
/// (P4.78 / dogfood finding #115), and a serde failure at the transport
/// boundary would answer the host's `invalid chatCreate request: …` instead
/// (the P4.60 wrong-type-collapse convention; the shape P4.73 fixed for
/// `conciergeState` / `roleplayTemplateId` one field at a time, generalized
/// here to the whole body).
///
/// The typed view is therefore only meaningful for a body
/// [`validate_create_body`] has ACCEPTED — which is why `handle_create` runs
/// that stage over [`Self::raw`] as its first statement, before it reads a
/// single typed field. A dropped value here is unreachable in production, not
/// silently tolerated.
#[derive(Debug, Clone, Default)]
pub struct ChatCreateRequest {
    /// The body exactly as it arrived — what the validation stage reads. The
    /// typed fields cannot express "present but the wrong type", and that
    /// distinction is the whole of v4's refusal.
    pub raw: Map<String, Value>,
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
    ///
    /// RAW `Value`, not `String`: v4 Zod-parses it, so a present-but-WRONG-TYPED
    /// value must reach the handler's refusal and answer v4's `Validation
    /// error` 400 — a boundary that refused it with the transport's own
    /// `Invalid request: …` would never get there (the P4.60
    /// wrong-type-collapse convention; MEASURED at `rt_wrong_type_400`).
    pub roleplay_template_id: Option<Option<Value>>,
    /// v4 [`303288fb4`] `conciergeState:
    /// z.enum(['monitored','flagged','vouched','uncensored']).optional()` — the
    /// per-chat Concierge state to set at creation, using the same enum as the
    /// sidebar's PUT `conciergeState`. Omitted or `'monitored'` → the chat is
    /// created Monitored exactly as before (no write, no announcement). Any
    /// other value is applied through
    /// [`apply_concierge_flip`](crate::services::dangerous_content::manual_flip::apply_concierge_flip)
    /// after the system-prompt message and before any staff announcement or
    /// greeting, so the Concierge's bubble sits where the history says the state
    /// was set and the opening greeting is generated under the chosen state.
    ///
    /// ⚠ `.optional()` is NOT nullable, so an explicit JSON `null` REJECTS —
    /// the same arm [`Self::timestamp_config`] carries, not
    /// [`Self::roleplay_template_id`]'s deliberate-null. The double `Option`
    /// keeps the three arms expressible (absent → `None`, null → `Some(None)`,
    /// string → `Some(Some(s))`); the string is validated against the four wire
    /// values inside `handle_create` so an unknown one answers v4's route-level
    /// `Validation error` 400 rather than failing the dispatch decode with a
    /// different envelope.
    ///
    /// RAW `Value` for the same reason as [`Self::roleplay_template_id`]: a
    /// wrong TYPE must reach the handler, not fail the decode (MEASURED at
    /// `cs_wrong_type_400`).
    pub concierge_state: Option<Option<Value>>,
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

impl ChatCreateRequest {
    /// Build the typed view from a raw body, dropping anything of the wrong
    /// shape rather than failing. See the struct's note: the drops are
    /// unreachable for a body [`validate_create_body`] accepted, and every
    /// body it refuses never reaches a typed read.
    pub fn from_raw(raw: Map<String, Value>) -> Self {
        let s = |k: &str| raw.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let b = |k: &str| raw.get(k).and_then(|v| v.as_bool());
        ChatCreateRequest {
            participants: raw
                .get("participants")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().map(ChatCreateParticipant::from_raw).collect())
                .unwrap_or_default(),
            title: s("title"),
            scenario: s("scenario"),
            scenario_id: s("scenarioId"),
            project_scenario_path: s("projectScenarioPath"),
            general_scenario_path: s("generalScenarioPath"),
            group_scenario_path: s("groupScenarioPath"),
            group_scenario_group_id: s("groupScenarioGroupId"),
            timestamp_config: raw_tri_state(&raw, "timestampConfig"),
            project_id: s("projectId"),
            image_profile_id: s("imageProfileId"),
            roleplay_template_id: raw_tri_state(&raw, "roleplayTemplateId"),
            concierge_state: raw_tri_state(&raw, "conciergeState"),
            outfit_selections: raw
                .get("outfitSelections")
                .and_then(|v| v.as_array())
                .cloned(),
            avatar_generation_enabled: b("avatarGenerationEnabled"),
            continuation_from_chat_id: s("continuationFromChatId"),
            progress_id: s("progressId"),
            chat_type: s("chatType"),
            schedule_cron: s("scheduleCron"),
            schedule_freshness_window_ms: raw_js_int(&raw, "scheduleFreshnessWindowMs"),
            budget_max_turns: raw_js_int(&raw, "budgetMaxTurns"),
            budget_max_tokens: raw_js_int(&raw, "budgetMaxTokens"),
            budget_max_wall_clock_ms: raw_js_int(&raw, "budgetMaxWallClockMs"),
            budget_estimated_spend_cap_usd: raw
                .get("budgetEstimatedSpendCapUSD")
                .and_then(|v| v.as_f64()),
            run_visibility: s("runVisibility"),
            run_destructive_tools_allowed: b("runDestructiveToolsAllowed"),
            budget_exclude_cache_hits: b("budgetExcludeCacheHits"),
            raw,
        }
    }
}

impl<'de> Deserialize<'de> for ChatCreateRequest {
    /// Infallible for any JSON OBJECT (a non-object body is still a decode
    /// error — v4's `req.json()` would have thrown first). See
    /// [`ChatCreateRequest::from_raw`].
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        Ok(ChatCreateRequest::from_raw(Map::deserialize(de)?))
    }
}

// ============================================================================
// The whole-body validation stage (v4 `createChatSchema.parse`)
// ============================================================================

/// v4 zod 4's issue objects for `createChatSchema`, in the key order
/// `JSON.stringify` emits — the `image_gen::lora_validation::LoraZodIssue`
/// idiom (P4.D138), widened for this schema's extra codes. Untagged variants
/// rather than one struct of optional keys: `Option` skipping cannot reorder,
/// and each code puts a different key first (`invalid_type` leads with
/// `expected`, the size issues with `origin`, `invalid_value` with `code`, and
/// the safe-integer bound with `code` before `origin`).
///
/// Every shape here was MEASURED against v4's real route at the `f699da6f6`
/// pin, not inferred from Zod's source — see the corpus arms named in each
/// constructor.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum CreateZodIssue {
    /// `z.string()` / `z.boolean()` / `z.number()` / `z.array()` / `z.object()`.
    InvalidType {
        expected: &'static str,
        code: &'static str,
        path: Vec<Value>,
        message: String,
    },
    /// `z.number().int()`'s own type failure — Zod 4 reports the `safeint`
    /// format alongside `expected: "int"` (`vb_freshness_window_fractional`).
    InvalidIntType {
        expected: &'static str,
        format: &'static str,
        code: &'static str,
        path: Vec<Value>,
        message: String,
    },
    /// `z.enum([...])` and `z.literal(...)` — both report `invalid_value`, and
    /// both do so for a wrong TYPE as well as an out-of-domain value
    /// (`cs_wrong_type_400`, `vb_participant_type_persona`).
    InvalidValue {
        code: &'static str,
        values: Vec<Value>,
        path: Vec<Value>,
        message: String,
    },
    /// `z.uuid()` — the pattern is echoed verbatim in the issue
    /// (`vb_participant_character_id_not_uuid`).
    InvalidFormat {
        origin: &'static str,
        code: &'static str,
        format: &'static str,
        pattern: &'static str,
        path: Vec<Value>,
        message: &'static str,
    },
    TooSmall {
        origin: &'static str,
        code: &'static str,
        minimum: Value,
        inclusive: bool,
        path: Vec<Value>,
        message: String,
    },
    TooBig {
        origin: &'static str,
        code: &'static str,
        maximum: Value,
        inclusive: bool,
        path: Vec<Value>,
        message: String,
    },
    /// The safe-integer ceiling of `z.number().int()`, which carries Zod's
    /// `note` and puts `code` before `origin` (`vb_budget_max_turns_unsafe_int`).
    TooBigInt {
        code: &'static str,
        maximum: Value,
        note: &'static str,
        origin: &'static str,
        inclusive: bool,
        path: Vec<Value>,
        message: String,
    },
    /// The safe-integer floor — the mirror of [`Self::TooBigInt`]. Measured at
    /// `vb_budget_max_turns_unsafe_negative`, which also proves the bound does
    /// NOT abort the following `positive()` check: that body answers TWO issues.
    TooSmallInt {
        code: &'static str,
        minimum: Value,
        note: &'static str,
        origin: &'static str,
        inclusive: bool,
        path: Vec<Value>,
        message: String,
    },
}

/// Zod 4's `z.uuid()` pattern, echoed verbatim in every `invalid_format` issue.
const ZOD_UUID_PATTERN: &str = "/^([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-8][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}|00000000-0000-0000-0000-000000000000|ffffffff-ffff-ffff-ffff-ffffffffffff)$/";

/// JS `Number.MAX_SAFE_INTEGER` — the bound `z.number().int()` reports as
/// `format: "safeint"`.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// The `details` array of v4's `validationError(err)` body, ready for
/// [`CoreError::details`](crate::api::types::CoreError::details).
pub fn create_issue_details(issues: &[CreateZodIssue]) -> Value {
    serde_json::to_value(issues).unwrap_or(Value::Null)
}

/// v4 `util.parsedType` — the received-type word in an `invalid_type` message.
/// The same table [`crate::api::chat_outfits::received`] carries; kept local
/// because this module renders whole issue objects rather than sentences.
fn parsed_type(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

fn invalid_type(expected: &'static str, path: Vec<Value>, got: Option<&Value>) -> CreateZodIssue {
    CreateZodIssue::InvalidType {
        expected,
        code: "invalid_type",
        path,
        message: format!(
            "Invalid input: expected {expected}, received {}",
            parsed_type(got)
        ),
    }
}

fn invalid_uuid(path: Vec<Value>) -> CreateZodIssue {
    CreateZodIssue::InvalidFormat {
        origin: "string",
        code: "invalid_format",
        format: "uuid",
        pattern: ZOD_UUID_PATTERN,
        path,
        message: "Invalid UUID",
    }
}

/// `z.enum([...])` — the message quotes every member and joins with `|`.
fn invalid_enum(values: &[&str], path: Vec<Value>) -> CreateZodIssue {
    CreateZodIssue::InvalidValue {
        code: "invalid_value",
        values: values.iter().map(|v| json!(v)).collect(),
        path,
        message: format!(
            "Invalid option: expected one of {}",
            values
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join("|")
        ),
    }
}

/// `z.literal(v)` — the same `invalid_value` code as an enum, a different
/// message (`Invalid input: expected "CHARACTER"`).
fn invalid_literal(value: &'static str, path: Vec<Value>) -> CreateZodIssue {
    CreateZodIssue::InvalidValue {
        code: "invalid_value",
        values: vec![json!(value)],
        path,
        message: format!("Invalid input: expected \"{value}\""),
    }
}

/// A path built from a schema key and (optionally) an array index / sub-key.
fn path(parts: &[Value]) -> Vec<Value> {
    parts.to_vec()
}

fn key(k: &str) -> Value {
    Value::String(k.to_string())
}

// ---------------------------------------------------------------------------
// The per-primitive checks. Each takes the value AS FOUND (`None` = the key was
// absent, which every `.optional()` / `.default()` field accepts) so the
// present-but-null arm stays distinguishable — `.optional()` is NOT nullable.
// ---------------------------------------------------------------------------

/// `z.string().optional()` (and, with `max`, `z.string().max(N).optional()`).
/// The `max` bound counts CODE POINTS, not UTF-16 units (Zod 4.5, P4.D158 §3) —
/// `vb_ok_project_scenario_path_500_astral` is the discriminator.
fn check_opt_string(
    v: Option<&Value>,
    at: Vec<Value>,
    max: Option<usize>,
    issues: &mut Vec<CreateZodIssue>,
) {
    let Some(v) = v else { return };
    let Some(s) = v.as_str() else {
        issues.push(invalid_type("string", at, Some(v)));
        return;
    };
    if let Some(max) = max {
        if s.chars().count() > max {
            issues.push(CreateZodIssue::TooBig {
                origin: "string",
                code: "too_big",
                maximum: json!(max),
                inclusive: true,
                path: at,
                message: format!("Too big: expected string to have <={max} characters"),
            });
        }
    }
}

/// `z.string().nullable().optional()` — absent OR null is fine.
fn check_nullable_opt_string(v: Option<&Value>, at: Vec<Value>, issues: &mut Vec<CreateZodIssue>) {
    match v {
        None | Some(Value::Null) => {}
        Some(other) => check_opt_string(Some(other), at, None, issues),
    }
}

/// `z.uuid().optional()`.
fn check_opt_uuid(v: Option<&Value>, at: Vec<Value>, issues: &mut Vec<CreateZodIssue>) {
    let Some(v) = v else { return };
    check_required_uuid(Some(v), at, issues);
}

/// `z.uuid()` — required. An absent key is `received undefined`, not a format
/// failure (`vb_participant_character_id_missing`).
fn check_required_uuid(v: Option<&Value>, at: Vec<Value>, issues: &mut Vec<CreateZodIssue>) {
    match v.and_then(|v| v.as_str()) {
        Some(s) if crate::api::chat_outfits::is_zod_uuid(s) => {}
        Some(_) => issues.push(invalid_uuid(at)),
        None => issues.push(invalid_type("string", at, v)),
    }
}

/// `z.boolean()` (optional, or with a `.default()` — both accept an absent key).
fn check_opt_bool(v: Option<&Value>, at: Vec<Value>, issues: &mut Vec<CreateZodIssue>) {
    let Some(v) = v else { return };
    if !v.is_boolean() {
        issues.push(invalid_type("boolean", at, Some(v)));
    }
}

/// `z.enum([...])` (optional, or with a `.default()`). A wrong TYPE reports
/// `invalid_value` exactly like an out-of-domain string does.
fn check_opt_enum(
    v: Option<&Value>,
    values: &[&str],
    at: Vec<Value>,
    issues: &mut Vec<CreateZodIssue>,
) {
    let Some(v) = v else { return };
    match v.as_str() {
        Some(s) if values.contains(&s) => {}
        _ => issues.push(invalid_enum(values, at)),
    }
}

/// `z.enum([...])` REQUIRED — an absent key reports the same `invalid_value`
/// (`vb_outfit_selection_mode_missing`).
fn check_required_enum(
    v: Option<&Value>,
    values: &[&str],
    at: Vec<Value>,
    issues: &mut Vec<CreateZodIssue>,
) {
    match v.and_then(|v| v.as_str()) {
        Some(s) if values.contains(&s) => {}
        _ => issues.push(invalid_enum(values, at)),
    }
}

/// The shared body of `z.number().int()`: the number check, then the integer
/// check (which ABORTS — `vb_budget_max_turns_negative_fractional` answers one
/// issue, not two), then the safe-integer bounds (which do NOT abort — see
/// [`CreateZodIssue::TooSmallInt`]). Returns the value when it is a number, so
/// the caller can run its own `positive()` / `min(1)` bound after.
fn check_int_bounds(v: &Value, at: &[Value], issues: &mut Vec<CreateZodIssue>) -> Option<f64> {
    let Some(n) = v.as_f64() else {
        issues.push(invalid_type("number", path(at), Some(v)));
        return None;
    };
    if !n.is_finite() || n.fract() != 0.0 {
        issues.push(CreateZodIssue::InvalidIntType {
            expected: "int",
            format: "safeint",
            code: "invalid_type",
            path: path(at),
            message: "Invalid input: expected int, received number".to_string(),
        });
        return None;
    }
    if n > MAX_SAFE_INTEGER {
        issues.push(CreateZodIssue::TooBigInt {
            code: "too_big",
            maximum: json!(MAX_SAFE_INTEGER as i64),
            note: "Integers must be within the safe integer range.",
            origin: "int",
            inclusive: true,
            path: path(at),
            message: format!("Too big: expected int to be <={}", MAX_SAFE_INTEGER as i64),
        });
    } else if n < -MAX_SAFE_INTEGER {
        issues.push(CreateZodIssue::TooSmallInt {
            code: "too_small",
            minimum: json!(-(MAX_SAFE_INTEGER as i64)),
            note: "Integers must be within the safe integer range.",
            origin: "int",
            inclusive: true,
            path: path(at),
            message: format!(
                "Too small: expected int to be >=-{}",
                MAX_SAFE_INTEGER as i64
            ),
        });
    }
    Some(n)
}

/// `z.number().int().positive().optional()`. The `positive()` bound reports
/// `origin: "number"` (not `"int"`) — measured, not assumed.
fn check_opt_int_positive(v: Option<&Value>, at: Vec<Value>, issues: &mut Vec<CreateZodIssue>) {
    let Some(v) = v else { return };
    let Some(n) = check_int_bounds(v, &at, issues) else {
        return;
    };
    if n <= 0.0 {
        issues.push(CreateZodIssue::TooSmall {
            origin: "number",
            code: "too_small",
            minimum: json!(0),
            inclusive: false,
            path: at,
            message: "Too small: expected number to be >0".to_string(),
        });
    }
}

/// `z.number().int().min(1)` — `timestampConfig.intervalMinutes`. Same integer
/// machinery, an inclusive bound (`tc_bad_value_rejected`).
fn check_opt_int_min(
    v: Option<&Value>,
    minimum: i64,
    at: Vec<Value>,
    issues: &mut Vec<CreateZodIssue>,
) {
    let Some(v) = v else { return };
    let Some(n) = check_int_bounds(v, &at, issues) else {
        return;
    };
    if n < minimum as f64 {
        issues.push(CreateZodIssue::TooSmall {
            origin: "number",
            code: "too_small",
            minimum: json!(minimum),
            inclusive: true,
            path: at,
            message: format!("Too small: expected number to be >={minimum}"),
        });
    }
}

/// `z.number().positive().optional()` — no `int()`, so `1.5` is legal
/// (`vb_ok_spend_cap_fractional`).
fn check_opt_number_positive(v: Option<&Value>, at: Vec<Value>, issues: &mut Vec<CreateZodIssue>) {
    let Some(v) = v else { return };
    let Some(n) = v.as_f64() else {
        issues.push(invalid_type("number", at, Some(v)));
        return;
    };
    if n <= 0.0 {
        issues.push(CreateZodIssue::TooSmall {
            origin: "number",
            code: "too_small",
            minimum: json!(0),
            inclusive: false,
            path: at,
            message: "Too small: expected number to be >0".to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// The sub-schemas
// ---------------------------------------------------------------------------

/// v4 `createParticipantSchema`, in its declaration order (which is the order
/// Zod reports issues in).
fn check_participant(v: &Value, index: usize, issues: &mut Vec<CreateZodIssue>) {
    let idx = json!(index);
    let base = |k: &str| path(&[key("participants"), idx.clone(), key(k)]);
    let Some(obj) = v.as_object() else {
        issues.push(invalid_type(
            "object",
            path(&[key("participants"), idx]),
            Some(v),
        ));
        return;
    };
    match obj.get("type").and_then(|t| t.as_str()) {
        Some("CHARACTER") => {}
        _ => issues.push(invalid_literal("CHARACTER", base("type"))),
    }
    check_required_uuid(obj.get("characterId"), base("characterId"), issues);
    check_opt_uuid(
        obj.get("connectionProfileId"),
        base("connectionProfileId"),
        issues,
    );
    // Legacy, ignored by the handler — but still schema-checked by v4.
    check_opt_uuid(obj.get("imageProfileId"), base("imageProfileId"), issues);
    check_opt_enum(
        obj.get("controlledBy"),
        &["llm", "user"],
        base("controlledBy"),
        issues,
    );
    check_opt_uuid(
        obj.get("selectedSystemPromptId"),
        base("selectedSystemPromptId"),
        issues,
    );
}

/// v4 `TimestampConfigSchema` (`lib/schemas/settings.types.ts`). Every field
/// carries a `.default()` or is `.nullable().optional()`, so an absent key is
/// always fine; a PRESENT one is checked.
fn check_timestamp_config(obj: &Map<String, Value>, issues: &mut Vec<CreateZodIssue>) {
    let at = |k: &str| path(&[key("timestampConfig"), key(k)]);
    check_opt_enum(
        obj.get("mode"),
        &["NONE", "START_ONLY", "EVERY_MESSAGE", "EVERY_N_MINUTES"],
        at("mode"),
        issues,
    );
    check_opt_enum(
        obj.get("format"),
        &["ISO8601", "FRIENDLY", "DATE_ONLY", "TIME_ONLY", "CUSTOM"],
        at("format"),
        issues,
    );
    check_nullable_opt_string(obj.get("customFormat"), at("customFormat"), issues);
    check_opt_bool(obj.get("useFictionalTime"), at("useFictionalTime"), issues);
    check_nullable_opt_string(
        obj.get("fictionalBaseTimestamp"),
        at("fictionalBaseTimestamp"),
        issues,
    );
    check_nullable_opt_string(
        obj.get("fictionalBaseRealTime"),
        at("fictionalBaseRealTime"),
        issues,
    );
    check_opt_bool(obj.get("autoPrepend"), at("autoPrepend"), issues);
    check_nullable_opt_string(obj.get("timezone"), at("timezone"), issues);
    check_opt_int_min(obj.get("intervalMinutes"), 1, at("intervalMinutes"), issues);
}

/// v4 `EquippedSlotsSchema` — five slots since P4.D87 (`hair` is a hairdo, not
/// hair), each `z.array(UUIDSchema).default([])`.
const WARDROBE_SLOT_KEYS: [&str; 5] = ["top", "bottom", "footwear", "accessories", "hair"];

/// v4 `OutfitSelectionSchema` (`lib/schemas/wardrobe.types.ts`).
fn check_outfit_selection(v: &Value, index: usize, issues: &mut Vec<CreateZodIssue>) {
    let idx = json!(index);
    let base = |k: &str| path(&[key("outfitSelections"), idx.clone(), key(k)]);
    let Some(obj) = v.as_object() else {
        issues.push(invalid_type(
            "object",
            path(&[key("outfitSelections"), idx]),
            Some(v),
        ));
        return;
    };
    check_required_uuid(obj.get("characterId"), base("characterId"), issues);
    check_required_enum(
        obj.get("mode"),
        &["default", "manual", "llm_choose", "none", "previous_chat"],
        base("mode"),
        issues,
    );
    let Some(slots) = obj.get("slots") else {
        return;
    };
    let Some(slots) = slots.as_object() else {
        issues.push(invalid_type("object", base("slots"), obj.get("slots")));
        return;
    };
    for slot in WARDROBE_SLOT_KEYS {
        let Some(v) = slots.get(slot) else { continue };
        let at_slot = path(&[
            key("outfitSelections"),
            idx.clone(),
            key("slots"),
            key(slot),
        ]);
        let Some(items) = v.as_array() else {
            issues.push(invalid_type("array", at_slot, Some(v)));
            continue;
        };
        for (i, item) in items.iter().enumerate() {
            let at_item = path(&[
                key("outfitSelections"),
                idx.clone(),
                key("slots"),
                key(slot),
                json!(i),
            ]);
            check_required_uuid(Some(item), at_item, issues);
        }
    }
}

// ---------------------------------------------------------------------------
// The stage itself
// ---------------------------------------------------------------------------

/// v4 `createChatSchema.parse(body)` (`app/api/v1/chats/route.ts:98-187`,
/// called at `:1084` as the FIRST statement after `req.json()`).
///
/// One pass over the raw body in the schema's own key order — which is the
/// order Zod reports issues in (`vb_two_issues_schema_order`) — collecting
/// EVERY issue rather than stopping at the first, exactly as `.parse` does on
/// an object. Unknown keys are stripped, never refused
/// (`vb_ok_unknown_key_stripped`).
///
/// The refusal is v4's middleware envelope: an uncaught `ZodError` reaches
/// `createContextHandler` (`lib/api/middleware/context.ts:166`) →
/// `validationError(error)` (`lib/api/responses.ts:108`) → `{error:
/// 'Validation error', details: err.issues}` at 400 — and, critically, BEFORE
/// the creation-progress emitter (`:1090`) and the continuation ownership
/// lookup (`:1097`), so a wrong-typed field answers 400 even when the
/// continuation chat is missing (`vb_title_wrong_type_before_404`).
pub fn validate_create_body(body: &Map<String, Value>) -> Result<(), Vec<CreateZodIssue>> {
    let mut issues: Vec<CreateZodIssue> = Vec::new();
    let at = |k: &str| path(&[key(k)]);

    // participants: z.array(createParticipantSchema).min(1, '…')
    match body.get("participants") {
        Some(Value::Array(list)) => {
            for (i, p) in list.iter().enumerate() {
                check_participant(p, i, &mut issues);
            }
            if list.is_empty() {
                issues.push(CreateZodIssue::TooSmall {
                    origin: "array",
                    code: "too_small",
                    minimum: json!(1),
                    inclusive: true,
                    path: at("participants"),
                    message: "At least one participant is required".to_string(),
                });
            }
        }
        other => issues.push(invalid_type("array", at("participants"), other)),
    }

    check_opt_string(body.get("title"), at("title"), None, &mut issues);
    check_opt_string(body.get("scenario"), at("scenario"), None, &mut issues);
    check_opt_uuid(body.get("scenarioId"), at("scenarioId"), &mut issues);
    check_opt_string(
        body.get("projectScenarioPath"),
        at("projectScenarioPath"),
        Some(500),
        &mut issues,
    );
    check_opt_string(
        body.get("generalScenarioPath"),
        at("generalScenarioPath"),
        Some(500),
        &mut issues,
    );
    check_opt_string(
        body.get("groupScenarioPath"),
        at("groupScenarioPath"),
        Some(500),
        &mut issues,
    );
    check_opt_uuid(
        body.get("groupScenarioGroupId"),
        at("groupScenarioGroupId"),
        &mut issues,
    );
    // timestampConfig: TimestampConfigSchema.optional() — `.optional()` is NOT
    // nullable, so an explicit null is an object type failure
    // (`tc_explicit_null_rejected`).
    match body.get("timestampConfig") {
        None => {}
        Some(Value::Object(obj)) => check_timestamp_config(obj, &mut issues),
        other => issues.push(invalid_type("object", at("timestampConfig"), other)),
    }
    check_opt_uuid(body.get("projectId"), at("projectId"), &mut issues);
    check_opt_uuid(
        body.get("imageProfileId"),
        at("imageProfileId"),
        &mut issues,
    );
    // roleplayTemplateId: z.uuid().nullable().optional() — a deliberate null.
    match body.get("roleplayTemplateId") {
        None | Some(Value::Null) => {}
        other => check_opt_uuid(other, at("roleplayTemplateId"), &mut issues),
    }
    check_opt_enum(
        body.get("conciergeState"),
        &["monitored", "flagged", "vouched", "uncensored"],
        at("conciergeState"),
        &mut issues,
    );
    match body.get("outfitSelections") {
        None => {}
        Some(Value::Array(list)) => {
            for (i, sel) in list.iter().enumerate() {
                check_outfit_selection(sel, i, &mut issues);
            }
        }
        other => issues.push(invalid_type("array", at("outfitSelections"), other)),
    }
    check_opt_bool(
        body.get("avatarGenerationEnabled"),
        at("avatarGenerationEnabled"),
        &mut issues,
    );
    check_opt_uuid(
        body.get("continuationFromChatId"),
        at("continuationFromChatId"),
        &mut issues,
    );
    check_opt_uuid(body.get("progressId"), at("progressId"), &mut issues);
    check_opt_enum(
        body.get("chatType"),
        &["salon", "autonomous"],
        at("chatType"),
        &mut issues,
    );
    check_opt_string(
        body.get("scheduleCron"),
        at("scheduleCron"),
        Some(120),
        &mut issues,
    );
    check_opt_int_positive(
        body.get("scheduleFreshnessWindowMs"),
        at("scheduleFreshnessWindowMs"),
        &mut issues,
    );
    check_opt_int_positive(
        body.get("budgetMaxTurns"),
        at("budgetMaxTurns"),
        &mut issues,
    );
    check_opt_int_positive(
        body.get("budgetMaxTokens"),
        at("budgetMaxTokens"),
        &mut issues,
    );
    check_opt_int_positive(
        body.get("budgetMaxWallClockMs"),
        at("budgetMaxWallClockMs"),
        &mut issues,
    );
    check_opt_number_positive(
        body.get("budgetEstimatedSpendCapUSD"),
        at("budgetEstimatedSpendCapUSD"),
        &mut issues,
    );
    check_opt_enum(
        body.get("runVisibility"),
        &["owner_only", "household", "open"],
        at("runVisibility"),
        &mut issues,
    );
    check_opt_bool(
        body.get("runDestructiveToolsAllowed"),
        at("runDestructiveToolsAllowed"),
        &mut issues,
    );
    check_opt_bool(
        body.get("budgetExcludeCacheHits"),
        at("budgetExcludeCacheHits"),
        &mut issues,
    );

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
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

/// The payload of [`HandleCreateError::BadRequest`]: v4's sentence, plus the
/// `details` bag its `validationError(err)` envelope carries beside it.
///
/// A single tuple field rather than a second one on the variant, so the
/// composing host's `HandleCreateError::BadRequest(_)` arm is untouched.
#[derive(Debug)]
pub struct BadRequestBody {
    /// The `error` sentence — v4's flat `'Validation error'` for a Zod refusal,
    /// or a handler's own `badRequest(...)` copy.
    pub message: String,
    /// v4 `validationError(err).details` — the raw Zod issue array. `None` for
    /// a plain `badRequest(...)`, which has no `details` key at all.
    pub details: Option<Value>,
}

/// v4's `handleCreate` failure modes: `notFound` (404), `badRequest` (400), and
/// everything else (500) as a `Db` wrap.
#[derive(Debug)]
pub enum HandleCreateError {
    NotFound(String),
    BadRequest(BadRequestBody),
    Db(DbError),
}

impl HandleCreateError {
    /// A plain `badRequest(message)` — no `details` key.
    pub fn bad_request(message: impl Into<String>) -> Self {
        HandleCreateError::BadRequest(BadRequestBody {
            message: message.into(),
            details: None,
        })
    }

    /// v4's `validationError(err)` envelope — the fixed `'Validation error'`
    /// sentence plus the Zod issue array.
    pub fn validation_error(issues: &[CreateZodIssue]) -> Self {
        HandleCreateError::BadRequest(BadRequestBody {
            message: VALIDATION_ERROR.to_string(),
            details: Some(create_issue_details(issues)),
        })
    }

    /// The `details` bag a 400 carries, when it has one.
    pub fn details(&self) -> Option<&Value> {
        match self {
            HandleCreateError::BadRequest(b) => b.details.as_ref(),
            _ => None,
        }
    }
}

impl std::fmt::Display for HandleCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandleCreateError::NotFound(what) => write!(f, "{what} not found"),
            HandleCreateError::BadRequest(b) => write!(f, "{}", b.message),
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
        HandleCreateError::Db(DbError::Internal(e.to_string()))
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

    // v4 `route.ts:1084` `const validatedData = createChatSchema.parse(body)` —
    // the FIRST statement after `req.json()`, ahead of the creation-progress
    // emitter (`:1090`), the continuation ownership lookup (`:1097`) and every
    // read below. ONE stage over the whole body: v5 used to refuse three fields
    // ad hoc (`conciergeState`, `roleplayTemplateId`, `timestampConfig`) and let
    // the rest through, which is dogfood finding #115 — a stored
    // `controlledBy: "LLM"` split the server's and the SPA's readers on the
    // Friday copy because nothing ever refused it. An uncaught `ZodError` is
    // v4's `{error: 'Validation error', details: err.issues}` at 400, and it
    // writes nothing.
    if let Err(issues) = validate_create_body(&req.raw) {
        return Err(HandleCreateError::validation_error(&issues));
    }

    let is_autonomous = req.chat_type.as_deref() == Some("autonomous");

    // v4 `303288fb4` `createChatSchema.conciergeState`. The refusals moved into
    // the stage above (an explicit null and a wrong type both land on v4's
    // `z.enum` `invalid_value`); what stays here is the NORMALIZATION of an
    // already-validated value to the typed state.
    let requested_concierge_state: Option<ConciergeState> =
        req.concierge_state.as_ref().and_then(|v| {
            v.as_ref()
                .and_then(|raw| raw.as_str())
                .and_then(ConciergeState::from_wire)
        });

    // v4 `roleplayTemplateId: z.uuid().nullable().optional()`. The wrong-TYPE
    // and non-UUID-STRING refusals are the stage's (P4.78 closed the unmeasured
    // note this comment used to carry — `vb_roleplay_template_id_not_uuid` now
    // pins the format arm the port previously let through). What stays here is
    // the tri-state normalization; the RESOLUTION (does the template exist)
    // stays where v4 does it, below.
    let requested_roleplay_template_id: Option<Option<String>> = req
        .roleplay_template_id
        .as_ref()
        .map(|v| v.as_ref().and_then(|v| v.as_str()).map(str::to_string));
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
            return Err(HandleCreateError::bad_request(
                "Autonomous rooms cannot include user-controlled participants",
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
            return Err(HandleCreateError::bad_request(
                "Autonomous rooms require at least two LLM-controlled characters",
            ));
        }
        if let Some(cron_raw) = req.schedule_cron.as_deref() {
            let expr = cron_raw.trim();
            if !expr.is_empty() {
                match cron::try_next_occurrence(expr, deps.now_ms, &deps.tz) {
                    Ok(next) => autonomous_next_run_at = next.map(iso_from_unix_ms),
                    Err(_) => {
                        return Err(HandleCreateError::bad_request(format!(
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

    // 4. Resolve the chosen preset scenario body and layer the free-text notes
    //    beneath it. The precedence chain lives in `resolve_scenario_selection`
    //    (v4 `44a8137e` extracted it out of this route) so the in-chat scenario
    //    picker resolves a selection exactly the way the New Chat dialog does.
    let resolved_scenario = resolve_scenario_selection(
        main,
        mount,
        &ScenarioSelectionFields {
            scenario: req.scenario.as_deref(),
            scenario_id: req.scenario_id.as_deref(),
            project_scenario_path: req.project_scenario_path.as_deref(),
            group_scenario_path: req.group_scenario_path.as_deref(),
            group_scenario_group_id: req.group_scenario_group_id.as_deref(),
            general_scenario_path: req.general_scenario_path.as_deref(),
        },
        &ResolveScenarioSelectionOptions {
            project_id: req.project_id.as_deref(),
            character: primary_character.as_ref(),
            log_tag: Some("[Chats v1]"),
        },
    )?;

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
    // The REFUSALS (an explicit null, a non-object, a bad inner field) are the
    // stage's; `parse_timestamp_config` is kept for what it alone does —
    // materializing `TimestampConfigSchema`'s defaults and stripping unknown
    // keys. Its own error arm is unreachable for a body the stage accepted, and
    // answers the same sentence WITHOUT v4's `details` if the two ever disagree.
    let request_timestamp_config: Option<Value> = match &req.timestamp_config {
        None | Some(None) => None,
        Some(Some(v)) => Some(
            chat_timestamp::parse_timestamp_config(v)
                .map_err(|_| HandleCreateError::bad_request(VALIDATION_ERROR))?,
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
    if let Some(Some(requested)) = requested_roleplay_template_id.as_ref() {
        if !requested.is_empty()
            && crate::db::roleplay_templates::find_full_json_by_id(main, requested)?.is_none()
        {
            return Err(HandleCreateError::bad_request(
                "Roleplay template not found",
            ));
        }
    }
    let user_default_roleplay_template_id: Option<String> = chat_settings
        .as_ref()
        .and_then(|s| s.get("defaultRoleplayTemplateId").and_then(Value::as_str))
        .map(str::to_string);
    let default_roleplay_template_id: Option<String> = match &requested_roleplay_template_id {
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
        requested = ?requested_roleplay_template_id.as_ref().and_then(Option::as_deref),
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

    let create_data: ChatCreate = serde_json::from_value(create_obj).map_err(|e| {
        HandleCreateError::Db(DbError::Internal(format!("chat create marshal: {e}")))
    })?;
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
        HandleCreateError::Db(DbError::Internal(
            "created chat vanished on re-read".to_string(),
        ))
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
        // Before the backfill: the Concierge's note must precede the replayed
        // tail, so the history reads "state set, then the previous chapter".
        apply_requested_concierge_state(db, &chat_id, &chat, requested_concierge_state, emitter)
            .await
            .map_err(HandleCreateError::Db)?;
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
        apply_requested_concierge_state(db, &chat_id, &chat, requested_concierge_state, emitter)
            .await
            .map_err(HandleCreateError::Db)?;
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
        // Ordinary flow: system prompt → the Concierge's note (when a
        // non-Monitored state was picked on the New Chat form) → the scene and
        // the greeting, which is then generated under the chosen state.
        emitter.status("Setting the opening scene\u{2026}");
        write_system_prompt_message(main, &chat_context, &chat_id)?;
        apply_requested_concierge_state(db, &chat_id, &chat, requested_concierge_state, emitter)
            .await
            .map_err(HandleCreateError::Db)?;
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
            return Err(HandleCreateError::bad_request(
                "characterId is required for CHARACTER participants",
            ));
        };
        let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
            return Err(HandleCreateError::bad_request("Character not found"));
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
            return Err(HandleCreateError::bad_request(
                "connectionProfileId is required for LLM-controlled CHARACTER participants",
            ));
        }
        if let Some(cpid) = data.connection_profile_id.as_deref() {
            if connection_profiles::find_by_id(main, cpid)?.is_none() {
                return Err(HandleCreateError::bad_request(
                    "Connection profile not found",
                ));
            }
        }
        if let Some(ipid) = data.image_profile_id.as_deref() {
            if image_profiles::find_by_id(main, ipid)?.is_none() {
                return Err(HandleCreateError::bad_request("Image profile not found"));
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
        return Err(HandleCreateError::bad_request(
            "At least one LLM-controlled CHARACTER participant is required",
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
    .map_err(|e| DbError::Internal(format!("system message marshal: {e}")))?;
    ChatMessagesRepository::new(main).add_message(chat_id, &event)
}

/// v4 `303288fb4` `applyRequestedConciergeState`. Apply a Concierge state
/// requested at creation. Runs after the system-prompt message and before any
/// staff announcement or greeting, so the Concierge's bubble is the first thing
/// in the history after the prompt and the greeting is generated under the
/// chosen state. Monitored (or absence) is a no-op — v4 returns before it even
/// emits the progress line, so a Monitored create is byte-identical to one that
/// never named the field.
///
/// The route runs in the parent process, so the announcement's write lands
/// immediately — every later reader (the greeting's own `find_by_id`, the
/// scheduled danger scan, memory extraction, story backgrounds) sees the pair.
///
/// `chat_id` is the caller's own id (v4 reads `chat.id` unconditionally — there
/// is no "row without an id" arm, so none is invented here); `chat` is the
/// created row the caller already holds (v4's `ChatMetadata` from
/// `chats.create`; here the step-8 re-read). A fresh chat's
/// `conciergeOverride`/`isDangerousChat` are null, so `get_concierge_state`
/// reads Monitored and any non-Monitored request always CHANGES.
async fn apply_requested_concierge_state(
    db: &Db,
    chat_id: &str,
    chat: &Value,
    requested: Option<ConciergeState>,
    emitter: &CreationProgressEmitter,
) -> Result<(), DbError> {
    let Some(requested) = requested else {
        return Ok(());
    };
    if requested == ConciergeState::Monitored {
        return Ok(());
    }
    emitter.status("Briefing the Concierge\u{2026}");
    let result =
        apply_concierge_flip(db, &RealConciergeAnnouncer { db }, chat_id, requested, chat).await?;
    tracing::debug!(
        chat_id = chat_id,
        requested = requested.as_str(),
        changed = result.changed,
        "[Chats v1] Applied Concierge state at creation"
    );
    Ok(())
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
    // v4 `23af7146`: reasoning is only ever captured for a GENERATED greeting —
    // a scripted one never touched a model. DISPLAY ONLY, like every other
    // stored turn.
    let mut first_message_reasoning = String::new();
    if first_message_content.is_empty() {
        let generated = auto_generate_first_message(
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
        first_message_content = generated.content;
        first_message_reasoning = generated.reasoning_content;
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
        // v4 `reasoningContent: firstMessageReasoning || null` — the JS `||`
        // maps the empty string to null, so a scripted first message (and a
        // model that produced no thinking) stores an explicit NULL.
        "reasoningContent": if first_message_reasoning.is_empty() {
            Value::Null
        } else {
            Value::String(first_message_reasoning.clone())
        },
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
    for slot in crate::wardrobe::WARDROBE_SLOT_TYPES {
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
            // v4 `sharedWardrobeTiersForCharacter(characterId, equippedProjectMountPointIds)`
            // — the project tier is resolved once for the whole cast, the group
            // tier per character.
            &crate::wardrobe_tiers::shared_wardrobe_tiers_for_character(
                main,
                mount,
                character_id,
                equipped_project_mount_point_ids,
            ),
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
    let outfit = crate::wardrobe::build_outfit_slot_values(titles_for);

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

/// A generated opening greeting (v4 `GeneratedGreeting`, `23af7146`).
/// `reasoning_content` is the thinking a reasoning model produced while
/// composing it — DISPLAY ONLY, persisted onto the greeting message so the
/// Salon renders its thinking fold like any other turn. Empty for the give-up
/// paths and for models that produced none.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct GeneratedGreeting {
    pub content: String,
    pub reasoning_content: String,
}

impl GeneratedGreeting {
    /// v4's `NO_GREETING` constant — the shape all four give-up paths return.
    fn none() -> Self {
        Self::default()
    }
    fn from_result(res: &crate::services::initial_greeting::GreetingResult) -> Self {
        Self {
            content: res.content.clone(),
            reasoning_content: res.reasoning_content.clone(),
        }
    }
}

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
) -> GeneratedGreeting
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
        return GeneratedGreeting::none();
    };
    let Some(connection_profile_id) = participant
        .get("connectionProfileId")
        .and_then(Value::as_str)
    else {
        return GeneratedGreeting::none();
    };

    let connection_profile = match connection_profiles::find_by_id(main, connection_profile_id) {
        Ok(Some(p)) => p,
        _ => return GeneratedGreeting::none(),
    };

    let mut api_key = String::new();
    if let Some(api_key_id) = connection_profile.get("apiKeyId").and_then(Value::as_str) {
        match api_keys::find_by_id(main, api_key_id) {
            Ok(Some(k)) => api_key = k.key_value,
            _ => return GeneratedGreeting::none(),
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

    // P4.D83 (v4 `d89babc4`): `const sampling = resolveSamplingParams(parameters)`.
    // The greeting had the camelCase bug too — `parameters.maxTokens` / `.topP`
    // are names the profile editor never writes, so a greeting went out with the
    // provider's defaults while Settings displayed the profile's own figures.
    // Note the blob is the RAW `parameters` cell (v4's `rawParameters ?? {}`),
    // not `profileParams(...)` — v4 resolves the sampling knobs off the former
    // and forwards the latter as `profileParameters`.
    let sampling = crate::sampling_params::resolve_sampling_params(Some(&parameters));
    let temperature = sampling.temperature;
    let max_tokens = sampling.max_tokens.map(|n| n as i64);
    let top_p = sampling.top_p;
    let base_url = connection_profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::to_string);
    // v4 `d9c5a1c7`: `profileParameters: profileParams(connectionProfile)` —
    // the greeting's own inline `typeof === 'object'` copy became the shared
    // helper, so an Ollama greeting carries `num_ctx` from Max Context. (An
    // ARRAY `parameters` cell now forwards, as `typeof [] === 'object'`; the
    // separate `parameters` variable above keeps v4's raw cast, which yields
    // `undefined` for every key either way.)
    let profile_parameters = crate::cheap_llm::profile_params_value(&connection_profile);

    // v4 `303288fb4`: the chat's own Concierge state decides which desk this
    // greeting goes to. `apply_requested_concierge_state` has already written
    // the pair by the time the scenario-and-staff phase reaches the greeting, so
    // a chat created Uncensored asks the frank desk first instead of discovering
    // it after a refusal. (v4 `repos.chats.findById` — `None` when the row is
    // gone, which reads as Monitored everywhere below.)
    let chat_row = chats_read::find_by_id(main, chat_id).ok().flatten();

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

    // Attempt 0 (v4 `303288fb4`): a Flagged or Uncensored chat opens at the
    // uncensored desk. The three-attempt ladder below (with memories → without →
    // uncensored on a content filter) stays the path for Monitored and Vouched
    // Safe chats.
    let mut uncensored_desk_tried = false;
    if should_use_uncensored_route(chat_row.as_ref()) {
        uncensored_desk_tried = true;
        match generate_via_uncensored_desk(
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
            "chat-state",
            chat_row.as_ref(),
        )
        .await
        {
            Ok(Some(rerouted)) => return rerouted,
            Ok(None) => tracing::info!(
                character_id = %character_id,
                chat_id = %chat_id,
                "[Chats v1] Uncensored desk unavailable or empty for greeting — using the participant\u{2019}s own profile"
            ),
            // v4's try/catch around the closure: a throw is a log line and a
            // fall-through to the participant's own profile, never a failed create.
            Err(error) => tracing::warn!(
                character_id = %character_id,
                error = %error,
                "[Chats v1] Concierge uncensored greeting attempt failed"
            ),
        }
    }

    // Track whether any attempt hit a content filter so we can try the Concierge fallback
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
                    return GeneratedGreeting::from_result(&res);
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
                    return GeneratedGreeting::from_result(&res);
                }
                if res.content_filter_detected {
                    content_filter_hit = true;
                }
            }
            Err(_) => { /* swallowed */ }
        }
    }

    // Attempt 3: if a content filter was detected, try the Concierge uncensored
    // provider — unless the chat's own state already sent us there first, in
    // which case there is nothing new to try. A Vouched Safe chat resolves to
    // `mode: 'OFF'` inside the helper and never reroutes, whatever the globe says.
    if content_filter_hit && !uncensored_desk_tried {
        tracing::info!(
            character_id = %character_id,
            "[Chats v1] Content filter detected on greeting — falling back to Concierge uncensored provider"
        );
        match generate_via_uncensored_desk(
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
            "content-filter",
            chat_row.as_ref(),
        )
        .await
        {
            // `Ok(Some)` is never empty — the desk answers `Ok(None)` on an empty
            // greeting (v4's `if (!result.content) return null` inside the
            // closure), so the call site is v4's bare `if (rerouted) return`.
            Ok(Some(greeting)) => return greeting,
            Ok(None) => {}
            Err(error) => tracing::warn!(
                character_id = %character_id,
                error = %error,
                "[Chats v1] Concierge fallback for greeting generation failed"
            ),
        }
    }

    // Attempt 4: final plain retry (v4's 1s delay is host-timing — skipped).
    {
        let r = base_request();
        if let Ok(res) = generate_greeting_message(deps.streaming, &r, make_log().as_ref()).await {
            if !res.content.is_empty() {
                return GeneratedGreeting::from_result(&res);
            }
        }
    }

    GeneratedGreeting::none()
}

/// The Concierge-uncensored branch's sampling (v4 `d89babc4`,
/// `app/api/v1/chats/route.ts`): each knob resolved from the UNCENSORED
/// profile's bag falls back to the character's own profile INDEPENDENTLY, so an
/// uncensored profile that sets only a temperature still borrows the original's
/// Max Tokens and Top P. Split out so the rule is testable; since P4.D148 the
/// capstone corpus also pins it at the wire (the fixture's uncensored profile
/// sets a temperature and nothing else, so a reroute call must carry that
/// temperature with the ORIGINAL profile's Max Tokens and Top P).
fn borrow_sampling(
    uncensored: Option<&Value>,
    outer: &Value,
) -> crate::sampling_params::SamplingParams {
    let u = crate::sampling_params::resolve_sampling_params(uncensored);
    let o = crate::sampling_params::resolve_sampling_params(Some(outer));
    crate::sampling_params::SamplingParams {
        temperature: u.temperature.or(o.temperature),
        max_tokens: u.max_tokens.or(o.max_tokens),
        top_p: u.top_p.or(o.top_p),
    }
}

/// v4 `303288fb4` `generateViaUncensoredDesk` (formerly the attempt-3 body,
/// L748-804): generate the greeting on the Concierge's uncensored desk.
///
/// `Ok(None)` when there is nothing to reroute to — the resolved mode isn't
/// `AUTO_ROUTE`, no uncensored profile is configured, its key is unusable — or
/// the attempt came back empty, so the caller falls through to the
/// participant's own profile.
///
/// The resolver is asked WITH the chat: a Vouched Safe chat collapses to
/// `mode: 'OFF'` and never reroutes even under a global `AUTO_ROUTE`, and an
/// Uncensored chat reroutes even when the global mode is `OFF`. (Before
/// `303288fb4` this passed `None` and so asked the globe — the bug that made a
/// per-chat state mean nothing to the opening line.)
///
/// ⚠ v4's closure THROWS out of `generateGreetingMessage`, caught by each of
/// the two call sites; v5's `generate_greeting_message` returns `Err`, so the
/// error is propagated here and each call site carries v4's own catch. Do not
/// swallow it — the two catches log different sentences. (The parity is for the
/// GREETING call only: v4's `try` also catches throws from `chatSettings.
/// findByUserId` / the resolver / `resolveProviderForDangerousContent`, which
/// this helper `.ok()`-folds into `Ok(None)` — the warn sentence therefore
/// fires on strictly fewer inputs than v4's. Log-only, pre-existing.)
#[allow(clippy::too_many_arguments)]
async fn generate_via_uncensored_desk<EMB, CMP, STR>(
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
    // `trigger` is v4's `'chat-state' | 'content-filter'` — which of the two
    // call sites asked. Carried into the two info lines so a log says WHY the
    // desk was used, not just that it was.
    trigger: &'static str,
    chat_row: Option<&Value>,
) -> Result<Option<GeneratedGreeting>, StreamError>
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
    let resolved = resolve_dangerous_content_settings(global_settings, chat_row);
    if resolved.settings.mode != "AUTO_ROUTE" {
        return Ok(None);
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
        // v4 `app/api/v1/chats/route.ts:763` takes the `[]` default — a chat
        // being created carries no turn yet (v4 `a1d88aa3a`).
        &[],
    );
    if !route.rerouted {
        return Ok(None);
    }

    tracing::info!(
        character_id = %character_id,
        trigger = trigger,
        settings_source = resolved.source.as_str(),
        uncensored_profile = %route.connection_profile.name,
        uncensored_provider = %route.connection_profile.provider,
        uncensored_model = %route.connection_profile.model_name,
        "[Chats v1] Generating greeting on the Concierge uncensored provider"
    );

    // v4 reads the rerouted profile's own parameters — re-fetch by id.
    let uncensored_params = connection_profiles::find_by_id(main, &route.connection_profile.id)
        .ok()
        .flatten()
        .and_then(|p| p.get("parameters").cloned())
        .filter(|v| v.is_object());

    // P4.D83 (v4 `d89babc4`): both bags go through the resolver, and each knob
    // falls back to the character's own profile INDEPENDENTLY — an uncensored
    // profile that only sets a temperature still borrows the original's Max
    // Tokens and Top P. (v4: `resolveSamplingParams(uncensoredParams ?? {})`
    // then `uncensoredSampling.x ?? sampling.x`, knob by knob.)
    let borrowed = borrow_sampling(uncensored_params.as_ref(), parameters);
    let temperature = borrowed.temperature;
    let max_tokens = borrowed.max_tokens.map(|n| n as i64);
    let top_p = borrowed.top_p;

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

    let res = generate_greeting_message(deps.streaming, &request, log).await?;
    if res.content.is_empty() {
        return Ok(None);
    }
    tracing::info!(
        character_id = %character_id,
        trigger = trigger,
        provider = %route.connection_profile.provider,
        model = %route.connection_profile.model_name,
        "[Chats v1] Greeting generation succeeded via Concierge uncensored provider"
    );
    Ok(Some(GeneratedGreeting::from_result(&res)))
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

/// Parse `Slots` out of a manual-mode selection's `slots` Value (the canonical
/// reader — a missing slot key, `hair` on pre-hair payloads included, reads as
/// empty).
fn slots_from_value(v: &Value) -> Slots {
    Slots::from_value(Some(v))
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

    /// P4.D83: the Concierge-uncensored borrow is PER KNOB. v4 spreads the
    /// uncensored profile's own resolution over the character profile's one
    /// knob at a time (`uncensoredSampling.x ?? sampling.x`), so a partial
    /// uncensored bag does not blank the rest. No differential covers this —
    /// the capstone corpus has no dangerous-reroute case.
    #[test]
    fn uncensored_sampling_borrows_each_knob_independently() {
        let outer = json!({ "temperature": 0.9, "max_tokens": 4096, "top_p": 0.5 });

        // Only a temperature: the other two come from the character's profile.
        let b = borrow_sampling(Some(&json!({ "temperature": 0.2 })), &outer);
        assert_eq!(b.temperature, Some(0.2));
        assert_eq!(b.max_tokens, Some(4096.0));
        assert_eq!(b.top_p, Some(0.5));

        // An empty (or missing) uncensored bag borrows everything.
        for uncensored in [Some(json!({})), None] {
            let b = borrow_sampling(uncensored.as_ref(), &outer);
            assert_eq!(b.temperature, Some(0.9));
            assert_eq!(b.max_tokens, Some(4096.0));
            assert_eq!(b.top_p, Some(0.5));
        }

        // The uncensored bag's own spellings are tolerated the same way, and a
        // knob neither bag sets stays unset.
        let b = borrow_sampling(Some(&json!({ "maxTokens": 128 })), &json!({}));
        assert_eq!(b.max_tokens, Some(128.0));
        assert_eq!(b.temperature, None);
        assert_eq!(b.top_p, None);
    }

    // ------------------------------------------------------------------
    // P4.78 — the whole-body validation stage and the lenient decode.
    // The BEHAVIOUR is pinned arm-by-arm against v4's real route by
    // `chat_create_capstone_equivalence`; these guard the two seams the
    // differential cannot see from outside.
    // ------------------------------------------------------------------

    /// The decode can never fail on an object — that is what lets a wrong-typed
    /// field reach `handle_create` and answer v4's 400 instead of the host's
    /// `invalid chatCreate request: …`. A regression here would move the
    /// envelope without moving any corpus row (the refusal would simply never
    /// be reached), so it is asserted directly.
    #[test]
    fn every_wrong_typed_field_still_decodes() {
        for (key, bad) in [
            ("title", json!(42)),
            ("participants", json!("nope")),
            ("outfitSelections", json!("nope")),
            ("avatarGenerationEnabled", json!("yes")),
            ("budgetMaxTurns", json!("x")),
            ("budgetEstimatedSpendCapUSD", json!(true)),
            ("chatType", json!(7)),
            ("timestampConfig", json!(42)),
            ("conciergeState", json!(42)),
            ("roleplayTemplateId", json!(42)),
        ] {
            let body = json!({ "participants": [], key: bad });
            let decoded = serde_json::from_value::<ChatCreateRequest>(body.clone());
            assert!(
                decoded.is_ok(),
                "`{key}` failed the decode — the refusal can no longer reach \
                 the handler: {decoded:?}"
            );
            // …and the raw value is still there for the stage to refuse.
            assert!(validate_create_body(body.as_object().unwrap()).is_err());
        }
    }

    /// Zod's `z.number().int()` accepts an integral FLOAT (`1.0` is an int;
    /// `1.5` is not), so the typed view must too — otherwise a body the stage
    /// accepted would silently lose its value. `vb_ok_freshness_window_integral_float`
    /// pins the stored `1`; this pins the reader that produces it.
    #[test]
    fn integral_floats_reach_the_typed_view() {
        let req = ChatCreateRequest::from_raw(
            json!({
                "participants": [],
                "scheduleFreshnessWindowMs": 1.0,
                "budgetMaxTurns": 9007199254740991_i64,
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        assert_eq!(req.schedule_freshness_window_ms, Some(1));
        assert_eq!(req.budget_max_turns, Some(9007199254740991));

        // A non-integral or out-of-safe-range value is dropped rather than
        // rounded — unreachable in production because the stage refuses it
        // first, which this asserts in the same breath.
        let raw = json!({ "participants": [], "budgetMaxTurns": 1.5 });
        let obj = raw.as_object().unwrap();
        assert_eq!(
            ChatCreateRequest::from_raw(obj.clone()).budget_max_turns,
            None
        );
        assert!(validate_create_body(obj).is_err());
    }

    /// The stage collects EVERY issue in the schema's key order rather than
    /// stopping at the first — `vb_two_issues_schema_order` measures it against
    /// v4, this states the invariant in one line.
    #[test]
    fn the_stage_collects_issues_in_schema_order() {
        let raw = json!({
            "participants": [{ "type": "CHARACTER", "characterId": "11111111-1111-4111-8111-111111111111" }],
            "chatType": "brahma",
            "title": 42,
        });
        let issues = validate_create_body(raw.as_object().unwrap()).unwrap_err();
        let rendered = create_issue_details(&issues);
        let paths: Vec<&str> = rendered
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["path"][0].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["title", "chatType"]);
    }

    /// An unknown key is STRIPPED, never refused (`vb_ok_unknown_key_stripped`).
    #[test]
    fn unknown_keys_are_stripped_not_refused() {
        let raw = json!({
            "participants": [{ "type": "CHARACTER", "characterId": "11111111-1111-4111-8111-111111111111" }],
            "notAField": "whatever",
        });
        assert!(validate_create_body(raw.as_object().unwrap()).is_ok());
    }
}
