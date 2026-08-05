//! The Almanack — Phase 6, "Reading the wire records"
//! (v4 `lib/tools/almanack/phase6-wire-records.ts` + the aggregate half of
//! `lib/database/repositories/llm-logs.repository.ts`).
//!
//! The llm-logs database: what was asked of which model, how long it took, what
//! it cost in tokens, and how much of the prompt the provider served from cache.
//!
//! Two honesty constraints shape this whole phase, carried from v4:
//!
//!  1. **Attribution.** Before 4.9 no row recorded which profile served it, so
//!     older rows can only be grouped by `(provider, modelName)` — which cannot
//!     separate two profiles sharing a pair. The section says "approximate" when
//!     that fallback is in use rather than presenting a guess as a fact.
//!  2. **Denominators.** `durationMs` is null on a great many historical rows,
//!     so every latency figure carries the count it was averaged over.
//!
//! ## §2 of the work order — where the aggregates live
//!
//! The read-side roll-ups and the attribution probe live HERE, as raw SQL over
//! the llm-logs connection, mirroring v4's fail-soft `aggregate()` helper. They
//! are deliberately NOT added to `db/llm_logs.rs`, which the sibling drift lane
//! (P4.D49) owns. The probe is what makes the two lanes runtime-independent:
//! this phase takes the approximate arm on any database without the columns —
//! including every v5-provisioned instance until that lane's schema re-dump
//! lands — and the exact arm on any database with them.

use std::collections::HashMap;

use rusqlite::types::Value as SqlValue;
use rusqlite::ToSql;

use crate::db::runtime::Db;
use crate::db::DbError;

use super::db::{logs_rows, main_rows, num_col, opt_text, text};
use super::types::*;

/// How many profiles each windowed table lists (v4 `PROFILE_ROW_LIMIT`).
const PROFILE_ROW_LIMIT: usize = 20;

fn ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator > 0.0 {
        numerator / denominator
    } else {
        0.0
    }
}

/// The attribution key a windowed roll-up groups by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    ConnectionProfileId,
    ImageProfileId,
    /// The approximate fallback older rows must use.
    ProviderModel,
}

impl GroupBy {
    fn key_expr(self) -> &'static str {
        match self {
            GroupBy::ProviderModel => r#""provider" || '/' || "modelName""#,
            GroupBy::ConnectionProfileId => r#""connectionProfileId""#,
            GroupBy::ImageProfileId => r#""imageProfileId""#,
        }
    }
    fn column(self) -> Option<&'static str> {
        match self {
            GroupBy::ProviderModel => None,
            GroupBy::ConnectionProfileId => Some("connectionProfileId"),
            GroupBy::ImageProfileId => Some("imageProfileId"),
        }
    }
    fn context(self) -> &'static str {
        match self {
            GroupBy::ProviderModel => "providerModel",
            GroupBy::ConnectionProfileId => "connectionProfileId",
            GroupBy::ImageProfileId => "imageProfileId",
        }
    }
}

/// v4 `hasProfileAttributionColumns()` — does the table carry the 4.9
/// profile-attribution columns yet? A `PRAGMA table_info` through the same
/// fail-soft door, so an unreachable llm-logs partition reads as "no columns"
/// and the phase takes the approximate arm (v4's identical fallback).
pub fn has_profile_attribution_columns(db: &Db) -> bool {
    let names: Vec<String> = logs_rows(
        db,
        r#"PRAGMA table_info("llm_logs")"#,
        &[],
        "llm-logs.hasProfileAttributionColumns",
        |r| text(r, 1),
    );
    names.iter().any(|n| n == "connectionProfileId")
}

/// One `getStatsByProfile` row.
struct ProfileStatsRow {
    key: String,
    provider: String,
    model_name: String,
    requests: f64,
    prompt_tokens: f64,
    completion_tokens: f64,
    total_tokens: f64,
    measured_requests: f64,
    avg_duration_ms: Option<f64>,
    failures: f64,
}

