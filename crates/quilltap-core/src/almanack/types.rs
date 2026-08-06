//! The Almanack — report data model (v4 `lib/tools/almanack/types.ts`).
//!
//! One struct per v4 interface, gathered into [`AlmanackReportData`]. The
//! renderer ([`super::render`]) is a pure function of this object, which is what
//! makes the whole-document byte differential possible.
//!
//! ## Numbers are `f64`
//!
//! Every count here is a TypeScript `number`, and most arrive from a SQLite
//! aggregate through v4's `num()` coercion. Holding them as `f64` keeps the
//! renderer's `toLocaleString()` / `toFixed()` behaviour byte-identical for
//! fractional values (a mean, a ratio) instead of silently truncating.
//!
//! ## Serde shape
//!
//! `rename_all = "camelCase"` + `skip_serializing_if` on the genuinely optional
//! fields, so a serialized [`AlmanackReportData`] is byte-comparable with v4's
//! own JSON — which is exactly what the tier-1 render oracle feeds back in and
//! the tier-2 data differential diffs.

use serde::{Deserialize, Serialize};

// ============================================================================
// Phase 1 — Taking the measure of the premises
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEnvironmentInfo {
    pub node_version: String,
    pub platform: String,
    pub arch: String,
    pub os_type: String,
    pub os_release: String,
    pub total_memory_bytes: f64,
    pub free_memory_bytes: f64,
    /// v4 `'docker' | 'lima' | 'electron' | 'node'`.
    pub runtime_type: String,
    pub electron_shell_version: Option<String>,
    pub shell_capabilities: Vec<String>,
    pub uptime_seconds: f64,
    pub data_directory: String,
    pub timezone: String,
}

