//! The Almanack — Phase 3, "Auditing the ledgers"
//! (v4 `lib/tools/almanack/phase3-ledgers.ts`).
//!
//! The main database: the census of every collection, the shape of the chats,
//! the state of the autonomous rooms, the Commonplace Book's contents, every
//! feature dial, the job queue, the embedding pipeline, the terminal and the
//! legacy file ledger.
//!
//! Everything here is a SQL roll-up, transcribed from v4's SQL verbatim. v4's
//! old collector hydrated whole entities to count them — one query per character
//! for memory counts alone — which is what made a report on a large instance
//! take minutes.
//!
//! ## ⚠ One recorded divergence — prompt templates are not seeded
//!
//! v4's `promptTemplates.findAllForUser()` calls `seedSamplePrompts()` first, so
//! generating a report can WRITE built-in template rows. v5's report is strictly
//! read-only: it counts the rows that are there. A fixture (or instance) whose
//! built-ins have already been seeded — which is every instance that has ever
//! listed templates — reads identically on both sides, and the differential's
//! fixture is built through v4's own path so the seeded rows exist.

use std::collections::HashMap;

use rusqlite::types::Value as SqlValue;
use serde_json::Value;

use crate::db::runtime::Db;
use crate::db::{embedding_profiles, instance_settings, roleplay_templates, DbError};
use crate::services::maintenance::is_stale_conn;
use crate::services::queue_service::{resolve_stale_chat_days_conn, retention_cutoff_iso};

use super::db::{main_row, main_rows, num_col, opt_text, text};
use super::types::*;

/// `COUNT(*)` for a table, 0 when the table is missing or the query fails
/// (v4 `countRows`).
fn count_rows(db: &Db, table: &str, where_clause: &str, params: &[&dyn rusqlite::ToSql]) -> f64 {
    main_row(
        db,
        &format!(r#"SELECT COUNT(*) AS n FROM "{table}" {where_clause}"#),
        params,
        &format!("ledgers.count:{table}"),
        |r| num_col(r, 0),
    )
    .unwrap_or(0.0)
}

/// JS `x ?? default` over a JSON path — absent AND `null` both take the default,
/// which is what `??` means and what every dial below relies on.
fn jbool(v: &Value, path: &[&str], default: bool) -> bool {
    jget(v, path).and_then(Value::as_bool).unwrap_or(default)
}
fn jnum(v: &Value, path: &[&str], default: f64) -> f64 {
    jget(v, path).and_then(Value::as_f64).unwrap_or(default)
}
fn jstr(v: &Value, path: &[&str], default: &str) -> String {
    jget(v, path)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}
/// Truthiness of a string field — v4's `!!chatSettings?.someId` (an empty string
/// is falsy in JS, so it is here too).
fn jtruthy(v: &Value, path: &[&str]) -> bool {
    match jget(v, path) {
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}
fn jget<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
        if cur.is_null() {
            return None;
        }
    }
    Some(cur)
}

/// v4 `collectDatabaseStats` — the headline census.
pub fn collect_database_stats(db: &Db, user_id: &str) -> Result<EnhancedDatabaseStats, DbError> {
    let characters = count_rows(db, "characters", r#"WHERE "userId" = ?"#, &[&user_id]);
    let favourite_characters = count_rows(
        db,
        "characters",
        r#"WHERE "userId" = ? AND "isFavorite" = 1"#,
        &[&user_id],
    );
    let chats = count_rows(db, "chats", r#"WHERE "userId" = ?"#, &[&user_id]);
    let memories = count_rows(db, "memories", "", &[]);
    let tags = count_rows(db, "tags", r#"WHERE "userId" = ?"#, &[&user_id]);
    let projects = count_rows(db, "projects", "", &[]);
    let groups = count_rows(db, "groups", "", &[]);

    let profile_row = main_row(
        db,
        r#"SELECT COUNT(*)                                                     AS total,
              SUM(CASE WHEN "allowWebSearch" = 1 THEN 1 ELSE 0 END)        AS webSearchEnabled,
              SUM(CASE WHEN "allowToolUse" = 1 THEN 1 ELSE 0 END)          AS toolUseEnabled,
              SUM(CASE WHEN "isDangerousCompatible" = 1 THEN 1 ELSE 0 END) AS dangerousCompatible
       FROM "connection_profiles" WHERE "userId" = ?"#,
        &[&user_id],
        "ledgers.connectionProfiles",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?, num_col(r, 2)?, num_col(r, 3)?)),
    );

    let image_profiles = count_rows(db, "image_profiles", r#"WHERE "userId" = ?"#, &[&user_id]);
    let embedding_profiles_count =
        count_rows(db, "embedding_profiles", r#"WHERE "userId" = ?"#, &[&user_id]);

    // Templates come through the repositories in v4 — built-ins live alongside
    // user rows, so the filter is `isBuiltIn = 1 OR userId = ?`, not a bare
    // COUNT(*). See the module header on the seeding divergence.
    let prompt_templates = template_census(db, "prompt_templates", user_id, "ledgers.promptTemplates");
    let roleplay_templates = {
        let rows = db
            .read_main(|c| roleplay_templates::find_all_for_user(c, user_id))
            .unwrap_or_default();
        let built_in = rows
            .iter()
            .filter(|t| t.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false))
            .count() as f64;
        TemplateCensus {
            total: rows.len() as f64,
            built_in,
            custom: rows.len() as f64 - built_in,
        }
    };

    let (total, web, tool, dangerous) = profile_row.unwrap_or((0.0, 0.0, 0.0, 0.0));
    Ok(EnhancedDatabaseStats {
        characters,
        favorite_characters: favourite_characters,
        chats,
        memories,
        tags,
        projects,
        groups,
        connection_profiles: ConnectionProfileCensus {
            total,
            web_search_enabled: web,
            tool_use_enabled: tool,
            dangerous_compatible: dangerous,
        },
        image_profiles,
        embedding_profiles: embedding_profiles_count,
        prompt_templates,
        roleplay_templates,
    })
}

/// v4's `findAllForUser().filter(isBuiltIn)` census, expressed as SQL.
fn template_census(db: &Db, table: &str, user_id: &str, context: &str) -> TemplateCensus {
    let row = main_row(
        db,
        &format!(
            r#"SELECT COUNT(*) AS total, SUM(CASE WHEN "isBuiltIn" = 1 THEN 1 ELSE 0 END) AS builtIn
               FROM "{table}" WHERE "isBuiltIn" = 1 OR "userId" = ?"#
        ),
        &[&user_id],
        context,
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?)),
    );
    let (total, built_in) = row.unwrap_or((0.0, 0.0));
    TemplateCensus {
        total,
        built_in,
        custom: total - built_in,
    }
}

