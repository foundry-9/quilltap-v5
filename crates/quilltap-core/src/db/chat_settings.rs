//! The chat-settings repository — a Phase-2 repo port (a plain `chat_settings`
//! main-DB repo), after `folders`, `tags`, `text_replacement_rules`,
//! `prompt_templates`, `conversation_annotations`, `image_profiles`,
//! `connection_profiles`, `users`, `terminal_sessions`, and the rest. Ports v4's
//! `lib/database/repositories/chat-settings.repository.ts` (+ the `_create`/
//! `_update`/`_delete` internals of `base.repository.ts`).
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods over the
//! base repo). v4's `create`/`update`/`delete` are thin `safeQuery` wrappers that
//! delegate STRAIGHT to `_create`/`_update`/`_delete` with NO default injection
//! and NO guard. The convenience helpers — `findByUserId`, `createForUser`, and
//! `updateForUser` (which injects the large default-settings object on first
//! access) — are out of scope here; the corpus supplies every column explicitly
//! on create instead of leaning on v4's defaults.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! `chat_settings` is by a wide margin the **widest JSON-object surface** the
//! tier-2 ports have hit — ~33 columns, ~15 of them nested typed-struct JSON
//! columns. It mixes every cell shape met so far plus a new one:
//!
//!   - **two UUID TEXT columns** (`id`, `userId`).
//!   - **one enum TEXT column** (`avatarDisplayMode`, `AvatarDisplayModeEnum`)
//!     and one plain-string-default TEXT column (`avatarDisplayStyle`,
//!     `z.string().default('CIRCULAR')`). Both bind as `String`.
//!   - **`tagStyles` — a record/map JSON-object column** (`TagStyleMapSchema =
//!     z.record(z.string(), TagVisualStyleSchema).default({})`). Modeled as a
//!     `serde_json::Value` (the map values are themselves objects). CONSTRAINED
//!     to `{}` in the corpus — not for key-order reasons (this crate enables
//!     serde_json's `preserve_order`, so a `Value::Object` keeps insertion
//!     order), but simply because nothing in this corpus needs a populated map.
//!     The `{}` case agrees trivially.
//!   - **~15 nested typed-struct JSON-object columns** (`cheapLLMSettings`,
//!     `themePreference`, `defaultTimestampConfig`, `memoryCascadePreferences`,
//!     `autoHousekeepingSettings`, `memoryExtractionLimits`,
//!     `autonomousRoomSettings`, `tokenDisplaySettings`,
//!     `contextCompressionSettings`, `llmLoggingSettings`, `agentModeSettings`,
//!     `coreWhisper`, `thinkingDisplay`, `answerConfirmationSettings`,
//!     `storyBackgroundsSettings`, `dangerousContentSettings`, `autoLockSettings`).
//!     Each is reproduced
//!     byte-for-byte with a serde struct in **schema field order**, which is what
//!     v4's `JSON.stringify(zodParsed)` emits (its key order is the Zod schema's
//!     field order). A typed struct — not a `serde_json::Value` — is what makes
//!     that order explicit and reviewable at the declaration site. This extends
//!     the `tags.visualStyle` typed-struct rule across many columns at once.
//!   - **five nullable UUID TEXT columns** (`imageDescriptionProfileId`,
//!     `uncensoredImageDescriptionProfileId`, `defaultRoleplayTemplateId`,
//!     plus the nested `*ProfileId` fields) → `Option<String>`; `None` → SQL
//!     NULL.
//!   - **one nullable string TEXT column** (`timezone`) → `Option<String>`.
//!   - **one optional INTEGER column** (`sidebarWidth`,
//!     `z.number().min(256).max(512).default(256).optional()`). This is the
//!     FIRST tier-2 INTEGER-affinity number column: both `.min(256)` and
//!     `.max(512)` are integers, so v4's `mapToSQLiteType` assigns INTEGER
//!     affinity (the prior numeric columns — `exitCode`, `maxContext`, the token
//!     counters — were all min-only/bare → REAL). It is `.optional()` with a
//!     default; v4 applies the Zod default during `validate`, so a row created
//!     without it stores `256`. The corpus supplies it explicitly. Bound as
//!     `i64`.
//!   - **six boolean columns** → INTEGER 0/1 (`i64::from(bool)`):
//!     `autoDetectRng`, `customTools`, `compositionModeDefault`,
//!     `composerSpellcheck`, `textReplacementsEnabled`,
//!     `autoScrollOnResponseComplete`.
//!
//! ### Nested JSON key-order discipline (the load-bearing detail)
//!
//! v4's `_create` runs `this.validate(entityInput)` (Zod `.parse`). Zod re-emits
//! object keys in **schema declaration order** regardless of input order, then
//! `JSON.stringify` serializes that. So the stored JSON's key order is fixed by
//! the schema, NOT by the corpus input. Each nested struct below lists its fields
//! in the exact order of its v4 schema (`settings.types.ts` / `common.types.ts` /
//! `themes/types.ts`), so `serde_json::to_string` of a fully-specified struct
//! reproduces v4's stored text byte-for-byte.
//!
//! Two nested optionality nuances reproduced here:
//!   - Fields that are `.nullable().optional()` with NO default (e.g.
//!     `CheapLLMSettings.userDefinedProfileId`, `ThemePreference.activeThemeId`
//!     is `.nullable().default(null)` so it is ALWAYS present; the truly
//!     optional `ThemePreference.customOverrides` and the `*ProfileId` fields):
//!     Zod OMITS the key entirely when the input omits it, but EMITS it as
//!     `null` when the input supplies `null`. The corpus supplies every such
//!     field as an explicit value (a UUID or `null`) so the key is always
//!     present, and these structs serialize them as explicit `null` (no
//!     `skip_serializing_if`). The one genuinely-omittable field that the corpus
//!     never supplies (`ThemePreference.customOverrides`,
//!     `TimestampConfig.customFormat`/`fictionalBaseTimestamp`/… ) is
//!     `skip_serializing_if = "Option::is_none"` so an absent value omits the key
//!     exactly as Zod does.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form
//! `folders`/`tags`/`connection_profiles`/… use.
//!
//! Deferred (not in the corpus, mirroring the precedent repos): clearing a
//! nullable column back to NULL via `update`; the multi-key `tagStyles` open-JSON
//! key-order seam (kept `{}`); and patching the nested JSON-object columns (the
//! corpus update patches the scalar/string columns and one whole-object replace).

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::DbError;

// ============================================================================
// Nested JSON-object structs — each in v4 SCHEMA FIELD ORDER (serde serializes
// struct fields in declaration order; this reproduces `JSON.stringify(zodParsed)`
// whose key order is the Zod schema's field order). A typed struct, not a
// `serde_json::Value`, so that order is declared here and reviewable.
// ============================================================================

/// `CheapLLMSettingsSchema` (settings.types.ts L49). The three `*ProfileId`
/// fields are `UUIDSchema.nullable().optional()` (no default) — Zod omits them
/// when the input omits them, emits `null` when the input gives `null`. The
/// corpus supplies them as explicit `null`/UUID, so they are always present;
/// hence plain `Option<String>` serialized as explicit `null` (no skip).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheapLlmSettings {
    pub strategy: String,
    pub user_defined_profile_id: Option<String>,
    pub default_cheap_profile_id: Option<String>,
    pub fallback_to_local: bool,
    pub embedding_provider: String,
    pub image_prompt_profile_id: Option<String>,
}

