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
//!     to `{}` in the corpus — a non-empty `tagStyles` would be a multi-key
//!     open-JSON object whose key order (`serde_json::Value` sorts; v4's
//!     `JSON.stringify` is insertion order) is a tracked deferred seam (see the
//!     `connection_profiles` `parameters` deferral). The `{}` case agrees
//!     trivially.
//!   - **~15 nested typed-struct JSON-object columns** (`cheapLLMSettings`,
//!     `themePreference`, `defaultTimestampConfig`, `memoryCascadePreferences`,
//!     `autoHousekeepingSettings`, `memoryExtractionLimits`,
//!     `autonomousRoomSettings`, `tokenDisplaySettings`,
//!     `contextCompressionSettings`, `llmLoggingSettings`, `agentModeSettings`,
//!     `coreWhisper`, `thinkingDisplay`, `answerConfirmationSettings`,
//!     `storyBackgroundsSettings`, `dangerousContentSettings`, `autoLockSettings`).
//!     Each is reproduced
//!     byte-for-byte with a serde struct in **schema field order** (NOT
//!     `serde_json::Value`, which would sort keys and diverge from v4's
//!     `JSON.stringify(zodParsed)`, whose key order is the Zod schema's field
//!     order). This extends the `tags.visualStyle` typed-struct rule across many
//!     columns at once.
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
//!   - **five boolean columns** → INTEGER 0/1 (`i64::from(bool)`):
//!     `autoDetectRng`, `compositionModeDefault`, `composerSpellcheck`,
//!     `textReplacementsEnabled`, `autoScrollOnResponseComplete`.
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
// whose key order is the Zod schema's field order). `serde_json::Value` is NOT
// used for these — its BTreeMap would sort keys and diverge from v4.
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
pub struct ChatSettingsCreate {
    pub user_id: String,
    /// Enum TEXT (`AvatarDisplayModeEnum`).
    pub avatar_display_mode: String,
    /// Plain-string-default TEXT.
    pub avatar_display_style: String,
    /// Record/map JSON-object column → compact JSON text. CONSTRAINED to `{}`
    /// (multi-key open-JSON key-order seam).
    pub tag_styles: serde_json::Value,
    pub cheap_llm_settings: CheapLlmSettings,
    /// Nullable UUID TEXT; `None` => SQL NULL.
    pub image_description_profile_id: Option<String>,
    /// Nullable UUID TEXT; `None` => SQL NULL.
    pub uncensored_image_description_profile_id: Option<String>,
    /// Nullable UUID TEXT; `None` => SQL NULL.
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

    /// Insert chat settings with the given pinned id + timestamps. All ~33
    /// columns are written explicitly in schema order; the JSON-object columns
    /// bind compact JSON text (schema key order), the boolean columns bind
    /// `i64::from(bool)`, `sidebarWidth` binds `i64` (INTEGER affinity), the
    /// nullable columns bind `Option<String>` (`None` → SQL NULL).
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

        self.conn.execute(
            "INSERT INTO chat_settings \
               (id, userId, avatarDisplayMode, avatarDisplayStyle, tagStyles, cheapLLMSettings, \
                imageDescriptionProfileId, uncensoredImageDescriptionProfileId, \
                defaultRoleplayTemplateId, themePreference, sidebarWidth, defaultTimestampConfig, \
                memoryCascadePreferences, autoHousekeepingSettings, memoryExtractionLimits, \
                autonomousRoomSettings, tokenDisplaySettings, contextCompressionSettings, \
                llmLoggingSettings, autoDetectRng, compositionModeDefault, composerSpellcheck, \
                textReplacementsEnabled, autoScrollOnResponseComplete, agentModeSettings, \
                coreWhisper, thinkingDisplay, storyBackgroundsSettings, dangerousContentSettings, \
                autoLockSettings, timezone, createdAt, updatedAt, answerConfirmationSettings) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34)",
            params![
                opts.id,
                data.user_id,
                data.avatar_display_mode,
                data.avatar_display_style,
                tag_styles,
                cheap_llm_settings,
                data.image_description_profile_id,
                data.uncensored_image_description_profile_id,
                data.default_roleplay_template_id,
                theme_preference,
                data.sidebar_width,
                default_timestamp_config,
                memory_cascade_preferences,
                auto_housekeeping_settings,
                memory_extraction_limits,
                autonomous_room_settings,
                token_display_settings,
                context_compression_settings,
                llm_logging_settings,
                i64::from(data.auto_detect_rng),
                i64::from(data.composition_mode_default),
                i64::from(data.composer_spellcheck),
                i64::from(data.text_replacements_enabled),
                i64::from(data.auto_scroll_on_response_complete),
                agent_mode_settings,
                core_whisper,
                thinking_display,
                story_backgrounds_settings,
                dangerous_content_settings,
                auto_lock_settings,
                data.timezone,
                opts.created_at,
                opts.updated_at,
                answer_confirmation_settings,
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
