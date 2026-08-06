//! The Almanack (System Report) — v4 `lib/tools/almanack/**` (`0cde7fbc`).
//!
//! An annual compendium of the establishment: what is installed, what is
//! configured, and what has accumulated. Formerly "the capabilities report";
//! renamed in v4 4.9 when its coverage was brought up to date with the
//! document-store cutovers it had never learned about. v5 never ported the old
//! collector (M6 parity row 578 said MISSING), so this is a port of the surface
//! v4 actually ships rather than of the thing it replaced.
//!
//! The pipeline walks the seven phases of [`phases::ALMANACK_PHASES`] in order,
//! announcing each on entry so the progress bar can name the one that is
//! actually running. Every collector is wrapped in [`db::collect`], which
//! swallows and logs failures: a report that refuses to render because one
//! section's query threw is worse than a report with one hollow section.
//!
//! v4 joins each phase's collectors in a `Promise.all` because they are
//! `await`ed database round-trips; v5's are synchronous pooled reads, so they
//! run in sequence within a phase. Same queries, same order of results, no
//! observable difference — a wall-clock detail, not a behavioural one.
//!
//! ## Divergences from v4, recorded (all differential-normalized)
//!
//! - **`runtimeEnvironment` reports v5's own runtime.** The section is a
//!   truthful description of the process that produced the report, so a Rust
//!   host reports Rust facts (no `process.version`; `nodeVersion` carries the
//!   host runtime string). The labels are byte-matched; the tier-2 differential
//!   normalizes the whole block, exactly as it normalizes `generatedAt`.
//! - **`version` is v5's** crate version, not v4's `package.json`.
//! - **No `capabilities-report-progress` SSE action.** v5 has ONE global
//!   `/api/events` stream, so progress frames ride it scope-tagged by
//!   `progressId` — the standing D3-family divergence the Green Room
//!   established ([`crate::services::creation_progress`]). The new `phase`
//!   frame kind is emitted there.
//! - **The llm-logs aggregates live here**, as raw SQL over the llm-logs
//!   connection, rather than on a user-scoped repository layer v5 does not
//!   have (a structural no-op — the SQL is v4's, byte-for-byte).

pub mod db;
pub mod phase1_premises;
pub mod phase2_machinery;
pub mod phase3_ledgers;
pub mod phase4_scriptorium;
pub mod phase5_personae;
pub mod phase6_wire_records;
pub mod phases;
pub mod render;
pub mod types;

pub use phase1_premises::{AlmanackPaths, RuntimeFacts};
pub use phases::{
    phase_index, AlmanackPhase, ALMANACK_NAME, ALMANACK_PHASES, ALMANACK_PHASE_COUNT,
    ALMANACK_TITLE,
};
pub use render::render_almanack_markdown;
pub use types::AlmanackReportData;

use crate::db::runtime::Db;
use crate::provider_manifest::Registry;
use crate::services::creation_progress::CreationProgressEmitter;

/// Everything the report pipeline needs that it cannot discover for itself. v4
/// reaches for `lib/paths`, `process`, `os` and `package.json` directly; keeping
/// them as an input is what lets the differential pin every machine-dependent
/// value on both sides.
pub struct AlmanackContext<'a> {
    pub db: &'a Db,
    pub registry: &'a Registry,
    pub user_id: &'a str,
    pub paths: AlmanackPaths,
    pub facts: RuntimeFacts,
    /// v4 `getHasUserPassphrase()`.
    pub passphrase_protected: bool,
    /// v4 `packageJson.version` — v5's own (a recorded divergence).
    pub version: String,
    /// v4 `process.env.NODE_ENV || 'development'`.
    pub node_env: String,
    /// v4's `new Date()` for `generatedAt` and the autonomous-room overdue
    /// boundary, injected so the differential can freeze it.
    pub now_ms: i64,
}

/// Announce entry into a phase on the progress channel (v4 `announce`, 1-based).
/// An unknown key is silently ignored, exactly as v4's `findIndex(...) < 0`
/// guard does.
fn announce(progress: &CreationProgressEmitter, key: &str) {
    let Some(index) = phase_index(key) else {
        return;
    };
    let phase = &ALMANACK_PHASES[index - 1];
    progress.phase(key, index as i64, ALMANACK_PHASE_COUNT as i64, phase.label);
}

/// Announce the seventh phase, "Binding the volume" — the one the write side
/// enters after the data is gathered. Public because `generate_and_save` lives
/// in the API layer (see below).
pub fn announce_binding(progress: &CreationProgressEmitter) {
    announce(progress, "binding");
}