/// `ThemePreferenceSchema` (themes/types.ts L532). `activeThemeId` is
/// `.nullable().default(null)` (ALWAYS present, possibly `null`). `customOverrides`
/// is `.optional()` with NO default — Zod omits the key when absent — so it is
/// `skip_serializing_if`; the corpus omits it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePreference {
    pub active_theme_id: Option<String>,
    pub color_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_overrides: Option<serde_json::Value>,
    pub show_nav_theme_selector: bool,
}

/// `TimestampConfigSchema` (settings.types.ts L76). The five `.nullable()
/// .optional()` (no default) fields — `customFormat`, `fictionalBaseTimestamp`,
/// `fictionalBaseRealTime`, `timezone` — are `skip_serializing_if`: Zod omits
/// them when absent. The corpus omits them, matching v4. `intervalMinutes` is
/// `z.number().int().min(1).default(15)` — a NESTED number; inside a JSON object
/// it is serialized by `JSON.stringify`, so an integer prints as `15` (no
/// `.0`). Bound `i64` to match.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampConfig {
    pub mode: String,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_format: Option<String>,
    pub use_fictional_time: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fictional_base_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fictional_base_real_time: Option<String>,
    pub auto_prepend: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    pub interval_minutes: i64,
}

/// `MemoryCascadePreferencesSchema` (settings.types.ts L111).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCascadePreferences {
    pub on_message_delete: String,
    pub on_swipe_regenerate: String,
}

/// `AutoHousekeepingSettingsSchema` (settings.types.ts L131). `perCharacterCap`
/// is `z.number().int().positive()` — nested integer, prints as `2000`.
/// `perCharacterCapOverrides` is a record → constrained to `{}` (multi-key
/// open-JSON key-order seam). `autoMergeSimilarThreshold` is a fractional
/// `z.number()` (e.g. `0.9`) — kept as `f64`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoHousekeepingSettings {
    pub enabled: bool,
    pub per_character_cap: i64,
    /// Record → constrained `{}` in the corpus (multi-key key-order seam).
    pub per_character_cap_overrides: serde_json::Value,
    pub auto_merge_similar_threshold: f64,
    pub merge_similar: bool,
}

/// `MemoryExtractionLimitsSchema` (settings.types.ts L190). `maxPerHour` is a
/// nested integer (`20`); `softStartFraction`/`softFloor` are fractional `f64`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExtractionLimits {
    pub enabled: bool,
    pub max_per_hour: i64,
    pub soft_start_fraction: f64,
    pub soft_floor: f64,
}

/// `AutonomousRoomSettingsSchema` (settings.types.ts L225). `dailyTokenBudget`
/// is `z.number().int().positive().nullable().default(null)` — ALWAYS present,
/// `null` or a nested integer. `defaultFreshnessWindowMs` is a nested integer
/// (e.g. `43200000`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousRoomSettings {
    pub daily_token_budget: Option<i64>,
    pub default_freshness_window_ms: i64,
    pub visibility_default: String,
    pub destructive_tool_policy: String,
}

/// `TokenDisplaySettingsSchema` (settings.types.ts L238).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenDisplaySettings {
    pub show_per_message_tokens: bool,
    pub show_per_message_cost: bool,
    pub show_chat_totals: bool,
    pub show_system_events: bool,
}

/// `ContextCompressionSettingsSchema` (settings.types.ts L24). All five numbers
/// are bounded integers (nested) → print as bare integers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompressionSettings {
    pub enabled: bool,
    pub window_size: i64,
    pub compression_target_tokens: i64,
    pub system_prompt_target_tokens: i64,
    pub project_context_reinject_interval: i64,
}

/// `LLMLoggingSettingsSchema` (settings.types.ts L255). `retentionDays` is a
/// nested bounded integer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLoggingSettings {
    pub enabled: bool,
    pub verbose_mode: bool,
    pub retention_days: i64,
}

/// `AgentModeSettingsSchema` (settings.types.ts L318). `maxTurns` is a nested
/// bounded integer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModeSettings {
    pub max_turns: i64,
    pub default_enabled: bool,
}

/// `CoreWhisperSettingsSchema` (settings.types.ts L337). `interval`,
/// `silenceThreshold`, `packetTokenBudget` are nested integers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreWhisperSettings {
    pub enabled: bool,
    pub interval: i64,
    pub silence_threshold: i64,
    pub packet_token_budget: i64,
    pub fire_on_context_transition: bool,
}

/// `ThinkingDisplaySettingsSchema` (settings.types.ts L362).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingDisplaySettings {
    pub default_visible: bool,
    pub default_collapsed: bool,
}

/// `AnswerConfirmationSettingsSchema` (settings.types.ts). Global default for the
/// Salon answer-confirmation check. Single-key object; `enabled` carries a Zod
/// `.default(false)` but is always materialized on parse, so it is a plain bool.
/// Added by v4 `add-answer-confirmation-columns-v2` (DEFAULT `{"enabled":false}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerConfirmationSettings {
    pub enabled: bool,
}

/// `StoryBackgroundsSettingsSchema` (settings.types.ts L375).
/// `defaultImageProfileId` is `UUIDSchema.nullable().optional()` — the corpus
/// supplies it as explicit `null`/UUID, so it is always present (no skip).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryBackgroundsSettings {
    pub enabled: bool,
    pub default_image_profile_id: Option<String>,
}

/// `DangerousContentSettingsSchema` (settings.types.ts L276). `threshold` is a
/// fractional `f64`. The three `.nullable().optional()` (no default) fields —
/// `uncensoredTextProfileId`, `uncensoredImageProfileId`,
/// `customClassificationPrompt` — are `skip_serializing_if`: Zod omits them when
/// absent. The corpus omits them, matching v4.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DangerousContentSettings {
    pub mode: String,
    pub threshold: f64,
    pub scan_text_chat: bool,
    pub scan_image_prompts: bool,
    pub scan_image_generation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncensored_text_profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncensored_image_profile_id: Option<String>,
    pub display_mode: String,
    pub show_warning_badges: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_classification_prompt: Option<String>,
}

/// `AutoLockSettingsSchema` (settings.types.ts L305). `idleMinutes` is a nested
/// bounded integer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoLockSettings {
    pub enabled: bool,
    pub idle_minutes: i64,
}

// ============================================================================
// Create / update inputs
// ============================================================================