/// v4 `LLMLogsRepository.getStatsByType` — per-`type` counts, token totals and
/// measured-latency averages.
fn get_stats_by_type(db: &Db, user_id: &str) -> Vec<WireTypeRow> {
    logs_rows(
        db,
        r#"SELECT
         "type"                                                        AS type,
         COUNT(*)                                                      AS requests,
         COALESCE(SUM(json_extract("usage", '$.promptTokens')), 0)     AS promptTokens,
         COALESCE(SUM(json_extract("usage", '$.completionTokens')), 0) AS completionTokens,
         COALESCE(SUM(json_extract("usage", '$.totalTokens')), 0)      AS totalTokens,
         SUM(CASE WHEN "durationMs" IS NOT NULL AND "durationMs" > 0 THEN 1 ELSE 0 END) AS measuredRequests,
         AVG(CASE WHEN "durationMs" IS NOT NULL AND "durationMs" > 0 THEN "durationMs" END) AS avgDurationMs,
         SUM(CASE WHEN json_extract("response", '$.error') IS NOT NULL THEN 1 ELSE 0 END) AS failures
       FROM "llm_logs"
       WHERE "userId" = ?
       GROUP BY "type"
       ORDER BY requests DESC"#,
        &[&user_id],
        "llm-logs.getStatsByType",
        |r| {
            Ok(WireTypeRow {
                log_type: text(r, 0)?,
                requests: num_col(r, 1)?,
                prompt_tokens: num_col(r, 2)?,
                completion_tokens: num_col(r, 3)?,
                total_tokens: num_col(r, 4)?,
                measured_requests: num_col(r, 5)?,
                // v4 keeps a SQL NULL average as `null`, distinct from 0 — the
                // renderer prints `—` for it.
                avg_duration_ms: opt_num(r, 6)?,
                failures: num_col(r, 7)?,
            })
        },
    )
}