/// One database's on-disk footprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSizeInfo {
    pub label: String,
    pub size_bytes: f64,
    /// False when the file isn't there (fresh install, or the DB never opened).
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSecurityInfo {
    pub passphrase_protected: bool,
    /// Main, LLM logs, and mount index — three rows since 4.9.
    pub databases: Vec<DatabaseSizeInfo>,
    pub highest_app_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    // Field order follows v4's RUNTIME literal (`phase1-premises.ts:172-182` —
    // oldestDate before newestDate), not its interface; every struct in this
    // module keys serde order off the emitted object (§3 review, 2026-08-06).
    pub label: String,
    pub count: f64,
    pub oldest_date: Option<String>,
    pub newest_date: Option<String>,
    pub total_size_bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStateInfo {
    pub applied_count: f64,
    pub last_migration_id: Option<String>,
    pub last_migration_at: Option<String>,
    pub last_migration_version: Option<String>,
}

// ============================================================================
// Phase 2 — Cataloguing the machinery
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub name: String,
    pub title: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub enabled: bool,
    /// Installed from npm into the data dir, vs shipped in the app bundle.
    pub installed_from_npm: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityCount {
    pub capability: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginBreakdown {
    pub enabled: Vec<PluginInfo>,
    pub disabled: Vec<PluginInfo>,
    /// Count of plugins declaring each capability kind.
    pub by_capability: Vec<CapabilityCount>,
    pub npm_installed: f64,
    pub bundled: f64,
    pub plugin_config_rows: f64,
    pub character_plugin_data_rows: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub chat: bool,
    pub image_generation: bool,
    pub embeddings: bool,
    pub web_search: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
    pub configured: bool,
    pub capabilities: ProviderCapabilities,
    pub requires_api_key: bool,
    pub requires_base_url: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub provider: String,
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Freshness of the `provider_models` discovery cache, per provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelCacheRow {
    pub provider: String,
    pub total: f64,
    pub chat: f64,
    pub image: f64,
    pub embedding: f64,
    pub newest_updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyUsageRow {
    pub provider: String,
    pub total: f64,
    pub active: f64,
    pub never_used: f64,
    pub last_used: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageProviderInfo {
    pub provider: String,
    pub display_name: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingProviderInfo {
    pub provider: String,
    pub display_name: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServersInfo {
    pub configured: f64,
    pub enabled: f64,
    pub server_names: Vec<String>,
    pub auto_reconnect: bool,
    pub max_reconnect_attempts: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeListItem {
    pub name: String,
    pub version: String,
    pub source: String,
    /// How many canonical icons this theme overrides.
    pub icon_overrides: f64,
    /// Overrides naming something that is not a canonical icon — dead weight.
    pub unknown_icon_overrides: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeStats {
    pub total: f64,
    pub with_dark_mode: f64,
    pub with_css_overrides: f64,
    pub errors: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeReportInfo {
    pub active_theme_id: Option<String>,
    pub color_mode: String,
    pub stats: ThemeStats,
    pub themes: Vec<ThemeListItem>,
    pub themes_with_icons: f64,
    pub total_icon_overrides: f64,
    pub total_unknown_icon_overrides: f64,
}

// ============================================================================
// Phase 3 — Auditing the ledgers (main DB)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfileCensus {
    pub total: f64,
    pub web_search_enabled: f64,
    pub tool_use_enabled: f64,
    pub dangerous_compatible: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCensus {
    pub total: f64,
    pub built_in: f64,
    pub custom: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedDatabaseStats {
    pub characters: f64,
    pub favorite_characters: f64,
    pub chats: f64,
    pub memories: f64,
    pub tags: f64,
    pub projects: f64,
    pub groups: f64,
    pub connection_profiles: ConnectionProfileCensus,
    pub image_profiles: f64,
    pub embedding_profiles: f64,
    pub prompt_templates: TemplateCensus,
    pub roleplay_templates: TemplateCensus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTypeRow {
    pub chat_type: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantHistogramRow {
    pub participants: f64,
    pub chats: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatBreakdownInfo {
    pub by_type: Vec<ChatTypeRow>,
    /// Participant-count histogram: how many chats have N participants.
    pub participant_histogram: Vec<ParticipantHistogramRow>,
    pub paused_chats: f64,
    pub document_mode_chats: f64,
    pub chat_document_rows: f64,
    pub multi_document_chats: f64,
    pub chats_with_equipped_outfit: f64,
    pub pending_outfit_notification_chats: f64,
    pub narrative_timeline_chats: f64,
    pub chats_with_non_empty_state: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStateRow {
    pub run_state: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibilityRow {
    pub visibility: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutonomousRoomInfo {
    pub total: f64,
    pub by_run_state: Vec<RunStateRow>,
    pub scheduled: f64,
    pub overdue: f64,
    pub with_turn_budget: f64,
    pub with_token_budget: f64,
    pub with_wall_clock_budget: f64,
    pub with_spend_cap: f64,
    pub destructive_tools_allowed: f64,
    pub by_visibility: Vec<VisibilityRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryKindRow {
    pub kind: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySourceRow {
    pub source: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWitnessedRow {
    pub witnessed_context: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryBreakdownInfo {
    pub total: f64,
    pub by_kind: Vec<MemoryKindRow>,
    pub by_source: Vec<MemorySourceRow>,
    pub by_witnessed_context: Vec<MemoryWitnessedRow>,
    pub with_occurred_at: f64,
    pub with_narrative_time: f64,
    pub with_entities: f64,
    pub with_embedding: f64,
    pub reinforced_total: f64,
    pub max_reinforcement_count: f64,
    pub characters_with_memories: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterBreakdownInfo {
    pub total: f64,
    pub vault_linked: f64,
    pub vaultless: f64,
    pub npcs: f64,
    pub user_controlled: f64,
    pub carina_answerers: f64,
    pub system_transparent: f64,
    pub can_dress_themselves: f64,
    pub can_create_outfits: f64,
    pub core_whisper_overrides: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DangerousContentConfig {
    pub mode: String,
    pub threshold: f64,
    pub scan_text_chat: bool,
    pub scan_image_prompts: bool,
    pub scan_image_generation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompressionConfig {
    pub enabled: bool,
    pub window_size: f64,
    pub compression_target_tokens: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModeConfig {
    pub max_turns: f64,
    pub default_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryBackgroundsConfig {
    pub enabled: bool,
    pub has_default_image_profile: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampsConfig {
    pub mode: String,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoLockConfig {
    pub enabled: bool,
    pub idle_minutes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCascadeConfig {
    pub on_message_delete: String,
    pub on_swipe_regenerate: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarDisplayConfig {
    pub mode: String,
    pub style: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreWhisperConfig {
    pub enabled: bool,
    pub interval: f64,
    pub silence_threshold: f64,
    pub packet_token_budget: f64,
    pub fire_on_context_transition: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingDisplayConfig {
    pub default_visible: bool,
    pub default_collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerConfirmationConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoHousekeepingConfig {
    pub enabled: bool,
    pub per_character_cap: f64,
    pub merge_similar: bool,
    pub auto_merge_similar_threshold: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextReplacementsConfig {
    pub enabled: bool,
    pub rules: f64,
    pub enabled_rules: f64,
}

/// The feature dials, as configured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureConfigInfo {
    pub dangerous_content: DangerousContentConfig,
    pub context_compression: ContextCompressionConfig,
    pub agent_mode: AgentModeConfig,
    pub story_backgrounds: StoryBackgroundsConfig,
    pub timestamps: TimestampsConfig,
    pub auto_lock: AutoLockConfig,
    pub memory_cascade: MemoryCascadeConfig,
    pub auto_detect_rng: bool,
    pub custom_tools: bool,
    pub avatar_display: AvatarDisplayConfig,
    pub core_whisper: CoreWhisperConfig,
    pub thinking_display: ThinkingDisplayConfig,
    pub answer_confirmation: AnswerConfirmationConfig,
    /// v4 `Record<string, unknown>` — carried opaquely (nothing renders it).
    pub autonomous_room_defaults: serde_json::Value,
    pub auto_housekeeping: AutoHousekeepingConfig,
    pub text_replacements: TextReplacementsConfig,
    pub composer_spellcheck: bool,
    pub auto_scroll_on_response_complete: bool,
    pub image_description_profile_configured: bool,
    pub uncensored_image_description_profile_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallConfig {
    pub scope_policy: String,
    pub expand_related: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryExtractionLimitsConfig {
    pub enabled: bool,
    pub max_per_hour: f64,
    pub soft_start_fraction: f64,
    pub soft_floor: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSettingsInfo {
    pub stale_chat_days: f64,
    pub chats_eligible_for_next_sweep: f64,
    pub max_concurrent_jobs: f64,
    pub memory_recall: MemoryRecallConfig,
    pub memory_extraction_limits: MemoryExtractionLimitsConfig,
    pub last_maintenance_sweep_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatusRow {
    pub status: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTypeRow {
    #[serde(rename = "type")]
    pub job_type: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedJobRow {
    #[serde(rename = "type")]
    pub job_type: String,
    pub count: f64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundJobInfo {
    pub by_status: Vec<JobStatusRow>,
    pub by_type: Vec<JobTypeRow>,
    pub failed: Vec<FailedJobRow>,
    pub attempts_exhausted: f64,
    pub oldest_pending_scheduled_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingStatusRow {
    pub entity_type: String,
    pub status: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedTableCensus {
    pub total: f64,
    pub unembedded: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredDimensionRow {
    pub table: String,
    pub dimensions: f64,
    pub vectors: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingPipelineInfo {
    pub status_by_entity_type: Vec<EmbeddingStatusRow>,
    pub permanently_failed: f64,
    pub conversation_chunks: EmbeddedTableCensus,
    pub help_docs: EmbeddedTableCensus,
    /// Distinct stored vector widths, decoded from the self-describing header.
    pub stored_dimensions: Vec<StoredDimensionRow>,
    pub active_profile_dimensions: Option<f64>,
    pub dimension_mismatch: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub total_sessions: f64,
    pub live_sessions: f64,
    pub non_zero_exits: f64,
    pub distinct_shells: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderStats {
    pub path: String,
    pub file_count: f64,
    pub total_size: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedImageRow {
    pub generation_model: String,
    pub count: f64,
    pub bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub total_files: f64,
    pub total_size: f64,
    pub folders: Vec<FolderStats>,
    pub not_ok_files: f64,
    pub generated_images_by_model: Vec<GeneratedImageRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStatsInfo {
    #[serde(rename = "totalEstimatedCostUSD")]
    pub total_estimated_cost_usd: f64,
    pub total_prompt_tokens: f64,
    pub total_completion_tokens: f64,
    pub total_messages: f64,
    pub agent_mode_chats: f64,
    pub dangerous_chats: f64,
}

// ============================================================================
// Phase 4 — Touring the Scriptorium (mount index DB)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountPointSummaryRow {
    pub mount_type: String,
    pub store_type: String,
    pub count: f64,
    pub enabled: f64,
    pub file_count: f64,
    pub chunk_count: f64,
    pub total_size_bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WellKnownMountInfo {
    pub label: String,
    pub setting_key: String,
    pub mount_point_id: Option<String>,
    pub name: Option<String>,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCountRow {
    pub status: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptoriumMountPoints {
    pub total: f64,
    pub enabled: f64,
    pub by_kind: Vec<MountPointSummaryRow>,
    pub scan_errors: f64,
    pub conversion_errors: f64,
    pub scan_statuses: Vec<StatusCountRow>,
    pub conversion_statuses: Vec<StatusCountRow>,
    pub well_known: Vec<WellKnownMountInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobMimeRow {
    pub stored_mime_type: String,
    pub count: f64,
    pub bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptoriumContent {
    pub file_rows: f64,
    pub link_rows: f64,
    pub document_rows: f64,
    pub document_text_bytes: f64,
    pub blob_rows: f64,
    pub blob_bytes: f64,
    pub blobs_by_mime_type: Vec<BlobMimeRow>,
    pub chunk_rows: f64,
    pub unembedded_chunks: f64,
    pub chunk_tokens: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptoriumLinks {
    /// links ÷ files — how much the content-addressed store is deduping.
    pub dedup_ratio: f64,
    pub hard_link_groups: f64,
    pub hard_linked_links: f64,
    pub extraction_errors: f64,
    pub conversion_errors: f64,
    pub policy_embed_denied: f64,
    pub policy_character_read_denied: f64,
    pub policy_character_write_denied: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterVaultHealth {
    pub total: f64,
    pub with_keystone: f64,
    pub missing_keystone: f64,
    pub with_metadata: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeTierRow {
    pub tier: String,
    pub items: f64,
    pub archived: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolStoreRow {
    pub store: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolParseFailure {
    pub store: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolsInfo {
    pub total: f64,
    pub by_store: Vec<CustomToolStoreRow>,
    pub with_presets: f64,
    pub preset_files: f64,
    pub with_llm_consult: f64,
    pub with_effects: f64,
    pub metadata_gated: f64,
    pub parse_failures: f64,
    pub parse_failure_detail: Vec<CustomToolParseFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostOfficeInfo {
    pub letters: f64,
    pub unannounced: f64,
    pub mailboxes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosInfo {
    pub character_vault_photos: f64,
    pub character_vault_bytes: f64,
    pub user_gallery_photos: f64,
    pub user_gallery_bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioTierRow {
    pub tier: String,
    pub count: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateCascadeInfo {
    pub chats_with_state: f64,
    pub projects_with_state: f64,
    pub groups_with_state: f64,
    pub general_state_present: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptoriumInfo {
    pub available: bool,
    pub mount_points: ScriptoriumMountPoints,
    pub content: ScriptoriumContent,
    pub links: ScriptoriumLinks,
    pub character_vaults: CharacterVaultHealth,
    pub wardrobe: Vec<WardrobeTierRow>,
    pub custom_tools: CustomToolsInfo,
    pub post_office: PostOfficeInfo,
    pub photos: PhotosInfo,
    pub scenarios: Vec<ScenarioTierRow>,
    pub state_cascade: StateCascadeInfo,
}

// ============================================================================
// Phase 5 — Assembling the dramatis personae
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopCharacterRow {
    pub name: String,
    pub chats: f64,
    pub memories: f64,
    /// Vault size as of that store's last scan; bytes may be shared.
    pub storage_bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub name: String,
    pub linked_stores: f64,
    pub chats: f64,
    pub files: f64,
    pub documents: f64,
    pub has_state: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRow {
    pub name: String,
    pub members: Vec<String>,
    pub linked_stores: f64,
    pub has_official_store: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaeInfo {
    pub top_characters: Vec<TopCharacterRow>,
    /// Whether participants with `status: 'removed'` were counted.
    pub counts_removed_participants: bool,
    pub projects: Vec<ProjectRow>,
    pub groups: Vec<GroupRow>,
}

// ============================================================================
// Phase 6 — Reading the wire records (llm-logs DB)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTypeRow {
    #[serde(rename = "type")]
    pub log_type: String,
    pub requests: f64,
    pub prompt_tokens: f64,
    pub completion_tokens: f64,
    pub total_tokens: f64,
    pub measured_requests: f64,
    pub avg_duration_ms: Option<f64>,
    pub failures: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLifetimeRow {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_name: String,
    pub total_tokens: f64,
    pub total_prompt_tokens: f64,
    pub total_completion_tokens: f64,
    pub message_count: f64,
    /// `updatedAt` — the closest thing to a last-used stamp the table carries.
    pub last_touched_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileWindowRow {
    pub label: String,
    pub provider: String,
    pub model_name: String,
    pub requests: f64,
    pub prompt_tokens: f64,
    pub completion_tokens: f64,
    pub total_tokens: f64,
    pub measured_requests: f64,
    pub avg_duration_ms: Option<f64>,
    pub median_duration_ms: Option<f64>,
    pub failures: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheRow {
    pub label: String,
    pub rows_with_cache_usage: f64,
    pub rows_with_cache_read: f64,
    pub cache_read_tokens: f64,
    pub cache_creation_tokens: f64,
    pub uncached_prompt_tokens: f64,
    pub request_hit_ratio: f64,
    pub token_hit_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub prompt_tokens: f64,
    pub completion_tokens: f64,
    pub total_tokens: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireRecordsInfo {
    pub total_entries: f64,
    pub token_usage: TokenUsage,
    pub logging_enabled: bool,
    pub verbose_mode: bool,
    pub retention_days: f64,
    /// False when the 4.9 attribution columns aren't present, so joins are
    /// approximate.
    pub exact_profile_attribution: bool,
    pub by_type: Vec<WireTypeRow>,
    pub connection_profile_lifetime: Vec<ProfileLifetimeRow>,
    pub connection_profile_window: Vec<ProfileWindowRow>,
    pub image_profile_window: Vec<ProfileWindowRow>,
    pub cache_by_provider: Vec<CacheRow>,
    pub cache_by_profile: Vec<CacheRow>,
}

// ============================================================================
// Cost configuration (the designated background workers)
// ============================================================================

/// v4 `DesignatedProfileInfo` — every field optional; `{}` when unconfigured.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignatedProfileInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
}

// ============================================================================
// The whole volume
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlmanackReportData {
    pub version: String,
    pub node_env: String,
    pub generated_at: String,

    // Phase 1
    pub runtime_environment: RuntimeEnvironmentInfo,
    pub database_security: DatabaseSecurityInfo,
    pub backup_status: Vec<BackupInfo>,
    pub migration_state: MigrationStateInfo,

    // Phase 2
    pub plugins: PluginBreakdown,
    pub api_key_types: Vec<String>,
    pub providers: Vec<ProviderInfo>,
    pub models_by_provider: Vec<ModelInfo>,
    pub provider_model_cache: Vec<ProviderModelCacheRow>,
    pub api_key_usage: Vec<ApiKeyUsageRow>,
    #[serde(rename = "cheapLLM")]
    pub cheap_llm: DesignatedProfileInfo,
    #[serde(rename = "imagePromptLLM")]
    pub image_prompt_llm: DesignatedProfileInfo,
    pub embedding_provider: DesignatedProfileInfo,
    pub image_providers: Vec<ImageProviderInfo>,
    pub embedding_providers: Vec<EmbeddingProviderInfo>,
    pub mcp_servers: McpServersInfo,
    pub theme_info: ThemeReportInfo,

    // Phase 3
    pub database_stats: EnhancedDatabaseStats,
    pub chat_stats: ChatStatsInfo,
    pub chat_breakdown: ChatBreakdownInfo,
    pub autonomous_rooms: AutonomousRoomInfo,
    pub memory_breakdown: MemoryBreakdownInfo,
    pub character_breakdown: CharacterBreakdownInfo,
    pub feature_config: FeatureConfigInfo,
    pub instance_settings: InstanceSettingsInfo,
    pub background_jobs: BackgroundJobInfo,
    pub embedding_pipeline: EmbeddingPipelineInfo,
    pub terminal: TerminalInfo,
    pub storage_stats: StorageStats,

    // Phase 4
    pub scriptorium: ScriptoriumInfo,

    // Phase 5
    pub personae: PersonaeInfo,

    // Phase 6
    pub wire_records: WireRecordsInfo,
}