/// Fields for creating chat settings (the `Omit<ChatSettings,'id'|timestamps>`
/// shape) — every persisted column in schema (on-disk) order. The corpus
/// supplies every field explicitly (no reliance on Zod create-time defaults).
///
/// `Deserialize` was added for restore (P4.9G5 unit 4), whose input is a whole
/// settings row read back out of a backup archive. The four NULLABLE columns
/// carry `#[serde(default)]` because the backup projection omits a NULL column
/// entirely; the rest are required, which is the one place this deserialize is
/// stricter than v4's Zod parse — v4 would default a field an OLD archive
/// predates, v5 turns it into that phase's warn-and-continue. Recorded in the
/// P4.9G5 lane record.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSettingsCreate {
    pub user_id: String,
    /// Enum TEXT (`AvatarDisplayModeEnum`).
    pub avatar_display_mode: String,
    /// Plain-string-default TEXT.
    pub avatar_display_style: String,
    /// Record/map JSON-object column → compact JSON text. CONSTRAINED to `{}`
    /// (multi-key open-JSON key-order seam).
    pub tag_styles: serde_json::Value,
    /// v4's key is `cheapLLMSettings` (three capitals), not the camelCase
    /// derivation `cheapLlmSettings` — renamed explicitly.
    #[serde(rename = "cheapLLMSettings")]
    pub cheap_llm_settings: CheapLlmSettings,
    /// Nullable UUID TEXT; `None` => SQL NULL.
    #[serde(default)]
    pub image_description_profile_id: Option<String>,
    /// Nullable UUID TEXT; `None` => SQL NULL.
    #[serde(default)]
    pub uncensored_image_description_profile_id: Option<String>,
    /// Nullable UUID TEXT; `None` => SQL NULL.
    #[serde(default)]
    pub default_roleplay_template_id: Option<String>,
    pub theme_preference: ThemePreference,
    /// FIRST INTEGER-affinity number column (`.min(256).max(512)`, both int).
    pub sidebar_width: i64,
    pub default_timestamp_config: TimestampConfig,
    pub memory_cascade_preferences: MemoryCascadePreferences,
    pub auto_housekeeping_settings: AutoHousekeepingSettings,
    pub memory_extraction_limits: MemoryExtractionLimits,
    pub autonomous_room_settings: AutonomousRoomSettings,
    pub token_display_settings: TokenDisplaySettings,
    pub context_compression_settings: ContextCompressionSettings,
    pub llm_logging_settings: LlmLoggingSettings,
    pub auto_detect_rng: bool,
    pub custom_tools: bool,
    pub composition_mode_default: bool,
    pub composer_spellcheck: bool,
    pub text_replacements_enabled: bool,
    pub auto_scroll_on_response_complete: bool,
    pub agent_mode_settings: AgentModeSettings,
    pub core_whisper: CoreWhisperSettings,
    pub thinking_display: ThinkingDisplaySettings,
    /// Answer-confirmation global default JSON object (schema-order: between
    /// `thinkingDisplay` and `storyBackgroundsSettings`).
    pub answer_confirmation_settings: AnswerConfirmationSettings,
    pub story_backgrounds_settings: StoryBackgroundsSettings,
    pub dangerous_content_settings: DangerousContentSettings,
    pub auto_lock_settings: AutoLockSettings,
    /// Nullable string TEXT; `None` => SQL NULL.
    #[serde(default)]
    pub timezone: Option<String>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A chat-settings update patch. Mirrors v4 `update` over `_update`: provided
/// fields overwrite, id and createdAt are preserved (v4 deletes neither; we never
/// touch them), `updatedAt` is set explicitly. A representative subset of the
/// columns is exposed — the corpus exercises the scalar/enum/boolean columns, the
/// optional INTEGER `sidebarWidth`, a nullable UUID, `timezone`, and a couple of
/// whole-object JSON replaces. Each `Some` field sets that column; clearing a
/// nullable column to NULL and patching the remaining JSON objects are deferred
/// (not in the corpus).
#[derive(Default)]
pub struct ChatSettingsUpdate {
    pub avatar_display_mode: Option<String>,
    pub avatar_display_style: Option<String>,
    pub tag_styles: Option<serde_json::Value>,
    pub cheap_llm_settings: Option<CheapLlmSettings>,
    pub image_description_profile_id: Option<String>,
    pub default_roleplay_template_id: Option<String>,
    pub theme_preference: Option<ThemePreference>,
    pub sidebar_width: Option<i64>,
    pub dangerous_content_settings: Option<DangerousContentSettings>,
    pub auto_lock_settings: Option<AutoLockSettings>,
    pub auto_detect_rng: Option<bool>,
    pub custom_tools: Option<bool>,
    pub composition_mode_default: Option<bool>,
    pub composer_spellcheck: Option<bool>,
    pub text_replacements_enabled: Option<bool>,
    pub auto_scroll_on_response_complete: Option<bool>,
    pub answer_confirmation_settings: Option<AnswerConfirmationSettings>,
    pub timezone: Option<String>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct ChatSettingsRepository<'c> {
    conn: &'c Connection,
}

/// Serialize a nested JSON-object value to compact JSON text (schema field order
/// via serde struct declaration order). Errors map to [`DbError::Key`].
fn to_json<T: Serialize>(label: &str, value: &T) -> Result<String, DbError> {
    serde_json::to_string(value).map_err(|e| DbError::Key(format!("{label} serialize: {e}")))
}