/// v4's filename scheme: `almanack-<ISO with ':' and '.' replaced by '-'>.md`.
pub fn report_filename(now_ms: i64) -> String {
    let iso = crate::clock::iso_from_unix_ms(now_ms);
    let stamped: String = iso
        .chars()
        .map(|c| if c == ':' || c == '.' { '-' } else { c })
        .collect();
    format!("almanack-{stamped}.md")
}

/// v4 `generateAlmanackData(userId, progressId?)` — gather the whole report.
///
/// `progress` is inert when the caller supplied no `progressId`, so the pipeline
/// behaves exactly as it would have before progress existed (v4's
/// `createOperationProgressEmitter(undefined)` → `NOOP_EMITTER`).
///
/// Phase 7 ("Binding the volume") is the write side and lives with the caller —
/// see [`announce_binding`] and `api::almanack`. Reads never write, which is why
/// the split falls here rather than at v4's `persist` callback.
pub fn generate_almanack_data(
    ctx: &AlmanackContext<'_>,
    progress: &CreationProgressEmitter,
) -> AlmanackReportData {
    use db::collect;
    use types::*;

    let d = ctx.db;
    let user = ctx.user_id;

    // --- Phase 1: Taking the measure of the premises ------------------------
    announce(progress, "premises");
    // `collect_runtime_environment` reads only host-supplied facts, so it needs
    // no fallback shape of its own (v4 says the same of its `os`/`process` read).
    let runtime_environment = phase1_premises::collect_runtime_environment(&ctx.facts, &ctx.paths);
    // Error-arm divergence, recorded (§3 review, 2026-08-06): v4 wraps these
    // three in `collect()` (whole-section fallback on throw); the v5 shapes are
    // infallible-by-construction (per-query soft reads), so a dead main DB
    // still stats the three files and keeps the host's passphrase flag where
    // v4 would answer `databases: []` / `passphraseProtected: false`.
    let database_security =
        phase1_premises::collect_database_security(d, &ctx.paths, ctx.passphrase_protected);
    let backup_status = phase1_premises::collect_backup_status(&ctx.paths);
    let migration_state = phase1_premises::collect_migration_state(d);

    // --- Phase 2: Cataloguing the machinery ---------------------------------
    announce(progress, "machinery");
    let plugins = collect(
        "plugins",
        PluginBreakdown {
            enabled: Vec::new(),
            disabled: Vec::new(),
            by_capability: Vec::new(),
            npm_installed: 0.0,
            bundled: 0.0,
            plugin_config_rows: 0.0,
            character_plugin_data_rows: 0.0,
        },
        || phase2_machinery::collect_plugin_info(d),
    );
    let providers = collect("providers", Vec::new(), || {
        phase2_machinery::collect_provider_info(d, ctx.registry, user)
    });
    let models_by_provider = collect("models", Vec::new(), || {
        phase2_machinery::collect_models(d, ctx.registry, user)
    });
    let provider_model_cache = collect("providerModelCache", Vec::new(), || {
        phase2_machinery::collect_provider_model_cache(d)
    });
    let api_key_usage = collect("apiKeyUsage", Vec::new(), || {
        phase2_machinery::collect_api_key_usage(d, user)
    });
    let cheap_llm = collect("cheapLLM", DesignatedProfileInfo::default(), || {
        phase2_machinery::collect_cheap_llm_info(d, user)
    });
    let image_prompt_llm = collect("imagePromptLLM", DesignatedProfileInfo::default(), || {
        phase2_machinery::collect_image_prompt_llm_info(d, user)
    });
    let embedding_provider = collect(
        "embeddingProvider",
        DesignatedProfileInfo::default(),
        || phase2_machinery::collect_embedding_info(d, user),
    );
    let image_providers = collect("imageProviders", Vec::new(), || {
        phase2_machinery::collect_image_providers(d, ctx.registry)
    });
    let embedding_providers = collect("embeddingProviders", Vec::new(), || {
        phase2_machinery::collect_embedding_providers(d, ctx.registry)
    });
    let mcp_servers = collect(
        "mcpServers",
        McpServersInfo {
            configured: 0.0,
            enabled: 0.0,
            server_names: Vec::new(),
            auto_reconnect: true,
            max_reconnect_attempts: 5.0,
        },
        || phase2_machinery::collect_mcp_servers(d, user),
    );
    let theme_info = collect(
        "themeInfo",
        ThemeReportInfo {
            active_theme_id: None,
            color_mode: "system".into(),
            stats: ThemeStats {
                total: 0.0,
                with_dark_mode: 0.0,
                with_css_overrides: 0.0,
                errors: 0.0,
            },
            themes: Vec::new(),
            themes_with_icons: 0.0,
            total_icon_overrides: 0.0,
            total_unknown_icon_overrides: 0.0,
        },
        || phase2_machinery::collect_theme_info(d, user),
    );

    // --- Phase 3: Auditing the ledgers --------------------------------------
    announce(progress, "ledgers");
    let now_iso = crate::clock::iso_from_unix_ms(ctx.now_ms);
    let database_stats = collect(
        "databaseStats",
        EnhancedDatabaseStats {
            characters: 0.0,
            favorite_characters: 0.0,
            chats: 0.0,
            memories: 0.0,
            tags: 0.0,
            projects: 0.0,
            groups: 0.0,
            connection_profiles: ConnectionProfileCensus {
                total: 0.0,
                web_search_enabled: 0.0,
                tool_use_enabled: 0.0,
                dangerous_compatible: 0.0,
            },
            image_profiles: 0.0,
            embedding_profiles: 0.0,
            prompt_templates: TemplateCensus {
                total: 0.0,
                built_in: 0.0,
                custom: 0.0,
            },
            roleplay_templates: TemplateCensus {
                total: 0.0,
                built_in: 0.0,
                custom: 0.0,
            },
        },
        || phase3_ledgers::collect_database_stats(d, user),
    );
    let chat_stats = collect(
        "chatStats",
        ChatStatsInfo {
            total_estimated_cost_usd: 0.0,
            total_prompt_tokens: 0.0,
            total_completion_tokens: 0.0,
            total_messages: 0.0,
            agent_mode_chats: 0.0,
            dangerous_chats: 0.0,
        },
        || phase3_ledgers::collect_chat_stats(d, user),
    );
    let chat_breakdown = collect(
        "chatBreakdown",
        ChatBreakdownInfo {
            by_type: Vec::new(),
            participant_histogram: Vec::new(),
            paused_chats: 0.0,
            document_mode_chats: 0.0,
            chat_document_rows: 0.0,
            multi_document_chats: 0.0,
            chats_with_equipped_outfit: 0.0,
            pending_outfit_notification_chats: 0.0,
            narrative_timeline_chats: 0.0,
            chats_with_non_empty_state: 0.0,
        },
        || phase3_ledgers::collect_chat_breakdown(d, user),
    );
    let autonomous_rooms = collect(
        "autonomousRooms",
        AutonomousRoomInfo {
            total: 0.0,
            by_run_state: Vec::new(),
            scheduled: 0.0,
            overdue: 0.0,
            with_turn_budget: 0.0,
            with_token_budget: 0.0,
            with_wall_clock_budget: 0.0,
            with_spend_cap: 0.0,
            destructive_tools_allowed: 0.0,
            by_visibility: Vec::new(),
        },
        || phase3_ledgers::collect_autonomous_rooms(d, user, &now_iso),
    );
    let (memory_breakdown, memory_by_character) = collect(
        "memoryBreakdown",
        (
            MemoryBreakdownInfo {
                total: 0.0,
                by_kind: Vec::new(),
                by_source: Vec::new(),
                by_witnessed_context: Vec::new(),
                with_occurred_at: 0.0,
                with_narrative_time: 0.0,
                with_entities: 0.0,
                with_embedding: 0.0,
                reinforced_total: 0.0,
                max_reinforcement_count: 0.0,
                characters_with_memories: 0.0,
            },
            std::collections::HashMap::new(),
        ),
        || phase3_ledgers::collect_memory_breakdown(d),
    );
    let character_breakdown = collect(
        "characterBreakdown",
        CharacterBreakdownInfo {
            total: 0.0,
            vault_linked: 0.0,
            vaultless: 0.0,
            npcs: 0.0,
            user_controlled: 0.0,
            carina_answerers: 0.0,
            system_transparent: 0.0,
            can_dress_themselves: 0.0,
            can_create_outfits: 0.0,
            core_whisper_overrides: 0.0,
        },
        || phase3_ledgers::collect_character_breakdown(d, user),
    );
    let feature_config = collect(
        "featureConfig",
        phase3_ledgers::default_feature_config(),
        || phase3_ledgers::collect_feature_config(d, user),
    );
    let instance_settings = collect(
        "instanceSettings",
        InstanceSettingsInfo {
            stale_chat_days: 30.0,
            chats_eligible_for_next_sweep: 0.0,
            max_concurrent_jobs: 4.0,
            memory_recall: MemoryRecallConfig {
                scope_policy: "down-weight".into(),
                expand_related: false,
            },
            memory_extraction_limits: MemoryExtractionLimitsConfig {
                enabled: false,
                max_per_hour: 20.0,
                soft_start_fraction: 0.7,
                soft_floor: 0.7,
            },
            last_maintenance_sweep_at: None,
        },
        || phase3_ledgers::collect_instance_settings(d, user, ctx.now_ms),
    );
    let background_jobs = collect(
        "backgroundJobs",
        BackgroundJobInfo {
            by_status: Vec::new(),
            by_type: Vec::new(),
            failed: Vec::new(),
            attempts_exhausted: 0.0,
            oldest_pending_scheduled_at: None,
        },
        || phase3_ledgers::collect_background_jobs(d, user),
    );
    let embedding_pipeline = collect(
        "embeddingPipeline",
        EmbeddingPipelineInfo {
            status_by_entity_type: Vec::new(),
            failed: 0.0,
            conversation_chunks: EmbeddedTableCensus {
                total: 0.0,
                unembedded: 0.0,
            },
            help_docs: EmbeddedTableCensus {
                total: 0.0,
                unembedded: 0.0,
            },
            stored_dimensions: Vec::new(),
            active_profile_dimensions: None,
            dimension_mismatch: false,
        },
        || phase3_ledgers::collect_embedding_pipeline(d, user),
    );
    let terminal = collect(
        "terminal",
        TerminalInfo {
            total_sessions: 0.0,
            live_sessions: 0.0,
            non_zero_exits: 0.0,
            distinct_shells: Vec::new(),
        },
        || phase3_ledgers::collect_terminal(d),
    );
    let storage_stats = collect(
        "storageStats",
        StorageStats {
            total_files: 0.0,
            total_size: 0.0,
            folders: Vec::new(),
            not_ok_files: 0.0,
            generated_images_by_model: Vec::new(),
        },
        || phase3_ledgers::collect_storage_stats(d, user),
    );

    // --- Phase 4: Touring the Scriptorium -----------------------------------
    announce(progress, "scriptorium");
    let scriptorium = collect(
        "scriptorium",
        phase4_scriptorium::empty_scriptorium(),
        || {
            phase4_scriptorium::collect_scriptorium(
                d,
                user,
                chat_breakdown.chats_with_non_empty_state,
            )
        },
    );

    // --- Phase 5: Assembling the dramatis personae --------------------------
    announce(progress, "personae");
    let personae = collect(
        "personae",
        PersonaeInfo {
            top_characters: Vec::new(),
            counts_removed_participants: true,
            projects: Vec::new(),
            groups: Vec::new(),
        },
        || phase5_personae::collect_personae(d, user, &memory_by_character),
    );

    // --- Phase 6: Reading the wire records ----------------------------------
    announce(progress, "wire-records");
    let wire_records = collect(
        "wireRecords",
        WireRecordsInfo {
            total_entries: 0.0,
            token_usage: TokenUsage {
                prompt_tokens: 0.0,
                completion_tokens: 0.0,
                total_tokens: 0.0,
            },
            logging_enabled: true,
            verbose_mode: false,
            retention_days: 30.0,
            exact_profile_attribution: false,
            by_type: Vec::new(),
            connection_profile_lifetime: Vec::new(),
            connection_profile_window: Vec::new(),
            image_profile_window: Vec::new(),
            cache_by_provider: Vec::new(),
            cache_by_profile: Vec::new(),
        },
        || phase6_wire_records::collect_wire_records(d, user),
    );

    AlmanackReportData {
        version: ctx.version.clone(),
        node_env: ctx.node_env.clone(),
        generated_at: crate::clock::iso_from_unix_ms(ctx.now_ms),

        runtime_environment,
        database_security,
        backup_status,
        migration_state,

        plugins,
        api_key_types: phase2_machinery::collect_api_key_types(ctx.registry),
        providers,
        models_by_provider,
        provider_model_cache,
        api_key_usage,
        cheap_llm,
        image_prompt_llm,
        embedding_provider,
        image_providers,
        embedding_providers,
        mcp_servers,
        theme_info,

        database_stats,
        chat_stats,
        chat_breakdown,
        autonomous_rooms,
        memory_breakdown,
        character_breakdown,
        feature_config,
        instance_settings,
        background_jobs,
        embedding_pipeline,
        terminal,
        storage_stats,

        scriptorium,

        personae,

        wire_records,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_matches_v4_scheme() {
        // v4: `almanack-${new Date().toISOString().replace(/[:.]/g, '-')}.md`.
        assert_eq!(
            report_filename(1_785_931_200_000),
            "almanack-2026-08-05T12-00-00-000Z.md"
        );
    }
}