/// v4 `collectChatStats` — aggregate token/cost figures carried on the chat rows.
pub fn collect_chat_stats(db: &Db, user_id: &str) -> Result<ChatStatsInfo, DbError> {
    let row = main_row(
        db,
        r#"SELECT COALESCE(SUM("estimatedCostUSD"), 0)                     AS cost,
            COALESCE(SUM("totalPromptTokens"), 0)                    AS prompt,
            COALESCE(SUM("totalCompletionTokens"), 0)                AS completion,
            COALESCE(SUM("messageCount"), 0)                         AS messages,
            SUM(CASE WHEN "agentModeEnabled" = 1 THEN 1 ELSE 0 END)  AS agentMode,
            SUM(CASE WHEN "isDangerousChat" = 1 THEN 1 ELSE 0 END)   AS dangerous
     FROM "chats" WHERE "userId" = ?"#,
        &[&user_id],
        "ledgers.chatStats",
        |r| {
            Ok((
                num_col(r, 0)?,
                num_col(r, 1)?,
                num_col(r, 2)?,
                num_col(r, 3)?,
                num_col(r, 4)?,
                num_col(r, 5)?,
            ))
        },
    );
    let (cost, prompt, completion, messages, agent, dangerous) =
        row.unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
    Ok(ChatStatsInfo {
        total_estimated_cost_usd: cost,
        total_prompt_tokens: prompt,
        total_completion_tokens: completion,
        total_messages: messages,
        agent_mode_chats: agent,
        dangerous_chats: dangerous,
    })
}