/// A column that is `null` in SQL stays `None` (v4's `row.avgDurationMs === null
/// ? null : num(...)`), otherwise coerced through `num()`.
fn opt_num(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<Option<f64>> {
    Ok(match row.get_ref(idx)? {
        rusqlite::types::ValueRef::Null => None,
        other => Some(super::db::num(other)),
    })
}

/// v4 `LLMLogsRepository.getStatsByProfile` — the per-profile roll-up.
fn get_stats_by_profile(
    db: &Db,
    user_id: &str,
    group_by: GroupBy,
    log_type: Option<&str>,
) -> Vec<ProfileStatsRow> {
    let type_clause = if log_type.is_some() {
        r#"AND "type" = ?"#
    } else {
        ""
    };
    let not_null_clause = match group_by.column() {
        Some(col) => format!(r#"AND "{col}" IS NOT NULL"#),
        None => String::new(),
    };
    let sql = format!(
        r#"SELECT
         {}                                                            AS key,
         "provider"                                                    AS provider,
         "modelName"                                                   AS modelName,
         COUNT(*)                                                      AS requests,
         COALESCE(SUM(json_extract("usage", '$.promptTokens')), 0)     AS promptTokens,
         COALESCE(SUM(json_extract("usage", '$.completionTokens')), 0) AS completionTokens,
         COALESCE(SUM(json_extract("usage", '$.totalTokens')), 0)      AS totalTokens,
         SUM(CASE WHEN "durationMs" IS NOT NULL AND "durationMs" > 0 THEN 1 ELSE 0 END) AS measuredRequests,
         AVG(CASE WHEN "durationMs" IS NOT NULL AND "durationMs" > 0 THEN "durationMs" END) AS avgDurationMs,
         SUM(CASE WHEN json_extract("response", '$.error') IS NOT NULL THEN 1 ELSE 0 END) AS failures
       FROM "llm_logs"
       WHERE "userId" = ? {type_clause}
         {not_null_clause}
       GROUP BY key, "provider", "modelName"
       ORDER BY requests DESC"#,
        group_by.key_expr()
    );

    let mut binds = vec![SqlValue::Text(user_id.to_string())];
    if let Some(t) = log_type {
        binds.push(SqlValue::Text(t.to_string()));
    }
    let params: Vec<&dyn ToSql> = binds.iter().map(|v| v as &dyn ToSql).collect();

    logs_rows(
        db,
        &sql,
        &params,
        &format!("llm-logs.getStatsByProfile:{}", group_by.context()),
        |r| {
            Ok(ProfileStatsRow {
                key: text(r, 0)?,
                provider: text(r, 1)?,
                model_name: text(r, 2)?,
                requests: num_col(r, 3)?,
                prompt_tokens: num_col(r, 4)?,
                completion_tokens: num_col(r, 5)?,
                total_tokens: num_col(r, 6)?,
                measured_requests: num_col(r, 7)?,
                avg_duration_ms: opt_num(r, 8)?,
                failures: num_col(r, 9)?,
            })
        },
    )
}

/// v4 `LLMLogsRepository.getMedianDurationByProfile` — median measured
/// `durationMs` per attribution key.
///
/// SQLite has no percentile aggregate, so this walks the ordered rows with a
/// window function and picks the middle one. Kept separate from the stats query
/// because the window pass is the expensive half.
fn get_median_duration_by_profile(db: &Db, user_id: &str, group_by: GroupBy) -> Vec<(String, f64)> {
    let key_expr = group_by.key_expr();
    let not_null_clause = match group_by.column() {
        Some(col) => format!(r#"AND "{col}" IS NOT NULL"#),
        None => String::new(),
    };
    let sql = format!(
        r#"WITH measured AS (
         SELECT {key_expr} AS key, "durationMs" AS d,
                ROW_NUMBER() OVER (PARTITION BY {key_expr} ORDER BY "durationMs") AS rn,
                COUNT(*)    OVER (PARTITION BY {key_expr})                        AS n
         FROM "llm_logs"
         WHERE "userId" = ?
           AND "durationMs" IS NOT NULL AND "durationMs" > 0
           {not_null_clause}
       )
       SELECT key, AVG(d) AS medianDurationMs
       FROM measured
       WHERE rn IN ((n + 1) / 2, (n + 2) / 2)
       GROUP BY key"#
    );
    logs_rows(
        db,
        &sql,
        &[&user_id],
        &format!("llm-logs.getMedianDurationByProfile:{}", group_by.context()),
        |r| Ok((text(r, 0)?, num_col(r, 1)?)),
    )
}

/// One `getCacheStats` row.
struct CacheStatsRow {
    key: String,
    rows_with_cache_usage: f64,
    rows_with_cache_read: f64,
    cache_read_tokens: f64,
    cache_creation_tokens: f64,
    prompt_tokens: f64,
}

/// v4 `LLMLogsRepository.getCacheStats` — the prompt-cache hit/miss roll-up.
///
/// `cacheUsage` is populated essentially only on `CHAT_MESSAGE` rows (the
/// streaming path is its sole writer). There is no boolean "was this a hit" — a
/// hit is derived as `cacheReadInputTokens > 0`, and the denominator is "rows
/// carrying any cacheUsage at all".
fn get_cache_stats(db: &Db, user_id: &str, group_by: &str) -> Vec<CacheStatsRow> {
    let sql = format!(
        r#"SELECT
         "{group_by}" AS key,
         COUNT(*)     AS rowsWithCacheUsage,
         SUM(CASE WHEN COALESCE(json_extract("cacheUsage", '$.cacheReadInputTokens'), 0) > 0 THEN 1 ELSE 0 END) AS rowsWithCacheRead,
         COALESCE(SUM(json_extract("cacheUsage", '$.cacheReadInputTokens')), 0)     AS cacheReadTokens,
         COALESCE(SUM(json_extract("cacheUsage", '$.cacheCreationInputTokens')), 0) AS cacheCreationTokens,
         COALESCE(SUM(json_extract("usage", '$.promptTokens')), 0)                  AS promptTokens
       FROM "llm_logs"
       WHERE "userId" = ?
         AND "cacheUsage" IS NOT NULL
         AND "{group_by}" IS NOT NULL
       GROUP BY key
       ORDER BY rowsWithCacheUsage DESC"#
    );
    logs_rows(
        db,
        &sql,
        &[&user_id],
        &format!("llm-logs.getCacheStats:{group_by}"),
        |r| {
            Ok(CacheStatsRow {
                key: text(r, 0)?,
                rows_with_cache_usage: num_col(r, 1)?,
                rows_with_cache_read: num_col(r, 2)?,
                cache_read_tokens: num_col(r, 3)?,
                cache_creation_tokens: num_col(r, 4)?,
                prompt_tokens: num_col(r, 5)?,
            })
        },
    )
}

/// v4 `countByUserId` — the log-entry total.
fn count_by_user_id(db: &Db, user_id: &str) -> f64 {
    logs_rows(
        db,
        r#"SELECT COUNT(*) AS n FROM "llm_logs" WHERE "userId" = ?"#,
        &[&user_id],
        "llm-logs.countByUserId",
        |r| num_col(r, 0),
    )
    .first()
    .copied()
    .unwrap_or(0.0)
}

/// v4 `getTotalTokenUsage` — the lifetime token totals over rows that HAVE a
/// `usage` payload.
///
/// v4 sums in JS after hydrating rows matching `usage: { $exists: true }`; the
/// `$ne: null` that used to sit beside it made the whole method return zeroes on
/// every install (the SQLite translator emitted `usage != NULL`, unknown for
/// every row), and the Almanack rendering exactly this number is how that was
/// finally traced. `$exists: true` lowers to `usage IS NOT NULL`, which is what
/// this query asks; the per-field `|| 0` guards become `COALESCE`.
fn get_total_token_usage(db: &Db, user_id: &str) -> TokenUsage {
    let row = logs_rows(
        db,
        r#"SELECT COALESCE(SUM(COALESCE(json_extract("usage", '$.promptTokens'), 0)), 0)     AS promptTokens,
              COALESCE(SUM(COALESCE(json_extract("usage", '$.completionTokens'), 0)), 0) AS completionTokens,
              COALESCE(SUM(COALESCE(json_extract("usage", '$.totalTokens'), 0)), 0)      AS totalTokens
       FROM "llm_logs" WHERE "userId" = ? AND "usage" IS NOT NULL"#,
        &[&user_id],
        "llm-logs.getTotalTokenUsage",
        |r| Ok((num_col(r, 0)?, num_col(r, 1)?, num_col(r, 2)?)),
    );
    let (prompt, completion, total) = row.first().copied().unwrap_or((0.0, 0.0, 0.0));
    TokenUsage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: total,
    }
}

/// v4 `collectProfileLifetime` — the counters carried on the
/// `connection_profiles` rows themselves (MAIN database).
fn collect_profile_lifetime(db: &Db, user_id: &str) -> Vec<ProfileLifetimeRow> {
    main_rows(
        db,
        r#"SELECT "id", "name", "provider", "modelName",
            COALESCE("totalTokens", 0)           AS totalTokens,
            COALESCE("totalPromptTokens", 0)     AS totalPromptTokens,
            COALESCE("totalCompletionTokens", 0) AS totalCompletionTokens,
            COALESCE("messageCount", 0)          AS messageCount,
            "updatedAt"
     FROM "connection_profiles"
     WHERE "userId" = ?
     ORDER BY totalTokens DESC"#,
        &[&user_id],
        "wire.profileLifetime",
        |r| {
            Ok(ProfileLifetimeRow {
                id: text(r, 0)?,
                name: text(r, 1)?,
                provider: text(r, 2)?,
                model_name: text(r, 3)?,
                total_tokens: num_col(r, 4)?,
                total_prompt_tokens: num_col(r, 5)?,
                total_completion_tokens: num_col(r, 6)?,
                message_count: num_col(r, 7)?,
                // There is no `lastUsedAt` column; `updatedAt` moves whenever
                // the counters are bumped, so it is the closest proxy — and is
                // labelled as one in the rendered table.
                last_touched_at: opt_text(r, 8)?,
            })
        },
    )
}

/// v4 `collectWireRecords(userId)` — the whole phase.
pub fn collect_wire_records(db: &Db, user_id: &str) -> Result<WireRecordsInfo, DbError> {
    // v4 reads the three logging dials off chat_settings inside a try/catch.
    let settings = db
        .read_main(|c| crate::db::chat_settings::find_by_user_id(c, user_id))
        .unwrap_or(None)
        .unwrap_or(serde_json::Value::Null);
    let logging = settings.get("llmLoggingSettings");
    let logging_enabled = logging
        .and_then(|v| v.get("enabled"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let verbose_mode = logging
        .and_then(|v| v.get("verboseMode"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let retention_days = logging
        .and_then(|v| v.get("retentionDays"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(30.0);

    let exact_attribution = has_profile_attribution_columns(db);
    let connection_group_by = if exact_attribution {
        GroupBy::ConnectionProfileId
    } else {
        GroupBy::ProviderModel
    };
    let image_group_by = if exact_attribution {
        GroupBy::ImageProfileId
    } else {
        GroupBy::ProviderModel
    };

    let total_entries = count_by_user_id(db, user_id);
    let token_usage = get_total_token_usage(db, user_id);
    let by_type = get_stats_by_type(db, user_id);
    let lifetime = collect_profile_lifetime(db, user_id);
    let connection_stats = get_stats_by_profile(db, user_id, connection_group_by, None);
    let connection_medians = get_median_duration_by_profile(db, user_id, connection_group_by);
    let image_stats = get_stats_by_profile(db, user_id, image_group_by, Some("IMAGE_GENERATION"));
    let image_medians = get_median_duration_by_profile(db, user_id, image_group_by);
    let cache_by_provider_rows = get_cache_stats(db, user_id, "provider");
    // v4 skips the per-profile cache query entirely on the approximate arm
    // (there is no profile column to group by).
    let cache_by_profile_rows = if exact_attribution {
        get_cache_stats(db, user_id, "connectionProfileId")
    } else {
        Vec::new()
    };

    let connection_names = profile_names(
        db,
        "connection_profiles",
        user_id,
        "wire.connectionProfileNames",
    );
    let image_names = profile_names(db, "image_profiles", user_id, "wire.imageProfileNames");

    /// v4's `labelFor` — an attribution key turned into something a reader
    /// recognises. On the approximate arm the key IS `provider/model` already.
    fn label_for(key: &str, names: &HashMap<String, String>, exact: bool) -> String {
        if !exact {
            return key.to_string();
        }
        names.get(key).cloned().unwrap_or_else(|| {
            // v4 `key.slice(0, 8)` — UTF-16 code units; profile ids are ASCII.
            format!(
                "(deleted profile {}…)",
                crate::jsstr::utf16_truncate(key, 8)
            )
        })
    }

    let to_window_rows = |stats: Vec<ProfileStatsRow>,
                          medians: Vec<(String, f64)>,
                          names: &HashMap<String, String>|
     -> Vec<ProfileWindowRow> {
        let median_by_key: HashMap<String, f64> = medians.into_iter().collect();
        stats
            .into_iter()
            .take(PROFILE_ROW_LIMIT)
            .map(|row| ProfileWindowRow {
                label: label_for(&row.key, names, exact_attribution),
                provider: row.provider,
                model_name: row.model_name,
                requests: row.requests,
                prompt_tokens: row.prompt_tokens,
                completion_tokens: row.completion_tokens,
                total_tokens: row.total_tokens,
                measured_requests: row.measured_requests,
                avg_duration_ms: row.avg_duration_ms,
                median_duration_ms: median_by_key.get(&row.key).copied(),
                failures: row.failures,
            })
            .collect()
    };

    let to_cache_rows = |rows: Vec<CacheStatsRow>,
                         names: Option<&HashMap<String, String>>|
     -> Vec<CacheRow> {
        rows.into_iter()
            .map(|row| {
                // Provider plugins strip cache-read tokens out of
                // `usage.promptTokens` (the plugin cache-accounting contract),
                // so what remains there is the uncached half and the three add
                // up to the full prompt.
                let total_prompt = row.prompt_tokens + row.cache_read_tokens;
                CacheRow {
                    label: match names {
                        Some(n) => label_for(&row.key, n, exact_attribution),
                        None => row.key.clone(),
                    },
                    rows_with_cache_usage: row.rows_with_cache_usage,
                    rows_with_cache_read: row.rows_with_cache_read,
                    cache_read_tokens: row.cache_read_tokens,
                    cache_creation_tokens: row.cache_creation_tokens,
                    uncached_prompt_tokens: row.prompt_tokens,
                    request_hit_ratio: ratio(row.rows_with_cache_read, row.rows_with_cache_usage),
                    token_hit_ratio: ratio(row.cache_read_tokens, total_prompt),
                }
            })
            .collect()
    };

    Ok(WireRecordsInfo {
        total_entries,
        token_usage,
        logging_enabled,
        verbose_mode,
        retention_days,
        exact_profile_attribution: exact_attribution,
        by_type,
        connection_profile_lifetime: lifetime,
        connection_profile_window: to_window_rows(
            connection_stats,
            connection_medians,
            &connection_names,
        ),
        image_profile_window: to_window_rows(image_stats, image_medians, &image_names),
        cache_by_provider: to_cache_rows(cache_by_provider_rows, None),
        cache_by_profile: to_cache_rows(cache_by_profile_rows, Some(&connection_names)),
    })
}

/// `SELECT id, name FROM <table> WHERE userId = ?` — the id→name maps the
/// exact-attribution labels resolve through.
fn profile_names(db: &Db, table: &str, user_id: &str, context: &str) -> HashMap<String, String> {
    main_rows(
        db,
        &format!(r#"SELECT "id", "name" FROM "{table}" WHERE "userId" = ?"#),
        &[&user_id],
        context,
        |r| Ok((text(r, 0)?, text(r, 1)?)),
    )
    .into_iter()
    .collect()
}