impl<'c> ChatSettingsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert chat settings with the given pinned id + timestamps. All ~35
    /// columns are written explicitly in schema order; the JSON-object columns
    /// bind compact JSON text (schema key order), the boolean columns bind
    /// `i64::from(bool)`, `sidebarWidth` binds `i64` (INTEGER affinity), the
    /// nullable columns bind `Option<String>` (`None` → SQL NULL).
    ///
    /// Goes through [`crate::db::tolerant_insert`], so a column the live table
    /// LACKS is dropped from the statement rather than erroring. A real
    /// migration-vintage instance can be missing `timezone` (the read side has
    /// tolerated that since the second Friday dogfood finding); this write did
    /// not, and the consequence was not confined to the rare full INSERT —
    /// `chat_settings_get` creates a default row when none exists, so an
    /// instance that lost its settings row answered EVERY page load with a 500
    /// and could not be repaired from the UI (third dogfood sighting of the
    /// class, 2026-08-03).
    pub fn create(&self, data: &ChatSettingsCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let tag_styles = to_json("tagStyles", &data.tag_styles)?;
        let cheap_llm_settings = to_json("cheapLLMSettings", &data.cheap_llm_settings)?;
        let theme_preference = to_json("themePreference", &data.theme_preference)?;
        let default_timestamp_config =
            to_json("defaultTimestampConfig", &data.default_timestamp_config)?;
        let memory_cascade_preferences =
            to_json("memoryCascadePreferences", &data.memory_cascade_preferences)?;
        let auto_housekeeping_settings =
            to_json("autoHousekeepingSettings", &data.auto_housekeeping_settings)?;
        let memory_extraction_limits =
            to_json("memoryExtractionLimits", &data.memory_extraction_limits)?;
        let autonomous_room_settings =
            to_json("autonomousRoomSettings", &data.autonomous_room_settings)?;
        let token_display_settings = to_json("tokenDisplaySettings", &data.token_display_settings)?;
        let context_compression_settings = to_json(
            "contextCompressionSettings",
            &data.context_compression_settings,
        )?;
        let llm_logging_settings = to_json("llmLoggingSettings", &data.llm_logging_settings)?;
        let agent_mode_settings = to_json("agentModeSettings", &data.agent_mode_settings)?;
        let core_whisper = to_json("coreWhisper", &data.core_whisper)?;
        let thinking_display = to_json("thinkingDisplay", &data.thinking_display)?;
        let answer_confirmation_settings = to_json(
            "answerConfirmationSettings",
            &data.answer_confirmation_settings,
        )?;
        let story_backgrounds_settings =
            to_json("storyBackgroundsSettings", &data.story_backgrounds_settings)?;
        let dangerous_content_settings =
            to_json("dangerousContentSettings", &data.dangerous_content_settings)?;
        let auto_lock_settings = to_json("autoLockSettings", &data.auto_lock_settings)?;

        // Booleans bind as i64; bound to locals so the `&dyn ToSql` refs below
        // outlive the call.
        let auto_detect_rng = i64::from(data.auto_detect_rng);
        let custom_tools = i64::from(data.custom_tools);
        let composition_mode_default = i64::from(data.composition_mode_default);
        let composer_spellcheck = i64::from(data.composer_spellcheck);
        let text_replacements_enabled = i64::from(data.text_replacements_enabled);
        let auto_scroll_on_response_complete = i64::from(data.auto_scroll_on_response_complete);

        crate::db::tolerant_insert(
            self.conn,
            "chat_settings",
            &[
                ("id", &opts.id),
                ("userId", &data.user_id),
                ("avatarDisplayMode", &data.avatar_display_mode),
                ("avatarDisplayStyle", &data.avatar_display_style),
                ("tagStyles", &tag_styles),
                ("cheapLLMSettings", &cheap_llm_settings),
                (
                    "imageDescriptionProfileId",
                    &data.image_description_profile_id,
                ),
                (
                    "uncensoredImageDescriptionProfileId",
                    &data.uncensored_image_description_profile_id,
                ),
                (
                    "defaultRoleplayTemplateId",
                    &data.default_roleplay_template_id,
                ),
                ("themePreference", &theme_preference),
                ("sidebarWidth", &data.sidebar_width),
                ("defaultTimestampConfig", &default_timestamp_config),
                ("memoryCascadePreferences", &memory_cascade_preferences),
                ("autoHousekeepingSettings", &auto_housekeeping_settings),
                ("memoryExtractionLimits", &memory_extraction_limits),
                ("autonomousRoomSettings", &autonomous_room_settings),
                ("tokenDisplaySettings", &token_display_settings),
                ("contextCompressionSettings", &context_compression_settings),
                ("llmLoggingSettings", &llm_logging_settings),
                ("autoDetectRng", &auto_detect_rng),
                ("customTools", &custom_tools),
                ("compositionModeDefault", &composition_mode_default),
                ("composerSpellcheck", &composer_spellcheck),
                ("textReplacementsEnabled", &text_replacements_enabled),
                (
                    "autoScrollOnResponseComplete",
                    &auto_scroll_on_response_complete,
                ),
                ("agentModeSettings", &agent_mode_settings),
                ("coreWhisper", &core_whisper),
                ("thinkingDisplay", &thinking_display),
                ("storyBackgroundsSettings", &story_backgrounds_settings),
                ("dangerousContentSettings", &dangerous_content_settings),
                ("autoLockSettings", &auto_lock_settings),
                ("timezone", &data.timezone),
                ("createdAt", &opts.created_at),
                ("updatedAt", &opts.updated_at),
                ("answerConfirmationSettings", &answer_confirmation_settings),
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the chat settings `id`. Returns `Ok(false)` when
    /// no row matched (v4's "not found -> null"). id and createdAt are never
    /// touched. Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &ChatSettingsUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(avatar_display_mode) = &patch.avatar_display_mode {
            assignments.push(format!("avatarDisplayMode = ?{}", values.len() + 1));
            values.push(Box::new(avatar_display_mode.clone()));
        }
        if let Some(avatar_display_style) = &patch.avatar_display_style {
            assignments.push(format!("avatarDisplayStyle = ?{}", values.len() + 1));
            values.push(Box::new(avatar_display_style.clone()));
        }
        if let Some(tag_styles) = &patch.tag_styles {
            assignments.push(format!("tagStyles = ?{}", values.len() + 1));
            values.push(Box::new(to_json("tagStyles", tag_styles)?));
        }
        if let Some(cheap_llm_settings) = &patch.cheap_llm_settings {
            assignments.push(format!("cheapLLMSettings = ?{}", values.len() + 1));
            values.push(Box::new(to_json("cheapLLMSettings", cheap_llm_settings)?));
        }
        if let Some(image_description_profile_id) = &patch.image_description_profile_id {
            assignments.push(format!("imageDescriptionProfileId = ?{}", values.len() + 1));
            values.push(Box::new(image_description_profile_id.clone()));
        }
        if let Some(default_roleplay_template_id) = &patch.default_roleplay_template_id {
            assignments.push(format!("defaultRoleplayTemplateId = ?{}", values.len() + 1));
            values.push(Box::new(default_roleplay_template_id.clone()));
        }
        if let Some(theme_preference) = &patch.theme_preference {
            assignments.push(format!("themePreference = ?{}", values.len() + 1));
            values.push(Box::new(to_json("themePreference", theme_preference)?));
        }
        if let Some(sidebar_width) = patch.sidebar_width {
            assignments.push(format!("sidebarWidth = ?{}", values.len() + 1));
            values.push(Box::new(sidebar_width));
        }
        if let Some(dangerous_content_settings) = &patch.dangerous_content_settings {
            assignments.push(format!("dangerousContentSettings = ?{}", values.len() + 1));
            values.push(Box::new(to_json(
                "dangerousContentSettings",
                dangerous_content_settings,
            )?));
        }
        if let Some(auto_lock_settings) = &patch.auto_lock_settings {
            assignments.push(format!("autoLockSettings = ?{}", values.len() + 1));
            values.push(Box::new(to_json("autoLockSettings", auto_lock_settings)?));
        }
        if let Some(answer_confirmation_settings) = &patch.answer_confirmation_settings {
            assignments.push(format!(
                "answerConfirmationSettings = ?{}",
                values.len() + 1
            ));
            values.push(Box::new(to_json(
                "answerConfirmationSettings",
                answer_confirmation_settings,
            )?));
        }
        if let Some(auto_detect_rng) = patch.auto_detect_rng {
            assignments.push(format!("autoDetectRng = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(auto_detect_rng)));
        }
        if let Some(custom_tools) = patch.custom_tools {
            assignments.push(format!("customTools = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(custom_tools)));
        }
        if let Some(composition_mode_default) = patch.composition_mode_default {
            assignments.push(format!("compositionModeDefault = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(composition_mode_default)));
        }
        if let Some(composer_spellcheck) = patch.composer_spellcheck {
            assignments.push(format!("composerSpellcheck = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(composer_spellcheck)));
        }
        if let Some(text_replacements_enabled) = patch.text_replacements_enabled {
            assignments.push(format!("textReplacementsEnabled = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(text_replacements_enabled)));
        }
        if let Some(auto_scroll_on_response_complete) = patch.auto_scroll_on_response_complete {
            assignments.push(format!(
                "autoScrollOnResponseComplete = ?{}",
                values.len() + 1
            ));
            values.push(Box::new(i64::from(auto_scroll_on_response_complete)));
        }
        if let Some(timezone) = &patch.timezone {
            assignments.push(format!("timezone = ?{}", values.len() + 1));
            values.push(Box::new(timezone.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE chat_settings SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the chat settings `id`. Returns `Ok(false)` when no row matched
    /// (v4's `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM chat_settings WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM chat_settings WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }
}

/// The full `chatSettings.findByUserId(userId)` net-read (v4
/// `ChatSettingsRepository.findByUserId` → `findOneByFilter({ userId })` =
/// `hydrateRow` + `ChatSettingsSchema.parse`). Returns the hydrated settings as
/// a `serde_json::Value` whose key order is the **schema field order** (v4's
/// `JSON.stringify(zodParsed)` emits keys in schema order; the `preserve_order`
/// `serde_json` feature keeps the insertion order this builder uses, and the
/// stored JSON-column text is itself already schema-ordered — v4 wrote it
/// post-parse — so re-parsing each cell raw preserves the nested key order too).
/// `None` when the user has no row (v4 `findOneByFilter` → `null`).
///
/// Marshaling faithful to v4's `hydrateRow` + Zod parse:
///   - every JSON-object column is parsed raw (a v4-written cell always carries
///     the Zod-materialized defaults);
///   - the four `.nullable().optional()` (no-default) columns
///     (`imageDescriptionProfileId`, `uncensoredImageDescriptionProfileId`,
///     `defaultRoleplayTemplateId`, `timezone`) are OMITTED when SQL NULL
///     (`hydrateRow` maps NULL → `undefined`, which Zod `.optional()` drops from
///     the parsed object); present as a string otherwise;
///   - the five boolean columns render as JSON booleans;
///   - `sidebarWidth` (INTEGER affinity, `.optional()` with `.default(256)`; a
///     v4-written row always stores a value) renders as a JSON number via
///     `js_number_to_json`;
///   - `id`/`userId`/`avatarDisplayMode`/`avatarDisplayStyle`/`createdAt`/
///     `updatedAt` are required strings.
///
/// This is the chat-settings read sub-unit the memory-gate watermark scoped
/// reader ([`find_auto_housekeeping_settings_by_user_id`]) deferred; the
/// `help_settings` tool ([`crate::tools::help`]) is its first consumer, and the
/// read-differential drives v4's REAL `findByUserId` for a full-object diff.
pub fn find_by_user_id(
    conn: &Connection,
    user_id: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    use serde_json::{Map, Value};

    // A JSON-object column: parse the stored text (schema-ordered, so key order
    // is preserved under `preserve_order`). A v4-written cell always parses; a
    // NULL cell would be Zod-defaulted by v4, but the corpus never writes NULL
    // JSON columns (create writes every column), so a NULL → `null` here is fine
    // and never exercised.
    fn parse_json(cell: Option<String>) -> Value {
        match cell {
            Some(text) if !text.is_empty() => serde_json::from_str(&text).unwrap_or(Value::Null),
            _ => Value::Null,
        }
    }
    // A `.nullable().optional()` string/UUID column: `Some` → present string,
    // `None` (SQL NULL) → the key is OMITTED (v4 drops the `undefined`).
    fn put_opt(obj: &mut Map<String, Value>, key: &str, v: Option<String>) {
        if let Some(s) = v {
            obj.insert(key.to_string(), Value::String(s));
        }
    }

    // The tolerant list: a real instance can lack never-migrated columns
    // (e.g. `timezone`) — substitute NULL so the positional extraction below
    // is unchanged (v4's SELECT-*-plus-absent-key semantics).
    let cols = crate::db::tolerant_select_list(
        conn,
        "chat_settings",
        &[
            "id",
            "userId",
            "avatarDisplayMode",
            "avatarDisplayStyle",
            "tagStyles",
            "cheapLLMSettings",
            "imageDescriptionProfileId",
            "uncensoredImageDescriptionProfileId",
            "defaultRoleplayTemplateId",
            "themePreference",
            "sidebarWidth",
            "defaultTimestampConfig",
            "memoryCascadePreferences",
            "autoHousekeepingSettings",
            "memoryExtractionLimits",
            "autonomousRoomSettings",
            "tokenDisplaySettings",
            "contextCompressionSettings",
            "llmLoggingSettings",
            "autoDetectRng",
            "customTools",
            "compositionModeDefault",
            "composerSpellcheck",
            "textReplacementsEnabled",
            "autoScrollOnResponseComplete",
            "agentModeSettings",
            "coreWhisper",
            "thinkingDisplay",
            "answerConfirmationSettings",
            "storyBackgroundsSettings",
            "dangerousContentSettings",
            "autoLockSettings",
            "timezone",
            "createdAt",
            "updatedAt",
        ],
    )?;
    let row = conn
        .query_row(
            &format!("SELECT {cols} FROM chat_settings WHERE userId = ?1 LIMIT 1"),
            params![user_id],
            |r| {
                // Build in schema field order (the ChatSettingsSchema declaration
                // order), matching v4's post-parse `JSON.stringify` key order.
                let mut obj = Map::new();
                obj.insert("id".into(), Value::String(r.get::<_, String>(0)?));
                obj.insert("userId".into(), Value::String(r.get::<_, String>(1)?));
                obj.insert(
                    "avatarDisplayMode".into(),
                    Value::String(r.get::<_, String>(2)?),
                );
                obj.insert(
                    "avatarDisplayStyle".into(),
                    Value::String(r.get::<_, String>(3)?),
                );
                obj.insert(
                    "tagStyles".into(),
                    parse_json(r.get::<_, Option<String>>(4)?),
                );
                obj.insert(
                    "cheapLLMSettings".into(),
                    parse_json(r.get::<_, Option<String>>(5)?),
                );
                put_opt(
                    &mut obj,
                    "imageDescriptionProfileId",
                    r.get::<_, Option<String>>(6)?,
                );
                put_opt(
                    &mut obj,
                    "uncensoredImageDescriptionProfileId",
                    r.get::<_, Option<String>>(7)?,
                );
                put_opt(
                    &mut obj,
                    "defaultRoleplayTemplateId",
                    r.get::<_, Option<String>>(8)?,
                );
                obj.insert(
                    "themePreference".into(),
                    parse_json(r.get::<_, Option<String>>(9)?),
                );
                // `.default(256).optional()` — the OUTER optional means an
                // absent key stays absent (the default never fires on
                // undefined), so a NULL/missing cell OMITS the key. A fresh
                // create always writes the column; NULL only arises on a
                // migration-vintage instance missing the column entirely.
                if let Some(w) = r.get::<_, Option<f64>>(10)? {
                    obj.insert("sidebarWidth".into(), super::js_number_to_json(w));
                }
                obj.insert(
                    "defaultTimestampConfig".into(),
                    parse_json(r.get::<_, Option<String>>(11)?),
                );
                obj.insert(
                    "memoryCascadePreferences".into(),
                    parse_json(r.get::<_, Option<String>>(12)?),
                );
                obj.insert(
                    "autoHousekeepingSettings".into(),
                    parse_json(r.get::<_, Option<String>>(13)?),
                );
                obj.insert(
                    "memoryExtractionLimits".into(),
                    parse_json(r.get::<_, Option<String>>(14)?),
                );
                obj.insert(
                    "autonomousRoomSettings".into(),
                    parse_json(r.get::<_, Option<String>>(15)?),
                );
                obj.insert(
                    "tokenDisplaySettings".into(),
                    parse_json(r.get::<_, Option<String>>(16)?),
                );
                obj.insert(
                    "contextCompressionSettings".into(),
                    parse_json(r.get::<_, Option<String>>(17)?),
                );
                obj.insert(
                    "llmLoggingSettings".into(),
                    parse_json(r.get::<_, Option<String>>(18)?),
                );
                obj.insert(
                    "autoDetectRng".into(),
                    Value::Bool(r.get::<_, i64>(19)? == 1),
                );
                // NULL here means the COLUMN IS ABSENT (the tolerant list above
                // substitutes NULL for a column the table lacks), not a stored
                // NULL — v4's ALTER carries `DEFAULT 1`, so no v4-written row
                // holds one. v4 reads `SELECT *` and parses through Zod, so an
                // absent column arrives as `undefined` and `customTools:
                // z.boolean().default(true)` supplies `true`. Reproduce that:
                // absent → `true`. This is the read half of unit 10's accepted
                // consequence — v5 provisions the column, but a pre-4.8.0 v4
                // instance lacks it (v5 does not port the migration runner), and
                // such an instance must still OPEN.
                obj.insert(
                    "customTools".into(),
                    Value::Bool(r.get::<_, Option<i64>>(20)?.is_none_or(|v| v == 1)),
                );
                obj.insert(
                    "compositionModeDefault".into(),
                    Value::Bool(r.get::<_, i64>(21)? == 1),
                );
                obj.insert(
                    "composerSpellcheck".into(),
                    Value::Bool(r.get::<_, i64>(22)? == 1),
                );
                obj.insert(
                    "textReplacementsEnabled".into(),
                    Value::Bool(r.get::<_, i64>(23)? == 1),
                );
                obj.insert(
                    "autoScrollOnResponseComplete".into(),
                    Value::Bool(r.get::<_, i64>(24)? == 1),
                );
                obj.insert(
                    "agentModeSettings".into(),
                    parse_json(r.get::<_, Option<String>>(25)?),
                );
                obj.insert(
                    "coreWhisper".into(),
                    parse_json(r.get::<_, Option<String>>(26)?),
                );
                obj.insert(
                    "thinkingDisplay".into(),
                    parse_json(r.get::<_, Option<String>>(27)?),
                );
                obj.insert(
                    "answerConfirmationSettings".into(),
                    parse_json(r.get::<_, Option<String>>(28)?),
                );
                obj.insert(
                    "storyBackgroundsSettings".into(),
                    parse_json(r.get::<_, Option<String>>(29)?),
                );
                obj.insert(
                    "dangerousContentSettings".into(),
                    parse_json(r.get::<_, Option<String>>(30)?),
                );
                obj.insert(
                    "autoLockSettings".into(),
                    parse_json(r.get::<_, Option<String>>(31)?),
                );
                put_opt(&mut obj, "timezone", r.get::<_, Option<String>>(32)?);
                obj.insert("createdAt".into(), Value::String(r.get::<_, String>(33)?));
                obj.insert("updatedAt".into(), Value::String(r.get::<_, String>(34)?));
                Ok(Value::Object(obj))
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row)
}

/// Scoped read for the memory gate's watermark check: the
/// `autoHousekeepingSettings` JSON of the user's settings row (v4
/// `chatSettings.findByUserId(userId)?.autoHousekeepingSettings`). `None` when
/// the user has no row or the cell is NULL — both collapse to the consumer's
/// "auto-housekeeping not configured" early return. The full `findByUserId`
/// read marshaling (~33 columns) is a later chat-settings read sub-unit; this
/// parses the one consumed column raw (a v4-written row's JSON already carries
/// the Zod-materialized defaults — parse-before-insert — and the consumer
/// `??`-defaults every key it reads, so the shapes agree).
pub fn find_auto_housekeeping_settings_by_user_id(
    conn: &rusqlite::Connection,
    user_id: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    let cell: Option<Option<String>> = conn
        .query_row(
            "SELECT autoHousekeepingSettings FROM chat_settings WHERE userId = ?1 LIMIT 1",
            rusqlite::params![user_id],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    // A v4-written cell always parses; a malformed one degrades to "not
    // configured" (the lenient precedent of the other JSON-cell readers).
    Ok(cell
        .flatten()
        .and_then(|text| serde_json::from_str(&text).ok()))
}

/// One user's scheduler-relevant chat-settings slice — the JSON-object columns the
/// daily maintenance sweeps consult, without the full net-read marshaling.
#[derive(Debug, Clone)]
pub struct SchedulerUserSettings {
    pub user_id: String,
    /// `autoHousekeepingSettings` (v4 JSON object) — the housekeeping enqueuer reads
    /// `.enabled`. `None` when the cell is NULL / malformed.
    pub auto_housekeeping_settings: Option<serde_json::Value>,
    /// `llmLoggingSettings` (v4 JSON object) — the LLM-log cleanup enqueuer reads
    /// `.enabled` + `.retentionDays`. `None` when the cell is NULL / malformed.
    pub llm_logging_settings: Option<serde_json::Value>,
}

/// Every user's scheduler slice (v4's `repos.chatSettings.findAll()` reduced to
/// the two JSON-object columns the daily housekeeping / LLM-log-cleanup enqueuers
/// consult). One row per `chat_settings` row (v4 iterates every row — orphan rows
/// included, matching v4 exactly). Insertion order is by rowid (v4's `findAll`
/// order); the enqueuers dedupe downstream so order does not affect the outcome.
pub fn find_all_scheduler_settings(
    conn: &rusqlite::Connection,
) -> Result<Vec<SchedulerUserSettings>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT userId, autoHousekeepingSettings, llmLoggingSettings FROM chat_settings",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let user_id: String = row.get(0)?;
            let ahk: Option<String> = row.get(1)?;
            let llm: Option<String> = row.get(2)?;
            Ok((user_id, ahk, llm))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(user_id, ahk, llm)| SchedulerUserSettings {
            user_id,
            auto_housekeeping_settings: ahk.and_then(|t| serde_json::from_str(&t).ok()),
            llm_logging_settings: llm.and_then(|t| serde_json::from_str(&t).ok()),
        })
        .collect())
}

// ============================================================================
// updateForUser — v4 `ChatSettingsRepository.updateForUser` (P4.6d)
// ============================================================================

/// A ready-to-bind chat-settings column value (the affinity the column expects):
/// `Text` for TEXT (incl. compact JSON-object columns, schema-ordered by the
/// caller), `Int` for the INTEGER/boolean columns, `Null` for a SQL NULL.
#[derive(Debug, Clone)]
pub enum SettingsColVal {
    Text(String),
    Int(i64),
    Null,
}

impl rusqlite::types::ToSql for SettingsColVal {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, Value as SqlValue};
        Ok(match self {
            SettingsColVal::Text(s) => ToSqlOutput::Owned(SqlValue::Text(s.clone())),
            SettingsColVal::Int(i) => ToSqlOutput::Owned(SqlValue::Integer(*i)),
            SettingsColVal::Null => ToSqlOutput::Owned(SqlValue::Null),
        })
    }
}

/// The captured default `chat_settings` seed — the byte-exact row v4's
/// `updateForUser` create branch produces (the same artifact the provisioner
/// replays). Reused so the P4.6d GET default-injection and fresh-row PUT are
/// byte-identical to v4's `updateForUser(userId, defaults ∪ data)`.
static CHAT_SETTINGS_SEED_JSON: &str =
    include_str!("../services/provisioning/chat_settings_seed.json");

#[derive(Deserialize)]
struct SeedRow {
    columns: Vec<String>,
    values: serde_json::Map<String, serde_json::Value>,
}

/// The 30 default non-id/non-timestamp columns of a fresh settings row, in v4
/// schema (create) order, each mapped to its bind value. Reads the captured seed
/// (a string cell → `Text`, a number → `Int`, JSON `null` → `Null`).
fn default_settings_columns() -> Result<Vec<(String, SettingsColVal)>, DbError> {
    let seed: SeedRow = serde_json::from_str(CHAT_SETTINGS_SEED_JSON)
        .map_err(|e| DbError::Key(format!("chat_settings_seed.json: {e}")))?;
    let mut out = Vec::with_capacity(seed.columns.len());
    for col in &seed.columns {
        let v = seed
            .values
            .get(col)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let cell = match v {
            serde_json::Value::String(s) => SettingsColVal::Text(s),
            serde_json::Value::Number(n) => SettingsColVal::Int(
                n.as_i64()
                    .ok_or_else(|| DbError::Key(format!("seed {col} not an integer")))?,
            ),
            serde_json::Value::Null => SettingsColVal::Null,
            other => {
                return Err(DbError::Key(format!(
                    "seed {col} unexpected shape: {other}"
                )));
            }
        };
        out.push((col.clone(), cell));
    }
    Ok(out)
}

/// v4 `ChatSettingsRepository.updateForUser(userId, data)` — the upsert chokepoint.
///
/// `assignments` is the validated `updateData` map (column → bind value, the
/// JSON-object columns already serialized schema-ordered by the caller). If the
/// user already has a settings row, this is a partial `SET` of the assignments +
/// a minted `updatedAt` (byte-identical to v4's full `$set: validated` because a
/// v4-written existing row already holds the schema-parsed value in every other
/// column). If not, it INSERTs a full default row (the captured seed) with the
/// assignments overriding the matching columns, minting `id` + `createdAt` ==
/// `updatedAt` == `now` (v4's create-branch `createdAt == updatedAt`).
///
/// Runs on the writer connection (both the existence check and the write in one
/// closure). The caller re-reads [`find_by_user_id`] for the response. Returns
/// `true` when it created a new row (v4's `updateForUser` create branch) — the
/// caller uses this to reproduce the create-RETURN shape, which keeps the explicit
/// `defaultRoleplayTemplateId: null` the default bag carries (the re-read omits it).
pub fn update_for_user(
    conn: &Connection,
    user_id: &str,
    assignments: &[(&str, SettingsColVal)],
    now: &str,
) -> Result<bool, DbError> {
    // Existing row?
    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM chat_settings WHERE userId = ?1 LIMIT 1",
            params![user_id],
            |r| r.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;

    if let Some(id) = existing_id {
        // Update branch: partial SET of the assignments + updatedAt.
        let mut set_clauses: Vec<String> = Vec::new();
        let mut binds: Vec<Box<dyn ToSql>> = Vec::new();
        for (col, val) in assignments {
            set_clauses.push(format!("{col} = ?{}", binds.len() + 1));
            binds.push(Box::new(val.clone()));
        }
        set_clauses.push(format!("updatedAt = ?{}", binds.len() + 1));
        binds.push(Box::new(now.to_string()));
        let id_idx = binds.len() + 1;
        binds.push(Box::new(id));
        let sql = format!(
            "UPDATE chat_settings SET {} WHERE id = ?{}",
            set_clauses.join(", "),
            id_idx
        );
        let refs: Vec<&dyn ToSql> = binds.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, refs.as_slice())?;
        return Ok(false);
    }

    // Create branch: full default row, assignments override, minted id + now/now.
    let mut columns = default_settings_columns()?;
    for (col, val) in assignments {
        if let Some(slot) = columns.iter_mut().find(|(c, _)| c == col) {
            slot.1 = val.clone();
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let mut col_names: Vec<String> = vec!["id".to_string(), "userId".to_string()];
    let mut binds: Vec<Box<dyn ToSql>> = vec![Box::new(id), Box::new(user_id.to_string())];
    for (col, val) in &columns {
        col_names.push(col.clone());
        binds.push(Box::new(val.clone()));
    }
    col_names.push("createdAt".to_string());
    binds.push(Box::new(now.to_string()));
    col_names.push("updatedAt".to_string());
    binds.push(Box::new(now.to_string()));

    // Drop any column the live table lacks (with its bind), rather than
    // erroring. THIS is the statement a migration-vintage instance dies on:
    // `chat_settings_get` creates a default row whenever none exists, so an
    // instance missing `timezone` answered every page load with
    // `no column named timezone` and could not be repaired from the UI. See
    // `crate::db::tolerant_insert`.
    let pairs: Vec<(&str, &dyn ToSql)> = col_names
        .iter()
        .map(String::as_str)
        .zip(binds.iter().map(|b| b.as_ref()))
        .collect();
    crate::db::tolerant_insert(conn, "chat_settings", &pairs)?;
    Ok(true)
}

/// Reproduce v4's `updateForUser` create-RETURN shape from a [`find_by_user_id`]
/// re-read: the create branch's `defaultSettings` carries an explicit
/// `defaultRoleplayTemplateId: null`, so the create return has that key present
/// (whereas the re-read omits the NULL nullable-optional column). Insert it at its
/// schema position (before `themePreference`) when absent.
pub fn patch_create_return_shape(settings: &mut serde_json::Value) {
    let Some(obj) = settings.as_object() else {
        return;
    };
    if obj.contains_key("defaultRoleplayTemplateId") {
        return;
    }
    // Rebuild preserving order, inserting the key just before `themePreference`.
    let mut rebuilt = serde_json::Map::new();
    for (k, v) in obj.iter() {
        if k == "themePreference" {
            rebuilt.insert(
                "defaultRoleplayTemplateId".to_string(),
                serde_json::Value::Null,
            );
        }
        rebuilt.insert(k.clone(), v.clone());
    }
    *settings = serde_json::Value::Object(rebuilt);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The second Friday dogfood finding: a real instance's `chat_settings`
    /// table predates the schema's `timezone` column (added to v4 with NO
    /// migration — v4's `SELECT *` reads never notice). The tolerant SELECT
    /// substitutes NULL so the read succeeds and the key is omitted, matching
    /// v4's absent-key semantics.
    #[test]
    fn find_by_user_id_tolerates_a_missing_timezone_column() {
        let conn = Connection::open_in_memory().unwrap();
        // A migration-vintage table: every column the read names EXCEPT
        // `timezone` (all TEXT is fine — the reader parses cell text).
        conn.execute_batch(
            "CREATE TABLE chat_settings (\
                id TEXT PRIMARY KEY, userId TEXT, avatarDisplayMode TEXT, \
                avatarDisplayStyle TEXT, tagStyles TEXT, cheapLLMSettings TEXT, \
                imageDescriptionProfileId TEXT, uncensoredImageDescriptionProfileId TEXT, \
                defaultRoleplayTemplateId TEXT, themePreference TEXT, sidebarWidth REAL, \
                defaultTimestampConfig TEXT, memoryCascadePreferences TEXT, \
                autoHousekeepingSettings TEXT, memoryExtractionLimits TEXT, \
                autonomousRoomSettings TEXT, tokenDisplaySettings TEXT, \
                contextCompressionSettings TEXT, llmLoggingSettings TEXT, \
                autoDetectRng INTEGER, customTools INTEGER, compositionModeDefault INTEGER, \
                composerSpellcheck INTEGER, textReplacementsEnabled INTEGER, \
                autoScrollOnResponseComplete INTEGER, agentModeSettings TEXT, \
                coreWhisper TEXT, thinkingDisplay TEXT, answerConfirmationSettings TEXT, \
                storyBackgroundsSettings TEXT, dangerousContentSettings TEXT, \
                autoLockSettings TEXT, createdAt TEXT, updatedAt TEXT);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chat_settings (id, userId, avatarDisplayMode, avatarDisplayStyle, \
             tagStyles, cheapLLMSettings, autoDetectRng, customTools, compositionModeDefault, \
             composerSpellcheck, textReplacementsEnabled, autoScrollOnResponseComplete, \
             createdAt, updatedAt) VALUES ('s1', 'u1', 'ALWAYS', 'CIRCULAR', '{}', \
             '{\"strategy\":\"PROVIDER_CHEAPEST\",\"fallbackToLocal\":true,\
             \"embeddingProvider\":\"OPENAI\"}', 1, 1, 0, 1, 1, 0, \
             '2026-07-10T00:00:00.000Z', '2026-07-10T00:00:00.000Z')",
            [],
        )
        .unwrap();

        let row = find_by_user_id(&conn, "u1").unwrap().expect("row");
        // The missing column reads as absent (v4 `undefined`, dropped).
        assert!(row.get("timezone").is_none());
        assert_eq!(row["avatarDisplayMode"], "ALWAYS");
        assert_eq!(row["cheapLLMSettings"]["strategy"], "PROVIDER_CHEAPEST");
    }

    /// The THIRD Friday dogfood sighting of the missing-column class
    /// (2026-08-03), and the one that bricked the app. The read side has
    /// tolerated a missing `timezone` since the second; the WRITE side did not,
    /// and `update_for_user`'s create branch is reached from
    /// `chat_settings_get` whenever no settings row exists — which is exactly
    /// the state a replace-mode restore leaves behind when its own settings
    /// insert failed for the same reason. Result: every page load answered
    /// `sqlite error: table chat_settings has no column named timezone`, with
    /// no way to repair it from the UI.
    ///
    /// Drives the real broken gesture (create-on-read with NO assignments),
    /// not the repository method in isolation.
    #[test]
    fn update_for_user_creates_a_row_on_a_table_lacking_timezone() {
        let conn = Connection::open_in_memory().unwrap();
        // Migration-vintage: every seeded column EXCEPT `timezone`.
        let seed: SeedRow = serde_json::from_str(CHAT_SETTINGS_SEED_JSON).unwrap();
        // Types matter: `sidebarWidth` and the booleans read back as i64, so a
        // blanket TEXT table would fail the RE-READ rather than the insert and
        // hide what this test is for. Derive affinity from the seed's own JSON.
        let cols = seed
            .columns
            .iter()
            .filter(|c| c.as_str() != "timezone")
            .map(|c| {
                let ty = match seed.values.get(c) {
                    Some(serde_json::Value::Number(_)) => "INTEGER",
                    _ => "TEXT",
                };
                format!("\"{c}\" {ty}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        conn.execute_batch(&format!(
            "CREATE TABLE chat_settings (id TEXT PRIMARY KEY, userId TEXT, {cols}, \
             createdAt TEXT, updatedAt TEXT);"
        ))
        .unwrap();

        let created =
            update_for_user(&conn, "u1", &[], "2026-08-03T00:00:00.000Z").expect("create branch");
        assert!(created, "no row existed, so this is the create branch");

        // And the row is readable — the pair of tolerances has to compose, since
        // the failing path is read -> create -> re-read.
        let row = find_by_user_id(&conn, "u1").unwrap().expect("row");
        assert!(row.get("timezone").is_none(), "absent column stays absent");
        assert_eq!(row["userId"], "u1");

        // A second call takes the UPDATE branch and must not re-create.
        let created_again =
            update_for_user(&conn, "u1", &[], "2026-08-03T00:00:01.000Z").expect("update branch");
        assert!(!created_again, "the row now exists");
    }

    /// Unit 10's accepted consequence, pinned: v5 adopts `customTools` WITHOUT
    /// porting v4's migration runner, so a pre-4.8.0 v4 instance opened by v5
    /// genuinely lacks the column. v4 would read it with `SELECT *` and let
    /// `z.boolean().default(true)` fill the `undefined` — so the absent column
    /// must surface as `true`, and the row must still READ rather than erroring
    /// on a NULL-into-`i64` extraction.
    #[test]
    fn find_by_user_id_defaults_custom_tools_when_the_column_is_absent() {
        let conn = Connection::open_in_memory().unwrap();
        // A pre-4.8.0 table: every column the read names EXCEPT `customTools`.
        conn.execute_batch(
            "CREATE TABLE chat_settings (\
                id TEXT PRIMARY KEY, userId TEXT, avatarDisplayMode TEXT, \
                avatarDisplayStyle TEXT, tagStyles TEXT, cheapLLMSettings TEXT, \
                imageDescriptionProfileId TEXT, uncensoredImageDescriptionProfileId TEXT, \
                defaultRoleplayTemplateId TEXT, themePreference TEXT, sidebarWidth REAL, \
                defaultTimestampConfig TEXT, memoryCascadePreferences TEXT, \
                autoHousekeepingSettings TEXT, memoryExtractionLimits TEXT, \
                autonomousRoomSettings TEXT, tokenDisplaySettings TEXT, \
                contextCompressionSettings TEXT, llmLoggingSettings TEXT, \
                autoDetectRng INTEGER, compositionModeDefault INTEGER, \
                composerSpellcheck INTEGER, textReplacementsEnabled INTEGER, \
                autoScrollOnResponseComplete INTEGER, agentModeSettings TEXT, \
                coreWhisper TEXT, thinkingDisplay TEXT, answerConfirmationSettings TEXT, \
                storyBackgroundsSettings TEXT, dangerousContentSettings TEXT, \
                autoLockSettings TEXT, timezone TEXT, createdAt TEXT, updatedAt TEXT);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chat_settings (id, userId, avatarDisplayMode, avatarDisplayStyle, \
             tagStyles, cheapLLMSettings, autoDetectRng, compositionModeDefault, \
             composerSpellcheck, textReplacementsEnabled, autoScrollOnResponseComplete, \
             createdAt, updatedAt) VALUES ('s1', 'u1', 'ALWAYS', 'CIRCULAR', '{}', \
             '{\"strategy\":\"PROVIDER_CHEAPEST\",\"fallbackToLocal\":true,\
             \"embeddingProvider\":\"OPENAI\"}', 1, 0, 1, 1, 0, \
             '2026-07-10T00:00:00.000Z', '2026-07-10T00:00:00.000Z')",
            [],
        )
        .unwrap();

        let row = find_by_user_id(&conn, "u1").unwrap().expect("row");
        // Absent → v4's Zod default, not an error and not `false`.
        assert_eq!(row["customTools"], serde_json::Value::Bool(true));
        // The columns either side still land on their own values.
        assert_eq!(row["autoDetectRng"], serde_json::Value::Bool(true));
        assert_eq!(
            row["compositionModeDefault"],
            serde_json::Value::Bool(false)
        );
    }
}