/// v4 `collectChatBreakdown` — kinds, cast sizes, modes and pending work.
pub fn collect_chat_breakdown(db: &Db, user_id: &str) -> Result<ChatBreakdownInfo, DbError> {
    let by_type = main_rows(
        db,
        r#"SELECT COALESCE("chatType", 'salon') AS chatType, COUNT(*) AS count
       FROM "chats" WHERE "userId" = ?
       GROUP BY chatType ORDER BY count DESC"#,
        &[&user_id],
        "ledgers.chatsByType",
        |r| {
            Ok(ChatTypeRow {
                chat_type: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );

    let participant_histogram = main_rows(
        db,
        r#"SELECT json_array_length("participants") AS participants, COUNT(*) AS chats
       FROM "chats"
       WHERE "userId" = ? AND "participants" IS NOT NULL AND json_valid("participants")
       GROUP BY participants ORDER BY participants"#,
        &[&user_id],
        "ledgers.participantHistogram",
        |r| {
            Ok(ParticipantHistogramRow {
                participants: num_col(r, 0)?,
                chats: num_col(r, 1)?,
            })
        },
    );

    let flags = main_row(
        db,
        r#"SELECT SUM(CASE WHEN "isPaused" = 1 THEN 1 ELSE 0 END)                        AS paused,
              SUM(CASE WHEN "documentEditingMode" = 1 THEN 1 ELSE 0 END)             AS documentMode,
              SUM(CASE WHEN "equippedOutfit" IS NOT NULL
                        AND "equippedOutfit" NOT IN ('', '{}') THEN 1 ELSE 0 END)    AS equipped,
              SUM(CASE WHEN "pendingOutfitNotifications" IS NOT NULL
                        AND "pendingOutfitNotifications" NOT IN ('', '{}', '[]') THEN 1 ELSE 0 END)
                                                                                     AS pendingOutfits,
              SUM(CASE WHEN "timelineMode" = 'narrative' THEN 1 ELSE 0 END)          AS narrative,
              SUM(CASE WHEN "state" IS NOT NULL
                        AND "state" NOT IN ('', '{}') THEN 1 ELSE 0 END)             AS withState
       FROM "chats" WHERE "userId" = ?"#,
        &[&user_id],
        "ledgers.chatFlags",
        |r| {
            Ok((
                num_col(r, 0)?,
                num_col(r, 1)?,
                num_col(r, 2)?,
                num_col(r, 3)?,
                num_col(r, 4)?,
                num_col(r, 5)?,
            ))
        },
    );

    let chat_document_rows = count_rows(db, "chat_documents", "", &[]);
    let multi_doc = main_row(
        db,
        r#"SELECT COUNT(*) AS n FROM (
         SELECT "chatId" FROM "chat_documents" WHERE "isActive" = 1
         GROUP BY "chatId" HAVING COUNT(*) > 1
       )"#,
        &[],
        "ledgers.multiDocumentChats",
        |r| num_col(r, 0),
    );

    let (paused, document_mode, equipped, pending, narrative, with_state) =
        flags.unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
    Ok(ChatBreakdownInfo {
        by_type,
        participant_histogram,
        paused_chats: paused,
        document_mode_chats: document_mode,
        chat_document_rows,
        multi_document_chats: multi_doc.unwrap_or(0.0),
        chats_with_equipped_outfit: equipped,
        pending_outfit_notification_chats: pending,
        narrative_timeline_chats: narrative,
        chats_with_non_empty_state: with_state,
    })
}

/// v4 `collectAutonomousRooms` — lifecycle, schedule, budgets and visibility.
/// `now_iso` is injected (v4 `new Date().toISOString()`) so the differential can
/// pin the overdue boundary.
pub fn collect_autonomous_rooms(
    db: &Db,
    user_id: &str,
    now_iso: &str,
) -> Result<AutonomousRoomInfo, DbError> {
    let total = count_rows(
        db,
        "chats",
        r#"WHERE "userId" = ? AND "chatType" = 'autonomous'"#,
        &[&user_id],
    );

    let by_run_state = main_rows(
        db,
        r#"SELECT COALESCE("runState", 'idle') AS runState, COUNT(*) AS count
       FROM "chats" WHERE "userId" = ? AND "chatType" = 'autonomous'
       GROUP BY runState ORDER BY count DESC"#,
        &[&user_id],
        "ledgers.autonomousRunState",
        |r| {
            Ok(RunStateRow {
                run_state: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );

    let by_visibility = main_rows(
        db,
        r#"SELECT COALESCE("runVisibility", 'inherit') AS visibility, COUNT(*) AS count
       FROM "chats" WHERE "userId" = ? AND "chatType" = 'autonomous'
       GROUP BY visibility ORDER BY count DESC"#,
        &[&user_id],
        "ledgers.autonomousVisibility",
        |r| {
            Ok(VisibilityRow {
                visibility: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );

    let counts = main_row(
        db,
        r#"SELECT SUM(CASE WHEN "scheduleCron" IS NOT NULL THEN 1 ELSE 0 END)               AS scheduled,
              SUM(CASE WHEN "scheduleNextRunAt" IS NOT NULL
                        AND "scheduleNextRunAt" < ? THEN 1 ELSE 0 END)                  AS overdue,
              SUM(CASE WHEN "budgetMaxTurns" IS NOT NULL THEN 1 ELSE 0 END)             AS turns,
              SUM(CASE WHEN "budgetMaxTokens" IS NOT NULL THEN 1 ELSE 0 END)            AS tokens,
              SUM(CASE WHEN "budgetMaxWallClockMs" IS NOT NULL THEN 1 ELSE 0 END)       AS wallClock,
              SUM(CASE WHEN "budgetEstimatedSpendCapUSD" IS NOT NULL THEN 1 ELSE 0 END) AS spend,
              SUM(CASE WHEN "runDestructiveToolsAllowed" = 1 THEN 1 ELSE 0 END)         AS destructive
       FROM "chats" WHERE "userId" = ? AND "chatType" = 'autonomous'"#,
        &[&now_iso, &user_id],
        "ledgers.autonomousBudgets",
        |r| {
            Ok((
                num_col(r, 0)?,
                num_col(r, 1)?,
                num_col(r, 2)?,
                num_col(r, 3)?,
                num_col(r, 4)?,
                num_col(r, 5)?,
                num_col(r, 6)?,
            ))
        },
    );
    let (scheduled, overdue, turns, tokens, wall, spend, destructive) =
        counts.unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));

    Ok(AutonomousRoomInfo {
        total,
        by_run_state,
        scheduled,
        overdue,
        with_turn_budget: turns,
        with_token_budget: tokens,
        with_wall_clock_budget: wall,
        with_spend_cap: spend,
        destructive_tools_allowed: destructive,
        by_visibility,
    })
}

/// v4 `collectMemoryBreakdown` — the Commonplace Book in the episodic era.
///
/// Also returns the per-character memory counts, which Phase 5's top-ten table
/// needs — one `GROUP BY` instead of one query per character.
pub fn collect_memory_breakdown(
    db: &Db,
) -> Result<(MemoryBreakdownInfo, HashMap<String, f64>), DbError> {
    let totals = main_row(
        db,
        r#"SELECT COUNT(*)                                                            AS total,
              SUM(CASE WHEN "occurredAt" IS NOT NULL THEN 1 ELSE 0 END)           AS occurredAt,
              SUM(CASE WHEN "narrativeTime" IS NOT NULL
                        AND "narrativeTime" != '' THEN 1 ELSE 0 END)              AS narrativeTime,
              SUM(CASE WHEN "entities" IS NOT NULL
                        AND "entities" NOT IN ('', '[]') THEN 1 ELSE 0 END)       AS entities,
              SUM(CASE WHEN "embedding" IS NOT NULL THEN 1 ELSE 0 END)            AS embedded,
              COALESCE(SUM("reinforcementCount"), 0)                              AS reinforced,
              COALESCE(MAX("reinforcementCount"), 0)                              AS maxReinforcement
       FROM "memories""#,
        &[],
        "ledgers.memoryTotals",
        |r| {
            Ok((
                num_col(r, 0)?,
                num_col(r, 1)?,
                num_col(r, 2)?,
                num_col(r, 3)?,
                num_col(r, 4)?,
                num_col(r, 5)?,
                num_col(r, 6)?,
            ))
        },
    );

    let by_kind = main_rows(
        db,
        r#"SELECT COALESCE("kind", 'semantic') AS kind, COUNT(*) AS count
       FROM "memories" GROUP BY kind ORDER BY count DESC"#,
        &[],
        "ledgers.memoriesByKind",
        |r| {
            Ok(MemoryKindRow {
                kind: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );
    let by_source = main_rows(
        db,
        r#"SELECT COALESCE("source", 'MANUAL') AS source, COUNT(*) AS count
       FROM "memories" GROUP BY source ORDER BY count DESC"#,
        &[],
        "ledgers.memoriesBySource",
        |r| {
            Ok(MemorySourceRow {
                source: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );
    let by_witnessed = main_rows(
        db,
        r#"SELECT COALESCE("witnessedContext", 'unrecorded') AS witnessedContext, COUNT(*) AS count
       FROM "memories" GROUP BY witnessedContext ORDER BY count DESC"#,
        &[],
        "ledgers.memoriesByWitnessedContext",
        |r| {
            Ok(MemoryWitnessedRow {
                witnessed_context: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );
    let per_character: Vec<(String, f64)> = main_rows(
        db,
        r#"SELECT "characterId" AS characterId, COUNT(*) AS count
       FROM "memories" GROUP BY "characterId""#,
        &[],
        "ledgers.memoriesByCharacter",
        |r| Ok((text(r, 0)?, num_col(r, 1)?)),
    );

    let mut by_character: HashMap<String, f64> = HashMap::new();
    for (id, count) in per_character {
        by_character.insert(id, count);
    }

    let (total, occurred, narrative, entities, embedded, reinforced, max_reinforcement) =
        totals.unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));

    Ok((
        MemoryBreakdownInfo {
            total,
            by_kind,
            by_source,
            by_witnessed_context: by_witnessed,
            with_occurred_at: occurred,
            with_narrative_time: narrative,
            with_entities: entities,
            with_embedding: embedded,
            reinforced_total: reinforced,
            max_reinforcement_count: max_reinforcement,
            characters_with_memories: by_character.len() as f64,
        },
        by_character,
    ))
}

/// v4 `collectCharacterBreakdown` — who is in the cast, and what they may do.
pub fn collect_character_breakdown(
    db: &Db,
    user_id: &str,
) -> Result<CharacterBreakdownInfo, DbError> {
    let row = main_row(
        db,
        r#"SELECT COUNT(*)                                                                     AS total,
            SUM(CASE WHEN "characterDocumentMountPointId" IS NOT NULL THEN 1 ELSE 0 END) AS vaultLinked,
            SUM(CASE WHEN "npc" = 1 THEN 1 ELSE 0 END)                                   AS npcs,
            SUM(CASE WHEN "controlledBy" = 'user' THEN 1 ELSE 0 END)                     AS userControlled,
            SUM(CASE WHEN "canBeCarina" = 1 THEN 1 ELSE 0 END)                           AS carina,
            SUM(CASE WHEN "systemTransparency" = 1 THEN 1 ELSE 0 END)                    AS transparent,
            SUM(CASE WHEN "canDressThemselves" = 1 THEN 1 ELSE 0 END)                    AS dress,
            SUM(CASE WHEN "canCreateOutfits" = 1 THEN 1 ELSE 0 END)                      AS outfits,
            SUM(CASE WHEN "coreWhisperEnabled" IS NOT NULL THEN 1 ELSE 0 END)            AS coreWhisper
     FROM "characters" WHERE "userId" = ?"#,
        &[&user_id],
        "ledgers.characterBreakdown",
        |r| {
            Ok((
                num_col(r, 0)?,
                num_col(r, 1)?,
                num_col(r, 2)?,
                num_col(r, 3)?,
                num_col(r, 4)?,
                num_col(r, 5)?,
                num_col(r, 6)?,
                num_col(r, 7)?,
                num_col(r, 8)?,
            ))
        },
    );
    let (total, vault_linked, npcs, user_controlled, carina, transparent, dress, outfits, whisper) =
        row.unwrap_or((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));

    Ok(CharacterBreakdownInfo {
        total,
        vault_linked,
        vaultless: (total - vault_linked).max(0.0),
        npcs,
        user_controlled,
        carina_answerers: carina,
        system_transparent: transparent,
        can_dress_themselves: dress,
        can_create_outfits: outfits,
        core_whisper_overrides: whisper,
    })
}

/// v4 `defaultFeatureConfig()` — the dials as they read with no settings row at
/// all. Written out in full rather than left as a `null` fallback: if the
/// collector fails, the report should say "everything at its default", which is
/// what an unconfigured instance genuinely looks like.
pub fn default_feature_config() -> FeatureConfigInfo {
    FeatureConfigInfo {
        dangerous_content: DangerousContentConfig {
            mode: "OFF".into(),
            threshold: 0.7,
            scan_text_chat: true,
            scan_image_prompts: true,
            scan_image_generation: false,
        },
        context_compression: ContextCompressionConfig {
            enabled: true,
            window_size: 5.0,
            compression_target_tokens: 800.0,
        },
        agent_mode: AgentModeConfig {
            max_turns: 10.0,
            default_enabled: false,
        },
        story_backgrounds: StoryBackgroundsConfig {
            enabled: false,
            has_default_image_profile: false,
        },
        timestamps: TimestampsConfig {
            mode: "NONE".into(),
            format: "FRIENDLY".into(),
        },
        auto_lock: AutoLockConfig {
            enabled: false,
            idle_minutes: 15.0,
        },
        memory_cascade: MemoryCascadeConfig {
            on_message_delete: "ASK_EVERY_TIME".into(),
            on_swipe_regenerate: "DELETE_MEMORIES".into(),
        },
        auto_detect_rng: true,
        custom_tools: true,
        avatar_display: AvatarDisplayConfig {
            mode: "ALWAYS".into(),
            style: "CIRCULAR".into(),
        },
        core_whisper: CoreWhisperConfig {
            enabled: true,
            interval: 12.0,
            silence_threshold: 3.0,
            packet_token_budget: 4096.0,
            fire_on_context_transition: true,
        },
        thinking_display: ThinkingDisplayConfig {
            default_visible: true,
            default_collapsed: true,
        },
        answer_confirmation: AnswerConfirmationConfig { enabled: false },
        autonomous_room_defaults: Value::Object(Default::default()),
        auto_housekeeping: AutoHousekeepingConfig {
            enabled: false,
            per_character_cap: 2000.0,
            merge_similar: false,
            auto_merge_similar_threshold: 0.9,
        },
        text_replacements: TextReplacementsConfig {
            enabled: true,
            rules: 0.0,
            enabled_rules: 0.0,
        },
        composer_spellcheck: true,
        auto_scroll_on_response_complete: false,
        image_description_profile_configured: false,
        uncensored_image_description_profile_configured: false,
    }
}

/// v4 `collectFeatureConfig` — every dial on `chat_settings` with its effective
/// value. The `??` chain is faithful: an absent key AND an explicit `null` both
/// take the default.
pub fn collect_feature_config(db: &Db, user_id: &str) -> Result<FeatureConfigInfo, DbError> {
    let settings = db
        .read_main(|c| crate::db::chat_settings::find_by_user_id(c, user_id))?
        .unwrap_or(Value::Null);
    let s = &settings;

    let rule_row = main_row(
        db,
        r#"SELECT COUNT(*) AS total, SUM(CASE WHEN "enabled" = 1 THEN 1 ELSE 0 END) AS enabled
       FROM "text_replacement_rules""#,
        &[],
        "ledgers.textReplacementRules",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?)),
    );
    let (rules, enabled_rules) = rule_row.unwrap_or((0.0, 0.0));

    Ok(FeatureConfigInfo {
        dangerous_content: DangerousContentConfig {
            mode: jstr(s, &["dangerousContentSettings", "mode"], "OFF"),
            threshold: jnum(s, &["dangerousContentSettings", "threshold"], 0.7),
            scan_text_chat: jbool(s, &["dangerousContentSettings", "scanTextChat"], true),
            scan_image_prompts: jbool(s, &["dangerousContentSettings", "scanImagePrompts"], true),
            scan_image_generation: jbool(
                s,
                &["dangerousContentSettings", "scanImageGeneration"],
                false,
            ),
        },
        context_compression: ContextCompressionConfig {
            enabled: jbool(s, &["contextCompressionSettings", "enabled"], true),
            window_size: jnum(s, &["contextCompressionSettings", "windowSize"], 5.0),
            compression_target_tokens: jnum(
                s,
                &["contextCompressionSettings", "compressionTargetTokens"],
                800.0,
            ),
        },
        agent_mode: AgentModeConfig {
            max_turns: jnum(s, &["agentModeSettings", "maxTurns"], 10.0),
            default_enabled: jbool(s, &["agentModeSettings", "defaultEnabled"], false),
        },
        story_backgrounds: StoryBackgroundsConfig {
            enabled: jbool(s, &["storyBackgroundsSettings", "enabled"], false),
            has_default_image_profile: jtruthy(
                s,
                &["storyBackgroundsSettings", "defaultImageProfileId"],
            ),
        },
        timestamps: TimestampsConfig {
            mode: jstr(s, &["defaultTimestampConfig", "mode"], "NONE"),
            format: jstr(s, &["defaultTimestampConfig", "format"], "FRIENDLY"),
        },
        auto_lock: AutoLockConfig {
            enabled: jbool(s, &["autoLockSettings", "enabled"], false),
            idle_minutes: jnum(s, &["autoLockSettings", "idleMinutes"], 15.0),
        },
        memory_cascade: MemoryCascadeConfig {
            on_message_delete: jstr(
                s,
                &["memoryCascadePreferences", "onMessageDelete"],
                "ASK_EVERY_TIME",
            ),
            on_swipe_regenerate: jstr(
                s,
                &["memoryCascadePreferences", "onSwipeRegenerate"],
                "DELETE_MEMORIES",
            ),
        },
        auto_detect_rng: jbool(s, &["autoDetectRng"], true),
        custom_tools: jbool(s, &["customTools"], true),
        avatar_display: AvatarDisplayConfig {
            mode: jstr(s, &["avatarDisplayMode"], "ALWAYS"),
            style: jstr(s, &["avatarDisplayStyle"], "CIRCULAR"),
        },
        core_whisper: CoreWhisperConfig {
            enabled: jbool(s, &["coreWhisper", "enabled"], true),
            interval: jnum(s, &["coreWhisper", "interval"], 12.0),
            silence_threshold: jnum(s, &["coreWhisper", "silenceThreshold"], 3.0),
            packet_token_budget: jnum(s, &["coreWhisper", "packetTokenBudget"], 4096.0),
            fire_on_context_transition: jbool(
                s,
                &["coreWhisper", "fireOnContextTransition"],
                true,
            ),
        },
        thinking_display: ThinkingDisplayConfig {
            default_visible: jbool(s, &["thinkingDisplay", "defaultVisible"], true),
            default_collapsed: jbool(s, &["thinkingDisplay", "defaultCollapsed"], true),
        },
        answer_confirmation: AnswerConfirmationConfig {
            enabled: jbool(s, &["answerConfirmationSettings", "enabled"], false),
        },
        autonomous_room_defaults: jget(s, &["autonomousRoomSettings"])
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default())),
        auto_housekeeping: AutoHousekeepingConfig {
            enabled: jbool(s, &["autoHousekeepingSettings", "enabled"], false),
            per_character_cap: jnum(s, &["autoHousekeepingSettings", "perCharacterCap"], 2000.0),
            merge_similar: jbool(s, &["autoHousekeepingSettings", "mergeSimilar"], false),
            auto_merge_similar_threshold: jnum(
                s,
                &["autoHousekeepingSettings", "autoMergeSimilarThreshold"],
                0.9,
            ),
        },
        text_replacements: TextReplacementsConfig {
            enabled: jbool(s, &["textReplacementsEnabled"], true),
            rules,
            enabled_rules,
        },
        composer_spellcheck: jbool(s, &["composerSpellcheck"], true),
        auto_scroll_on_response_complete: jbool(s, &["autoScrollOnResponseComplete"], false),
        image_description_profile_configured: jtruthy(s, &["imageDescriptionProfileId"]),
        uncensored_image_description_profile_configured: jtruthy(
            s,
            &["uncensoredImageDescriptionProfileId"],
        ),
    })
}

/// v4 `collectInstanceSettings` — instance-wide knobs plus the derived "how many
/// chats does the next sweep actually touch" figure.
///
/// The eligibility count goes through the same `resolveStaleChatDays` +
/// `isStale` pair the maintenance sweep uses, so the number the report prints
/// and the number the sweep acts on can never disagree.
pub fn collect_instance_settings(
    db: &Db,
    user_id: &str,
    now_ms: i64,
) -> Result<InstanceSettingsInfo, DbError> {
    let stale_chat_days = db.read_main(|c| Ok(resolve_stale_chat_days_conn(c)))?;
    let max_concurrent_jobs = db.read_main(instance_settings::get_max_concurrent_jobs)?;
    let (scope_policy, expand_related) =
        db.read_main(instance_settings::get_memory_recall_settings)?;
    let extraction = db.read_main(instance_settings::get_memory_extraction_limits)?;
    let last_sweep = db.read_main(instance_settings::get_last_maintenance_sweep_at)?;
    // v4 also awaits `getDataRetentionSettings()` and immediately voids it —
    // `resolveStaleChatDays` already folds it in. Not re-read here.

    let cutoff_iso = retention_cutoff_iso(stale_chat_days, now_ms);
    let cutoff_ms = crate::clock::iso_to_ms(&cutoff_iso).unwrap_or(0);
    let candidates: Vec<(String, Option<String>)> = main_rows(
        db,
        r#"SELECT "id", "updatedAt" FROM "chats"
       WHERE "userId" = ? AND "updatedAt" < ?"#,
        &[&user_id, &cutoff_iso],
        "ledgers.staleChatCandidates",
        |r| Ok((text(r, 0)?, opt_text(r, 1)?)),
    );
    let mut eligible = 0.0;
    let counted = db.read_main(|conn| {
        let mut n = 0.0;
        for (id, updated_at) in &candidates {
            let chat = serde_json::json!({ "id": id, "updatedAt": updated_at });
            if is_stale_conn(conn, &chat, cutoff_ms)? {
                n += 1.0;
            }
        }
        Ok(n)
    });
    match counted {
        Ok(n) => eligible = n,
        Err(e) => tracing::warn!(error = %e, "Failed to compute stale-chat eligibility"),
    }

    Ok(InstanceSettingsInfo {
        stale_chat_days: stale_chat_days as f64,
        chats_eligible_for_next_sweep: eligible,
        max_concurrent_jobs: max_concurrent_jobs as f64,
        memory_recall: MemoryRecallConfig {
            scope_policy,
            expand_related,
        },
        memory_extraction_limits: MemoryExtractionLimitsConfig {
            enabled: jbool(&extraction, &["enabled"], false),
            max_per_hour: jnum(&extraction, &["maxPerHour"], 20.0),
            soft_start_fraction: jnum(&extraction, &["softStartFraction"], 0.7),
            soft_floor: jnum(&extraction, &["softFloor"], 0.7),
        },
        last_maintenance_sweep_at: last_sweep.map(crate::clock::iso_from_unix_ms),
    })
}

/// v4 `collectBackgroundJobs` — what is waiting, what is stuck, what gave up.
pub fn collect_background_jobs(db: &Db, user_id: &str) -> Result<BackgroundJobInfo, DbError> {
    let by_status = main_rows(
        db,
        r#"SELECT COALESCE("status", 'PENDING') AS status, COUNT(*) AS count
       FROM "background_jobs" WHERE "userId" = ?
       GROUP BY status ORDER BY count DESC"#,
        &[&user_id],
        "ledgers.jobsByStatus",
        |r| {
            Ok(JobStatusRow {
                status: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );
    let by_type = main_rows(
        db,
        r#"SELECT "type" AS type, COUNT(*) AS count
       FROM "background_jobs" WHERE "userId" = ?
       GROUP BY type ORDER BY count DESC"#,
        &[&user_id],
        "ledgers.jobsByType",
        |r| {
            Ok(JobTypeRow {
                job_type: text(r, 0)?,
                count: num_col(r, 1)?,
            })
        },
    );
    let failed = main_rows(
        db,
        r#"SELECT "type" AS type, COUNT(*) AS count, MAX("lastError") AS lastError
       FROM "background_jobs"
       WHERE "userId" = ? AND "status" = 'FAILED'
       GROUP BY type ORDER BY count DESC"#,
        &[&user_id],
        "ledgers.failedJobs",
        |r| {
            Ok(FailedJobRow {
                job_type: text(r, 0)?,
                count: num_col(r, 1)?,
                // Truncated: an error string can be a whole stack trace, and the
                // report is meant to be shareable. v4's `slice(0, 200)` is over
                // UTF-16 code units.
                last_error: opt_text(r, 2)?.map(|e| crate::jsstr::utf16_truncate(&e, 200)),
            })
        },
    );
    let exhausted = main_row(
        db,
        r#"SELECT COUNT(*) AS n FROM "background_jobs"
       WHERE "userId" = ? AND "attempts" >= "maxAttempts""#,
        &[&user_id],
        "ledgers.exhaustedJobs",
        |r| num_col(r, 0),
    );
    let oldest_pending = main_row(
        db,
        r#"SELECT MIN("scheduledAt") AS scheduledAt FROM "background_jobs"
       WHERE "userId" = ? AND "status" = 'PENDING'"#,
        &[&user_id],
        "ledgers.oldestPendingJob",
        |r| opt_text(r, 0),
    )
    .flatten();

    Ok(BackgroundJobInfo {
        by_status,
        by_type,
        failed,
        attempts_exhausted: exhausted.unwrap_or(0.0),
        oldest_pending_scheduled_at: oldest_pending,
    })
}

/// Distinct stored vector widths in a table, derived from the BLOB header
/// (v4 `embeddingWidths`).
///
/// Quantized blobs (4.8+) begin with magic `0xEB`; the dtype byte then decides
/// the layout — int8 is an 11-byte header plus one byte per dimension, float16
/// a 7-byte header plus two. Anything without the magic is a legacy raw float32
/// buffer, four bytes per dimension. Branching on the prefix is mandatory: read
/// every blob as float32 and each width comes out a quarter of its real value.
fn embedding_widths(db: &Db, table: &str) -> Vec<StoredDimensionRow> {
    main_rows(
        db,
        &format!(
            r#"SELECT CASE
              WHEN hex(substr("embedding", 1, 1)) != 'EB'
                THEN length("embedding") / 4
              WHEN hex(substr("embedding", 3, 1)) = '02'
                THEN (length("embedding") - 7) / 2
              ELSE length("embedding") - 11
            END AS dimensions,
            COUNT(*) AS vectors
     FROM "{table}"
     WHERE "embedding" IS NOT NULL
     GROUP BY dimensions
     ORDER BY vectors DESC"#
        ),
        &[],
        &format!("ledgers.embeddingWidths:{table}"),
        |r| {
            Ok(StoredDimensionRow {
                table: table.to_string(),
                dimensions: num_col(r, 0)?,
                vectors: num_col(r, 1)?,
            })
        },
    )
}

/// v4 `collectEmbeddingPipeline` — what is indexed, what failed, and whether the
/// vectors on disk match the profile that would be used to query them.
pub fn collect_embedding_pipeline(db: &Db, user_id: &str) -> Result<EmbeddingPipelineInfo, DbError> {
    let status_rows = main_rows(
        db,
        r#"SELECT "entityType" AS entityType, COALESCE("status", 'PENDING') AS status, COUNT(*) AS count
       FROM "embedding_status" WHERE "userId" = ?
       GROUP BY entityType, status ORDER BY entityType, count DESC"#,
        &[&user_id],
        "ledgers.embeddingStatus",
        |r| {
            Ok(EmbeddingStatusRow {
                entity_type: text(r, 0)?,
                status: text(r, 1)?,
                count: num_col(r, 2)?,
            })
        },
    );

    let chunk_row = main_row(
        db,
        r#"SELECT COUNT(*) AS total, SUM(CASE WHEN "embedding" IS NULL THEN 1 ELSE 0 END) AS unembedded
       FROM "conversation_chunks""#,
        &[],
        "ledgers.conversationChunks",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?)),
    );
    let help_row = main_row(
        db,
        r#"SELECT COUNT(*) AS total, SUM(CASE WHEN "embedding" IS NULL THEN 1 ELSE 0 END) AS unembedded
       FROM "help_docs""#,
        &[],
        "ledgers.helpDocs",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?)),
    );

    let mut stored_dimensions = embedding_widths(db, "memories");
    stored_dimensions.extend(embedding_widths(db, "conversation_chunks"));

    let active_profile = db
        .read_main(|c| embedding_profiles::find_default(c, user_id))
        .ok()
        .flatten();
    // v4: `truncateToDimensions ?? dimensions ?? null`.
    let active_dims = active_profile
        .as_ref()
        .and_then(|p| p.truncate_to_dimensions.or(p.dimensions));

    let dimension_mismatch = active_dims
        .is_some_and(|d| stored_dimensions.iter().any(|row| row.dimensions != d));

    let permanently_failed: f64 = status_rows
        .iter()
        .filter(|r| r.status == "PERMANENTLY_FAILED")
        .map(|r| r.count)
        .sum();

    let (chunk_total, chunk_unembedded) = chunk_row.unwrap_or((0.0, 0.0));
    let (help_total, help_unembedded) = help_row.unwrap_or((0.0, 0.0));

    Ok(EmbeddingPipelineInfo {
        status_by_entity_type: status_rows,
        permanently_failed,
        conversation_chunks: EmbeddedTableCensus {
            total: chunk_total,
            unembedded: chunk_unembedded,
        },
        help_docs: EmbeddedTableCensus {
            total: help_total,
            unembedded: help_unembedded,
        },
        stored_dimensions,
        active_profile_dimensions: active_dims,
        dimension_mismatch,
    })
}

/// v4 `collectTerminal` — Ariel's sessions.
pub fn collect_terminal(db: &Db) -> Result<TerminalInfo, DbError> {
    let row = main_row(
        db,
        r#"SELECT COUNT(*)                                                            AS total,
              SUM(CASE WHEN "exitedAt" IS NULL THEN 1 ELSE 0 END)                 AS live,
              SUM(CASE WHEN "exitCode" IS NOT NULL AND "exitCode" != 0 THEN 1 ELSE 0 END) AS nonZero
       FROM "terminal_sessions""#,
        &[],
        "ledgers.terminalSessions",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?, num_col(r, 2)?)),
    );
    let shells: Vec<Option<String>> = main_rows(
        db,
        r#"SELECT DISTINCT "shell" AS shell FROM "terminal_sessions" ORDER BY shell"#,
        &[],
        "ledgers.terminalShells",
        |r| opt_text(r, 0),
    );
    let (total, live, non_zero) = row.unwrap_or((0.0, 0.0, 0.0));
    Ok(TerminalInfo {
        total_sessions: total,
        live_sessions: live,
        non_zero_exits: non_zero,
        // v4's `.filter(Boolean)` drops both NULL and the empty string.
        distinct_shells: shells
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect(),
    })
}

/// v4 `collectStorageStats` — the legacy `files` ledger.
pub fn collect_storage_stats(db: &Db, user_id: &str) -> Result<StorageStats, DbError> {
    let folder_rows = main_rows(
        db,
        r#"SELECT COALESCE("folderPath", '/') AS folderPath,
              COUNT(*)                    AS fileCount,
              COALESCE(SUM("size"), 0)    AS totalSize
       FROM "files" WHERE "userId" = ?
       GROUP BY folderPath ORDER BY totalSize DESC"#,
        &[&user_id],
        "ledgers.storageFolders",
        |r| {
            Ok(FolderStats {
                // v4's `r.folderPath ?? '/'` on top of the SQL COALESCE.
                path: opt_text(r, 0)?.unwrap_or_else(|| "/".to_string()),
                file_count: num_col(r, 1)?,
                total_size: num_col(r, 2)?,
            })
        },
    );

    let totals = main_row(
        db,
        r#"SELECT COUNT(*)                                                          AS totalFiles,
              COALESCE(SUM("size"), 0)                                          AS totalSize,
              SUM(CASE WHEN COALESCE("fileStatus", 'ok') != 'ok' THEN 1 ELSE 0 END) AS notOk
       FROM "files" WHERE "userId" = ?"#,
        &[&user_id],
        "ledgers.storageTotals",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?, num_col(r, 2)?)),
    );

    let generated = main_rows(
        db,
        r#"SELECT "generationModel" AS generationModel, COUNT(*) AS count, COALESCE(SUM("size"), 0) AS bytes
       FROM "files"
       WHERE "userId" = ? AND "generationModel" IS NOT NULL AND "generationModel" != ''
       GROUP BY generationModel ORDER BY count DESC"#,
        &[&user_id],
        "ledgers.generatedImages",
        |r| {
            Ok(GeneratedImageRow {
                generation_model: text(r, 0)?,
                count: num_col(r, 1)?,
                bytes: num_col(r, 2)?,
            })
        },
    );

    let (total_files, total_size, not_ok) = totals.unwrap_or((0.0, 0.0, 0.0));
    Ok(StorageStats {
        total_files,
        total_size,
        folders: folder_rows,
        not_ok_files: not_ok,
        generated_images_by_model: generated,
    })
}

/// Bind an owned string list as `&dyn ToSql` params — the `IN (…)` helper every
/// mount-side collector needs (kept here so phases 4 and 5 share one shape).
pub fn sql_params(values: &[String]) -> Vec<SqlValue> {
    values.iter().map(|v| SqlValue::Text(v.clone())).collect()
}
