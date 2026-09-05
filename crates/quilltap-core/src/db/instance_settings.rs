//! The `instance_settings` key/value store (v4 `lib/instance-settings/index.ts`).
//!
//! `instance_settings` is a tiny per-instance key/value table in the **main** db
//! (`"key" TEXT PRIMARY KEY, "value" TEXT NOT NULL` — see v4's
//! `lib/startup/version-guard.ts`, which `CREATE TABLE IF NOT EXISTS`es it). The
//! port needs only the one reader the wardrobe archetype tier depends on:
//! `getGeneralMountPointId`, the id of the singleton "Quilltap General" document
//! store that houses shared wardrobe/scenario archetypes.
//!
//! v4's `readSetting` wraps the `SELECT` in a try/catch and returns `null` on any
//! error (a freshly-cloned instance may not have the table yet — the provisioning
//! migration writes the key on first boot). We reproduce that: a query error
//! (including `no such table`) resolves to `None`, never propagating.

use rusqlite::{params, Connection};

use super::DbError;

/// The `instance_settings` key that stores the Quilltap General mount-point id.
const KEY_GENERAL_MOUNT_POINT_ID: &str = "generalMountPointId";

/// v4 `KEY_MAX_CONCURRENT_JOBS` — the per-instance background-job concurrency cap.
const KEY_MAX_CONCURRENT_JOBS: &str = "maxConcurrentJobs";

/// v4 `DEFAULT_MAX_CONCURRENT_JOBS`.
const DEFAULT_MAX_CONCURRENT_JOBS: i64 = 4;

/// v4 `KEY_LAST_MAINTENANCE_SWEEP_AT` — the last daily-maintenance-pass timestamp
/// (ISO 8601), used by the host driver to short-circuit the dev-restart startup
/// tick.
const KEY_LAST_MAINTENANCE_SWEEP_AT: &str = "lastMaintenanceSweepAt";

/// The `instance_settings` key that stores the singleton "Lantern Backgrounds"
/// mount-point id (v4 `getLanternBackgroundsMountPointId`), provisioned by
/// `provision-lantern-backgrounds-mount-v1`. Houses generated story backgrounds
/// (`generated/`) + ad-hoc `generate_image` tool output (`tool/`).
const KEY_LANTERN_BACKGROUNDS_MOUNT_POINT_ID: &str = "lanternBackgroundsMountPointId";

/// v4 `KEY_MEMORY_RECALL` — the per-instance Commonplace-Book recall settings.
const KEY_MEMORY_RECALL: &str = "memoryRecall";

/// v4 `KEY_DATA_RETENTION` — the per-instance data-retention settings (the
/// stale-chat window that governs the daily maintenance sweep's cache collapse,
/// image collapse, and conversation-chunk cold-tiering).
const KEY_DATA_RETENTION: &str = "dataRetention";

/// v4 `DEFAULT_DATA_RETENTION_SETTINGS.staleChatDays` — the documented default.
pub const DEFAULT_STALE_CHAT_DAYS: i64 = 30;

/// v4 `DataRetentionSettingsSchema` bound: `z.number().int().min(1).max(3650)`.
pub const STALE_CHAT_DAYS_MIN: i64 = 1;
/// v4 `DataRetentionSettingsSchema` bound: `z.number().int().min(1).max(3650)`.
pub const STALE_CHAT_DAYS_MAX: i64 = 3650;

/// v4 `readSetting(key)` — read one `instance_settings` value, or `None`.
///
/// Faithful to v4: the whole read is fallible-tolerant — a missing table or any
/// other SQLite error resolves to `None` (v4 logs a warning and returns null).
fn read_setting(main: &Connection, key: &str) -> Option<String> {
    main.query_row(
        "SELECT \"value\" FROM \"instance_settings\" WHERE \"key\" = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// v4 `writeSetting(key, value)` — upsert one `instance_settings` value.
fn write_setting(main: &Connection, key: &str, value: &str) -> Result<(), DbError> {
    main.execute(
        "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES (?1, ?2) \
         ON CONFLICT(\"key\") DO UPDATE SET \"value\" = excluded.\"value\"",
        params![key, value],
    )?;
    Ok(())
}

/// v4 `getGeneralMountPointId()` — the Quilltap General mount-point id, or `None`
/// when the General store has not been provisioned (or the table is absent).
pub fn get_general_mount_point_id(main: &Connection) -> Result<Option<String>, DbError> {
    Ok(read_setting(main, KEY_GENERAL_MOUNT_POINT_ID))
}

/// v4 `getLanternBackgroundsMountPointId()` — the Lantern Backgrounds mount-point
/// id, or `None` when the store has not been provisioned (or the table is absent).
pub fn get_lantern_backgrounds_mount_point_id(
    main: &Connection,
) -> Result<Option<String>, DbError> {
    Ok(read_setting(main, KEY_LANTERN_BACKGROUNDS_MOUNT_POINT_ID))
}

/// Instance-wide Commonplace-Book recall settings (v4 `MemoryRecallSettings`,
/// `lib/schemas/settings.types.ts`). One struct rather than a tuple since v4
/// `870a57fa` added a third field — a tuple that grows a field silently
/// re-orders every destructuring that reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRecallSettings {
    /// `'down-weight' | 'exclude'` — how cross-project memories are treated.
    pub scope_policy: String,
    /// Pull in memories related to the ones that matched.
    pub expand_related: bool,
    /// v4 `870a57fa`: re-run the vault conversation-summary search EVERY turn
    /// and fold the list into the consolidated whisper. Instance-wide by design
    /// (no chat / project / character override).
    pub per_turn_conversation_summaries: bool,
}

impl Default for MemoryRecallSettings {
    fn default() -> Self {
        // v4 `DEFAULT_MEMORY_RECALL_SETTINGS` (`lib/instance-settings/index.ts`).
        MemoryRecallSettings {
            scope_policy: "down-weight".to_string(),
            expand_related: false,
            per_turn_conversation_summaries: false,
        }
    }
}

impl MemoryRecallSettings {
    /// v4's stored/returned JSON shape — the Zod schema's declaration order
    /// (`scopePolicy`, `expandRelated`, `perTurnConversationSummaries`), which is
    /// both what `setMemoryRecallSettings` writes and what the route echoes.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "scopePolicy": self.scope_policy,
            "expandRelated": self.expand_related,
            "perTurnConversationSummaries": self.per_turn_conversation_summaries,
        })
    }
}

/// v4 `getMemoryRecallSettings()` — the per-instance Commonplace-Book recall
/// settings. Returns the documented default (`down-weight`, no expand, no
/// per-turn conversations) when the setting is unwritten OR fails to parse (v4's
/// Zod `safeParse` → `catch` → default). The Zod schema has `scopePolicy` enum
/// `['down-weight','exclude']` (`.default('down-weight')`), `expandRelated`
/// boolean (`.default(false)`) and `perTurnConversationSummaries` boolean
/// (`.default(false)`); an out-of-enum / non-bool value fails the parse (a
/// `.default`-carrying key means a bad *value* still fails, not defaults —
/// faithful to `.parse` throwing on a present-but-wrong value).
pub fn get_memory_recall_settings(main: &Connection) -> Result<MemoryRecallSettings, DbError> {
    let Some(raw) = read_setting(main, KEY_MEMORY_RECALL) else {
        return Ok(MemoryRecallSettings::default());
    };
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&raw);
    let Ok(obj) = parsed else {
        return Ok(MemoryRecallSettings::default());
    };
    // scopePolicy: enum, `.default('down-weight')` (absent → default; present but
    // out-of-enum → parse fails → whole-object default).
    let scope_policy = match obj.get("scopePolicy") {
        None => MemoryRecallSettings::default().scope_policy,
        Some(serde_json::Value::String(s)) if s == "down-weight" || s == "exclude" => s.clone(),
        Some(_) => return Ok(MemoryRecallSettings::default()),
    };
    // expandRelated: boolean, `.default(false)`.
    let expand_related = match obj.get("expandRelated") {
        None => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(_) => return Ok(MemoryRecallSettings::default()),
    };
    // perTurnConversationSummaries: boolean, `.default(false)` — same arm shape.
    let per_turn_conversation_summaries = match obj.get("perTurnConversationSummaries") {
        None => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(_) => return Ok(MemoryRecallSettings::default()),
    };
    Ok(MemoryRecallSettings {
        scope_policy,
        expand_related,
        per_turn_conversation_summaries,
    })
}

/// v4 `setMemoryRecallSettings` — store the recall settings (validated object
/// `{scopePolicy, expandRelated, perTurnConversationSummaries}`). The route
/// merges before calling this.
pub fn set_memory_recall_settings(
    main: &Connection,
    settings: &MemoryRecallSettings,
) -> Result<(), DbError> {
    write_setting(main, KEY_MEMORY_RECALL, &settings.to_json().to_string())
}

/// Validate a candidate `staleChatDays` against v4's
/// `DataRetentionSettingsSchema` field (`z.number().int().min(1).max(3650)`):
/// the JSON value must be an integer-valued number in `[1, 3650]`. Returns the
/// validated `i64`, or `None` on any violation (non-number, non-integer,
/// out-of-range) — mirroring Zod `.parse` throwing.
pub fn validate_stale_chat_days(value: &serde_json::Value) -> Option<i64> {
    let n = value.as_f64()?;
    if !n.is_finite() || n.fract() != 0.0 {
        return None; // `.int()` rejects non-integers / NaN / Inf
    }
    let days = n as i64;
    (STALE_CHAT_DAYS_MIN..=STALE_CHAT_DAYS_MAX)
        .contains(&days)
        .then_some(days)
}

/// v4 `getDataRetentionSettings()` — the effective `staleChatDays` window.
///
/// Returns the documented default (30) when the setting is unset. When the
/// stored blob is present but not a valid `DataRetentionSettingsSchema` object
/// (unparseable, non-object, or a `staleChatDays` outside `[1, 3650]` / not an
/// integer) v4 logs a warning and falls back to the default — reproduced here.
/// A stored object that OMITS `staleChatDays` parses via Zod `.default(30)` → 30.
pub fn get_data_retention_settings(main: &Connection) -> Result<i64, DbError> {
    let Some(raw) = read_setting(main, KEY_DATA_RETENTION) else {
        return Ok(DEFAULT_STALE_CHAT_DAYS);
    };
    let parsed = match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(serde_json::Value::Object(map)) => match map.get("staleChatDays") {
            // Zod `.default(30)` — a present object with the key absent.
            None => DEFAULT_STALE_CHAT_DAYS,
            Some(v) => validate_stale_chat_days(v).unwrap_or(DEFAULT_STALE_CHAT_DAYS),
        },
        // Non-object JSON (`z.object` throws) or unparseable → warn + default.
        _ => DEFAULT_STALE_CHAT_DAYS,
    };
    Ok(parsed)
}

/// v4 `setDataRetentionSettings(value)` — validate then persist the
/// data-retention settings object. The caller has already merged over the
/// current value (the PUT route's `{...current, ...body}`); this validates
/// `staleChatDays` against the schema and writes the canonical JSON. Returns the
/// validated `staleChatDays` (the parsed echo the route sends back).
pub fn set_data_retention_settings(main: &Connection, stale_chat_days: i64) -> Result<(), DbError> {
    let value = serde_json::json!({ "staleChatDays": stale_chat_days });
    write_setting(main, KEY_DATA_RETENTION, &value.to_string())
}

// ---------------------------------------------------------------------------
// P4.D50 — the Taboo list (`instance_settings['taboo']`, v4 `7df7de8e`).
// ---------------------------------------------------------------------------

/// v4 `KEY_TABOO` — the per-instance list of phrases no character may utter.
/// Same instance-settings class as `dataRetention` (single-user model), so it
/// needs no migration and — being absent from
/// [`NON_PORTABLE_INSTANCE_SETTING_KEYS`] — is portable by default: it exports
/// with `.qtap` instance-settings and rides along in full backups.
const KEY_TABOO: &str = "taboo";

/// v4 `TabooSettingsSchema`: `z.string().trim().min(1).max(200)` per entry.
/// The bound is JS `String.length` — UTF-16 code units, not chars.
pub const TABOO_MAX_PHRASE_LENGTH: usize = 200;
/// v4 `TabooSettingsSchema`: `.max(500)` on the array.
pub const TABOO_MAX_PHRASES: usize = 500;

/// Parse a candidate value against v4's `TabooSettingsSchema`
/// (`z.object({ phrases: z.array(z.string().trim().min(1).max(200)).max(500)
/// .default([]) })`), returning the parsed (TRIMMED) phrases or `None` when Zod
/// would throw.
///
/// Zod check ORDER is load-bearing: `.trim()` is an overwrite check that runs
/// BEFORE `.min(1)`/`.max(200)`, so a 201-character entry that trims to 200 is
/// valid and a whitespace-only entry is rejected (it trims to length 0, failing
/// `.min(1)`) rather than silently dropped. Unknown keys are stripped, as Zod's
/// default object mode does; `phrases` absent → `.default([])`.
pub fn parse_taboo_settings(value: &serde_json::Value) -> Option<Vec<String>> {
    use crate::jsstr::{js_trim, zod_len_max_ok, zod_len_min_ok};

    let obj = value.as_object()?; // `z.object` throws on a non-object
    let Some(raw) = obj.get("phrases") else {
        return Some(Vec::new()); // `.default([])`
    };
    // An explicit `undefined` cannot survive JSON; `null` is NOT undefined and
    // fails the array check (Zod's `.default` only fires for `undefined`).
    let arr = raw.as_array()?;
    if arr.len() > TABOO_MAX_PHRASES {
        return None;
    }
    let mut out = Vec::with_capacity(arr.len());
    for entry in arr {
        let s = entry.as_str()?; // `z.string()` throws on a non-string
        let trimmed = js_trim(s);
        // `z.string().trim().min(1).max(200)` under Zod ≥ 4.5.4's code-point
        // windows (v4 `6e1a64ea6`): 101 astral characters (202 units, 101 code
        // points) now pass the 200 bound where 4.4.3 refused them.
        if !(zod_len_min_ok(trimmed, 1) && zod_len_max_ok(trimmed, TABOO_MAX_PHRASE_LENGTH)) {
            return None;
        }
        out.push(trimmed.to_string());
    }
    Some(out)
}

/// v4 `normalizeTabooPhrases(phrases)` — trim each entry, drop the ones that
/// trimmed away to nothing, and drop case-insensitive duplicates keeping the
/// FIRST occurrence.
///
/// Order is deliberately preserved rather than sorted. The rendered section sits
/// inside the cacheable system-prompt prefix, so the byte order matters; leaving
/// it under the user's control means it only shifts when they actually edit the
/// list (a legitimate cache invalidation) instead of every time a phrase is
/// added in the "wrong" alphabetical spot.
pub fn normalize_taboo_phrases(phrases: &[String]) -> Vec<String> {
    use crate::jsstr::js_trim;
    use std::collections::HashSet;

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for raw in phrases {
        let phrase = js_trim(raw);
        if phrase.is_empty() {
            continue;
        }
        // v4's fingerprint is `phrase.toLowerCase()`; Phase 1 proved
        // `str::to_lowercase` byte-identical to JS `toLowerCase`.
        let fingerprint = phrase.to_lowercase();
        if !seen.insert(fingerprint) {
            continue;
        }
        out.push(phrase.to_string());
    }
    out
}

/// v4 `getTabooSettings()` — the per-instance Taboo list, the phrases no
/// character may utter.
///
/// Returns an empty list when the setting has never been written, which is what
/// suppresses the prompt section entirely (see
/// [`crate::system_prompt::render_taboo_section`]). A stored blob that does not
/// parse (bad JSON, wrong shape, an out-of-range phrase) is v4's `catch` arm:
/// warn and fall back to the default. Read once per turn on the conversational
/// path ([`crate::services::build_context::build_context`]).
pub fn get_taboo_settings(main: &Connection) -> Result<Vec<String>, DbError> {
    let Some(raw) = read_setting(main, KEY_TABOO) else {
        return Ok(Vec::new());
    };
    let parsed = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| parse_taboo_settings(&v));
    match parsed {
        Some(phrases) => Ok(phrases),
        None => {
            tracing::warn!("[InstanceSettings] taboo failed to parse — using defaults");
            Ok(Vec::new())
        }
    }
}

/// v4 `setTabooSettings(value)` — persist the Taboo list, normalized (see
/// [`normalize_taboo_phrases`]). Returns exactly what was stored so callers —
/// the PUT route, and through it the settings UI — echo the normalized list
/// rather than the raw one they submitted.
///
/// v4 re-validates the normalized list through the schema before writing and
/// lets a violation THROW (which the PUT route's catch turns into its 500).
/// `Ok(None)` IS that throw — nothing is written, and it is deliberately not a
/// [`DbError`] because no database operation failed. Normalization never
/// introduces a violation the input did not already carry, and the route
/// pre-parses, so the refusal is reachable only from a direct caller (v4's own
/// accessor suite exercises exactly that).
pub fn set_taboo_settings(
    main: &Connection,
    phrases: &[String],
) -> Result<Option<Vec<String>>, DbError> {
    let normalized = normalize_taboo_phrases(phrases);
    let candidate = serde_json::json!({ "phrases": normalized });
    let Some(validated) = parse_taboo_settings(&candidate) else {
        return Ok(None);
    };
    let value = serde_json::json!({ "phrases": validated });
    write_setting(main, KEY_TABOO, &value.to_string())?;
    Ok(Some(validated))
}

// ---------------------------------------------------------------------------
// P4.D57 — the Brahma Console turn budget (`instance_settings['brahmaConsole']`,
// v4 `6452e2c3`).
// ---------------------------------------------------------------------------

/// v4 `KEY_BRAHMA_CONSOLE` — the per-instance Brahma Console settings blob (the
/// agent-turn budget the streaming orchestrator and the one-shot `@Brahma` path
/// both read). Same instance-settings class as `dataRetention`/`taboo`
/// (single-user model), so it needs no migration and — being absent from
/// [`NON_PORTABLE_INSTANCE_SETTING_KEYS`] — is portable by default: it exports
/// with `.qtap` instance-settings and rides along in full backups.
const KEY_BRAHMA_CONSOLE: &str = "brahmaConsole";

/// v4 `DEFAULT_BRAHMA_CONSOLE_SETTINGS.maxAgentTurns` / the schema's
/// `.default(50)` / `DEFAULT_BRAHMA_MAX_AGENT_TURNS` — the documented default,
/// raised from the old hardcoded 25 when the budget became a setting.
pub const DEFAULT_BRAHMA_MAX_AGENT_TURNS: i64 = 50;

/// v4 `BrahmaConsoleSettingsSchema.maxAgentTurns`: `z.number().int().min(5)`.
pub const BRAHMA_MAX_AGENT_TURNS_MIN: i64 = 5;
/// v4 `BrahmaConsoleSettingsSchema.maxAgentTurns`: `.max(200)`.
pub const BRAHMA_MAX_AGENT_TURNS_MAX: i64 = 200;

/// Parse a candidate value against v4's `BrahmaConsoleSettingsSchema`
/// (`z.object({ maxAgentTurns: z.number().int().min(5).max(200).default(50) })`),
/// returning the validated `maxAgentTurns` or `None` when Zod `.parse` would
/// throw.
///
/// The mirror of [`validate_stale_chat_days`], only object-wrapped like taboo:
/// `z.object` throws on a non-object; an object that OMITS `maxAgentTurns`
/// parses via `.default(50)` → 50; a present `maxAgentTurns` must be an
/// integer-valued number in `[5, 200]` (an explicit `null` fails `z.number()`,
/// since `.default` fires only for `undefined`).
pub fn parse_brahma_console_settings(value: &serde_json::Value) -> Option<i64> {
    let obj = value.as_object()?; // `z.object` throws on a non-object
    match obj.get("maxAgentTurns") {
        // Zod `.default(50)` — a present object with the key absent.
        None => Some(DEFAULT_BRAHMA_MAX_AGENT_TURNS),
        Some(v) => {
            let n = v.as_f64()?; // `null` / non-number → `z.number()` throws
            if !n.is_finite() || n.fract() != 0.0 {
                return None; // `.int()` rejects non-integers / NaN / Inf
            }
            let turns = n as i64;
            (BRAHMA_MAX_AGENT_TURNS_MIN..=BRAHMA_MAX_AGENT_TURNS_MAX)
                .contains(&turns)
                .then_some(turns)
        }
    }
}

/// v4 `getBrahmaConsoleSettings()` — the effective `maxAgentTurns` budget.
///
/// Returns the documented default (50) when the setting is unset. When the
/// stored blob is present but not a valid `BrahmaConsoleSettingsSchema` object
/// (unparseable, non-object, or a `maxAgentTurns` outside `[5, 200]` / not an
/// integer) v4 logs a warning and falls back to the default — reproduced here.
/// A stored object that OMITS `maxAgentTurns` parses via Zod `.default(50)` → 50
/// (no warning). The read itself is fallible-tolerant (a missing table on a
/// pre-provisioning instance resolves to the default).
pub fn get_brahma_console_settings(main: &Connection) -> Result<i64, DbError> {
    let Some(raw) = read_setting(main, KEY_BRAHMA_CONSOLE) else {
        return Ok(DEFAULT_BRAHMA_MAX_AGENT_TURNS);
    };
    let parsed = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| parse_brahma_console_settings(&v));
    match parsed {
        Some(turns) => Ok(turns),
        None => {
            tracing::warn!("[InstanceSettings] brahmaConsole failed to parse — using defaults");
            Ok(DEFAULT_BRAHMA_MAX_AGENT_TURNS)
        }
    }
}

/// v4 `setBrahmaConsoleSettings(value)` — validate then persist the Brahma
/// Console settings object. The caller has already merged over the current value
/// (the PUT route's `{...current, ...body}`); this re-validates `maxAgentTurns`
/// against the schema (v4's `BrahmaConsoleSettingsSchema.parse` before the
/// write) and stores the canonical JSON, returning the validated value the route
/// echoes back.
///
/// v4's setter lets a schema violation THROW without writing; `Ok(None)` IS that
/// throw — nothing is written, and it is deliberately not a [`DbError`] because
/// no database operation failed (the taboo `set_taboo_settings` precedent). The
/// route pre-parses, so the refusal is reachable only from a direct caller
/// (v4's own accessor suite exercises exactly that).
pub fn set_brahma_console_settings(
    main: &Connection,
    max_agent_turns: i64,
) -> Result<Option<i64>, DbError> {
    let candidate = serde_json::json!({ "maxAgentTurns": max_agent_turns });
    let Some(validated) = parse_brahma_console_settings(&candidate) else {
        return Ok(None);
    };
    let value = serde_json::json!({ "maxAgentTurns": validated });
    write_setting(main, KEY_BRAHMA_CONSOLE, &value.to_string())?;
    Ok(Some(validated))
}

// === end P4.D57 ===

/// v4 `KEY_MEMORY_EXTRACTION_CONCURRENCY` — the per-instance MEMORY_EXTRACTION
/// concurrency cap (default **1**, distinct from `maxConcurrentJobs`'s 4).
const KEY_MEMORY_EXTRACTION_CONCURRENCY: &str = "memoryExtractionConcurrency";
const DEFAULT_MEMORY_EXTRACTION_CONCURRENCY: i64 = 1;
/// v4 `KEY_MEMORY_EXTRACTION_LIMITS`.
const KEY_MEMORY_EXTRACTION_LIMITS: &str = "memoryExtractionLimits";

/// v4 `getMemoryExtractionConcurrency()` — default 1 when unset OR when the stored
/// value is not a finite integer `>= 1`; otherwise clamped to `[1, 32]`.
pub fn get_memory_extraction_concurrency(main: &Connection) -> Result<i64, DbError> {
    let Some(raw) = read_setting(main, KEY_MEMORY_EXTRACTION_CONCURRENCY) else {
        return Ok(DEFAULT_MEMORY_EXTRACTION_CONCURRENCY);
    };
    match raw.trim().parse::<f64>() {
        Ok(f) if f.is_finite() && f.floor() as i64 >= 1 => Ok((f.floor() as i64).clamp(1, 32)),
        _ => Ok(DEFAULT_MEMORY_EXTRACTION_CONCURRENCY),
    }
}

/// v4 `setMemoryExtractionConcurrency(value)` — clamp `[1, 32]` (floor) and store.
pub fn set_memory_extraction_concurrency(main: &Connection, value: i64) -> Result<(), DbError> {
    let clamped = value.clamp(1, 32);
    write_setting(
        main,
        KEY_MEMORY_EXTRACTION_CONCURRENCY,
        &clamped.to_string(),
    )
}

/// v4 `getMemoryExtractionLimits()` — the `{enabled, maxPerHour, softStartFraction,
/// softFloor}` object; documented defaults `{false, 20, 0.7, 0.7}` when unset /
/// malformed.
pub fn get_memory_extraction_limits(main: &Connection) -> Result<serde_json::Value, DbError> {
    let default = serde_json::json!({
        "enabled": false, "maxPerHour": 20, "softStartFraction": 0.7, "softFloor": 0.7,
    });
    let Some(raw) = read_setting(main, KEY_MEMORY_EXTRACTION_LIMITS) else {
        return Ok(default);
    };
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(v) if v.is_object() => Ok(v),
        _ => Ok(default),
    }
}

/// v4 `setMemoryExtractionLimits(value)` — store the validated object (the route
/// merges + validates before calling).
pub fn set_memory_extraction_limits(
    main: &Connection,
    value: &serde_json::Value,
) -> Result<(), DbError> {
    write_setting(main, KEY_MEMORY_EXTRACTION_LIMITS, &value.to_string())
}

/// v4 `getMaxConcurrentJobs()` — the per-instance background-job concurrency cap.
/// Returns the documented default (4) when unset OR when the stored value is not a
/// finite integer `>= 1` (v4: `Math.floor(Number(raw))`, reject `!Number.isFinite`
/// or `< 1`); otherwise the parsed value clamped to `[1, 32]`. A missing table
/// reads as `None` → default, matching v4's `readSetting` try/catch.
pub fn get_max_concurrent_jobs(main: &Connection) -> Result<i64, DbError> {
    let Some(raw) = read_setting(main, KEY_MAX_CONCURRENT_JOBS) else {
        return Ok(DEFAULT_MAX_CONCURRENT_JOBS);
    };
    // v4: `Math.floor(Number(raw))`; `Number("")`/non-numeric → NaN → default.
    let parsed = raw.trim().parse::<f64>();
    match parsed {
        Ok(n) if n.is_finite() && n >= 1.0 => Ok((n.floor() as i64).clamp(1, 32)),
        _ => Ok(DEFAULT_MAX_CONCURRENT_JOBS),
    }
}

/// v4 `setMaxConcurrentJobs(value)` (`lib/instance-settings/index.ts:95`): reject
/// a non-finite value, floor + clamp to `[1, 32]`, and persist as a STRING
/// (P4.9G1 — the tasks-queue concurrency cap setter). The Zod-1..32 gate at the
/// verb edge means production callers never reach the clamp, but it carries v4's
/// storage semantics faithfully.
pub fn set_max_concurrent_jobs(main: &Connection, value: i64) -> Result<(), DbError> {
    let clamped = value.clamp(1, 32);
    write_setting(main, KEY_MAX_CONCURRENT_JOBS, &clamped.to_string())
}

/// v4 `getLastMaintenanceSweepAt()` — the last daily-maintenance-pass instant as
/// Unix milliseconds, or `None` when never recorded / unparseable (v4 returns a
/// `Date` or null; the host driver only needs the instant for the recent-run
/// window). Parsed via [`crate::clock::iso_to_ms`] (the `Date.parse` inverse).
pub fn get_last_maintenance_sweep_at(main: &Connection) -> Result<Option<i64>, DbError> {
    Ok(read_setting(main, KEY_LAST_MAINTENANCE_SWEEP_AT)
        .as_deref()
        .and_then(crate::clock::iso_to_ms))
}

/// v4 `setLastMaintenanceSweepAt(when)` — record the timestamp of a completed
/// maintenance pass (ISO 8601). Defaults to now. Written at the end of a pass
/// regardless of per-sweep failures ("last attempted pass").
pub fn set_last_maintenance_sweep_at(main: &Connection, when_iso: &str) -> Result<(), DbError> {
    write_setting(main, KEY_LAST_MAINTENANCE_SWEEP_AT, when_iso)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn missing_table_yields_none() {
        // v4's readSetting try/catch returns null when the table doesn't exist.
        let c = conn();
        assert_eq!(get_general_mount_point_id(&c).unwrap(), None);
    }

    #[test]
    fn absent_key_yields_none() {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
        )
        .unwrap();
        assert_eq!(get_general_mount_point_id(&c).unwrap(), None);
    }

    #[test]
    fn present_key_returns_value() {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);\
             INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('generalMountPointId', 'mp-general-1');",
        )
        .unwrap();
        assert_eq!(
            get_general_mount_point_id(&c).unwrap(),
            Some("mp-general-1".to_string())
        );
    }

    fn table() -> Connection {
        let c = conn();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
        )
        .unwrap();
        c
    }

    #[test]
    fn max_concurrent_jobs_default_and_clamp() {
        // Missing table → default (v4 readSetting try/catch → null → default).
        assert_eq!(get_max_concurrent_jobs(&conn()).unwrap(), 4);
        // Unset key → default.
        let c = table();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 4);
        // In-range passes through.
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "8").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 8);
        // Above 32 → clamp to 32.
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "100").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 32);
        // < 1 → default (v4 rejects `< 1` before the clamp).
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "0").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 4);
        // Non-numeric → default.
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "banana").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 4);
        // Floored (v4 Math.floor).
        write_setting(&c, KEY_MAX_CONCURRENT_JOBS, "5.9").unwrap();
        assert_eq!(get_max_concurrent_jobs(&c).unwrap(), 5);
    }

    #[test]
    fn maintenance_sweep_at_roundtrip() {
        let c = table();
        assert_eq!(get_last_maintenance_sweep_at(&c).unwrap(), None);
        set_last_maintenance_sweep_at(&c, "2026-07-06T12:00:00.000Z").unwrap();
        assert_eq!(
            get_last_maintenance_sweep_at(&c).unwrap(),
            crate::clock::iso_to_ms("2026-07-06T12:00:00.000Z")
        );
    }
}

// ---------------------------------------------------------------------------
// P4.1b appends (append-only region — lane b; lane d also appends below).
// ---------------------------------------------------------------------------

/// The `instance_settings` key that stores the singleton "Quilltap Uploads"
/// mount-point id (v4 `KEY_USER_UPLOADS_MOUNT_POINT_ID`), provisioned by
/// `provision-user-uploads-mount-v1`. Home for project-less file uploads,
/// image pastes, capabilities reports, and restored project-less backup files.
const KEY_USER_UPLOADS_MOUNT_POINT_ID: &str = "userUploadsMountPointId";

/// v4 `getUserUploadsMountPointId()` — the Quilltap Uploads mount-point id, or
/// `None` when the store has not been provisioned (or the table is absent).
pub fn get_user_uploads_mount_point_id(main: &Connection) -> Result<Option<String>, DbError> {
    Ok(read_setting(main, KEY_USER_UPLOADS_MOUNT_POINT_ID))
}

// ---------------------------------------------------------------------------
// P4.4u3 appends (lane d) — the mount-point-pointer setters. The built-in mount
// provisioner ([`crate::services::builtin_mounts`]) writes each store's id here
// after minting it (v4's `INSERT ... ON CONFLICT(key) DO UPDATE`).
// ---------------------------------------------------------------------------

/// v4 `NON_PORTABLE_INSTANCE_SETTING_KEYS` (`lib/instance-settings/index.ts:62-68`,
/// `7189a968`) — settings that must never leave the instance that wrote them.
/// Kept beside the key constants so adding a setting is a conscious
/// include/exclude decision: the `instance-settings` export type dumps the
/// whole table minus this set, so a new key is portable by default.
///
///  - The three mount-point pointers are UUIDs into *this* instance's
///    mount-index database. Carrying them over would aim the Lantern, uploads,
///    and general stores at mount points that don't exist on the receiver.
///  - `lastMaintenanceSweepAt` is local timing state; importing it would make
///    the receiving instance skip a sweep it never ran.
///  - `highest_app_version` is the version guard's downgrade tripwire. An
///    imported value could lock a healthy instance out of its own database.
pub const NON_PORTABLE_INSTANCE_SETTING_KEYS: &[&str] = &[
    KEY_LANTERN_BACKGROUNDS_MOUNT_POINT_ID,
    KEY_USER_UPLOADS_MOUNT_POINT_ID,
    KEY_GENERAL_MOUNT_POINT_ID,
    KEY_LAST_MAINTENANCE_SWEEP_AT,
    "highest_app_version",
];

/// v4 `listPortableInstanceSettings()` — every `instance_settings` row
/// `ORDER BY key`, minus [`NON_PORTABLE_INSTANCE_SETTING_KEYS`]. Values ride
/// verbatim (the whole point of "move my setup" is that a setting travels
/// without a typed helper). A read failure — including a missing table on a
/// pre-provisioning instance — answers `[]`, v4's catch arm.
pub fn list_portable_instance_settings(
    main: &Connection,
) -> Result<Vec<(String, String)>, DbError> {
    let mut stmt = match main
        .prepare("SELECT \"key\", \"value\" FROM \"instance_settings\" ORDER BY \"key\"")
    {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };
    let rows = match stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for row in rows.flatten() {
        if !NON_PORTABLE_INSTANCE_SETTING_KEYS.contains(&row.0.as_str()) {
            out.push(row);
        }
    }
    Ok(out)
}

/// v4 `writeInstanceSetting(key, value)` — upsert one raw row. Exposed for the
/// `instance-settings` importer, which writes values it cannot interpret.
/// Prefer the typed setters everywhere else.
pub fn write_instance_setting(main: &Connection, key: &str, value: &str) -> Result<(), DbError> {
    write_setting(main, key, value)
}

/// v4 `setGeneralMountPointId` — persist the Quilltap General store's id.
pub fn set_general_mount_point_id(main: &Connection, id: &str) -> Result<(), DbError> {
    write_setting(main, KEY_GENERAL_MOUNT_POINT_ID, id)
}

/// v4 `setUserUploadsMountPointId` — persist the Quilltap Uploads store's id.
pub fn set_user_uploads_mount_point_id(main: &Connection, id: &str) -> Result<(), DbError> {
    write_setting(main, KEY_USER_UPLOADS_MOUNT_POINT_ID, id)
}

/// v4 `setLanternBackgroundsMountPointId` — persist the Lantern Backgrounds id.
pub fn set_lantern_backgrounds_mount_point_id(main: &Connection, id: &str) -> Result<(), DbError> {
    write_setting(main, KEY_LANTERN_BACKGROUNDS_MOUNT_POINT_ID, id)
}

#[cfg(test)]
mod p4d50_taboo_tests {
    //! Mirrors v4's `__tests__/unit/lib/instance-settings/taboo.test.ts`
    //! case-for-case (`7df7de8e`).

    use super::*;

    fn store() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
        )
        .unwrap();
        c
    }

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // ---- getTabooSettings ----

    #[test]
    fn get_returns_empty_when_unset() {
        assert_eq!(get_taboo_settings(&store()).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn get_returns_the_stored_list_in_stored_order() {
        let c = store();
        write_setting(
            &c,
            KEY_TABOO,
            r#"{"phrases":["weight-bearing","that's not nothing"]}"#,
        )
        .unwrap();
        assert_eq!(
            get_taboo_settings(&c).unwrap(),
            v(&["weight-bearing", "that's not nothing"])
        );
    }

    #[test]
    fn get_falls_back_on_unparseable_json() {
        let c = store();
        write_setting(&c, KEY_TABOO, "not json").unwrap();
        assert_eq!(get_taboo_settings(&c).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn get_falls_back_when_the_stored_shape_is_wrong() {
        let c = store();
        write_setting(&c, KEY_TABOO, r#"{"phrases":"nope"}"#).unwrap();
        assert_eq!(get_taboo_settings(&c).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn get_falls_back_when_a_stored_phrase_is_out_of_range() {
        let c = store();
        let long = "x".repeat(201);
        write_setting(
            &c,
            KEY_TABOO,
            &serde_json::json!({ "phrases": [long] }).to_string(),
        )
        .unwrap();
        assert_eq!(get_taboo_settings(&c).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn get_falls_back_when_the_table_is_missing() {
        // v4's `readSetting` catch arm — the read itself resolves to null.
        let c = Connection::open_in_memory().unwrap();
        assert_eq!(get_taboo_settings(&c).unwrap(), Vec::<String>::new());
    }

    // ---- normalizeTabooPhrases ----

    #[test]
    fn normalize_trims_each_entry() {
        assert_eq!(
            normalize_taboo_phrases(&v(&["  weight-bearing  "])),
            v(&["weight-bearing"])
        );
    }

    #[test]
    fn normalize_drops_entries_that_trim_away_to_nothing() {
        assert_eq!(
            normalize_taboo_phrases(&v(&["", "   ", "\t\n", "kept"])),
            v(&["kept"])
        );
    }

    #[test]
    fn normalize_drops_case_insensitive_duplicates_keeping_the_first() {
        assert_eq!(
            normalize_taboo_phrases(&v(&["Weight-Bearing", "weight-bearing", "WEIGHT-BEARING"])),
            v(&["Weight-Bearing"])
        );
    }

    #[test]
    fn normalize_treats_a_whitespace_only_difference_as_a_duplicate() {
        assert_eq!(
            normalize_taboo_phrases(&v(&["tapestry", "  tapestry  "])),
            v(&["tapestry"])
        );
    }

    #[test]
    fn normalize_preserves_user_order_rather_than_sorting() {
        assert_eq!(
            normalize_taboo_phrases(&v(&["zeta", "alpha", "mu"])),
            v(&["zeta", "alpha", "mu"])
        );
    }

    #[test]
    fn normalize_returns_empty_for_empty_input() {
        assert_eq!(normalize_taboo_phrases(&[]), Vec::<String>::new());
    }

    // ---- setTabooSettings ----

    #[test]
    fn set_round_trips_through_get() {
        let c = store();
        set_taboo_settings(&c, &v(&["that's not nothing", "weight-bearing"])).unwrap();
        assert_eq!(
            get_taboo_settings(&c).unwrap(),
            v(&["that's not nothing", "weight-bearing"])
        );
    }

    #[test]
    fn set_normalizes_on_write_and_returns_exactly_what_was_stored() {
        let c = store();
        let saved = set_taboo_settings(
            &c,
            &v(&[
                "  weight-bearing ",
                "",
                "WEIGHT-BEARING",
                "that's not nothing",
            ]),
        )
        .unwrap()
        .unwrap();
        assert_eq!(saved, v(&["weight-bearing", "that's not nothing"]));
        assert_eq!(get_taboo_settings(&c).unwrap(), saved);
    }

    #[test]
    fn set_stores_an_empty_list_without_complaint() {
        let c = store();
        assert_eq!(
            set_taboo_settings(&c, &[]).unwrap(),
            Some(Vec::<String>::new())
        );
        assert_eq!(get_taboo_settings(&c).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn set_rejects_a_phrase_longer_than_200_without_writing() {
        let c = store();
        assert_eq!(
            set_taboo_settings(&c, &v(&[&"x".repeat(201)])).unwrap(),
            None
        );
        assert_eq!(read_setting(&c, KEY_TABOO), None);
    }

    #[test]
    fn set_rejects_more_than_500_phrases_without_writing() {
        let c = store();
        let too_many: Vec<String> = (0..501).map(|i| format!("phrase {i}")).collect();
        assert_eq!(set_taboo_settings(&c, &too_many).unwrap(), None);
        assert_eq!(read_setting(&c, KEY_TABOO), None);
    }

    #[test]
    fn set_accepts_exactly_500_phrases_and_a_200_character_phrase() {
        let c = store();
        let max_length = "x".repeat(200);
        let mut at_cap = vec![max_length.clone()];
        at_cap.extend((0..499).map(|i| format!("phrase {i}")));
        let saved = set_taboo_settings(&c, &at_cap).unwrap().unwrap();
        assert_eq!(saved.len(), 500);
        assert_eq!(saved[0], max_length);
    }

    // ---- the schema's check ORDER (v4's route arms depend on it) ----

    #[test]
    fn parse_trims_before_measuring_length() {
        // 201 raw units that trim to 200 — valid, and the stored value is the
        // trimmed one. (Zod runs `.trim()` before `.min`/`.max`.)
        let raw = format!("  {}  ", "x".repeat(200));
        let parsed = parse_taboo_settings(&serde_json::json!({ "phrases": [raw] })).expect("valid");
        assert_eq!(parsed, vec!["x".repeat(200)]);
        // …and a whitespace-only entry is REJECTED (trims to 0, failing `.min(1)`)
        // rather than dropped.
        assert_eq!(
            parse_taboo_settings(&serde_json::json!({ "phrases": ["   "] })),
            None
        );
    }

    #[test]
    fn parse_measures_length_in_utf16_code_units() {
        // 101 astral characters = 202 UTF-16 units — JS `String.length` rejects.
        let astral = "🎩".repeat(101);
        assert_eq!(
            parse_taboo_settings(&serde_json::json!({ "phrases": [astral] })),
            None
        );
        let ok = "🎩".repeat(100);
        assert!(parse_taboo_settings(&serde_json::json!({ "phrases": [ok] })).is_some());
    }

    #[test]
    fn parse_defaults_an_absent_phrases_key_and_strips_unknown_ones() {
        assert_eq!(
            parse_taboo_settings(&serde_json::json!({})),
            Some(Vec::new())
        );
        assert_eq!(
            parse_taboo_settings(&serde_json::json!({ "nope": 1 })),
            Some(Vec::new())
        );
        // …but a non-object and a non-string entry both throw.
        assert_eq!(parse_taboo_settings(&serde_json::json!("nope")), None);
        assert_eq!(
            parse_taboo_settings(&serde_json::json!({ "phrases": [42] })),
            None
        );
        assert_eq!(
            parse_taboo_settings(&serde_json::json!({ "phrases": null })),
            None
        );
    }

    #[test]
    fn taboo_is_portable_by_default() {
        // v4 leaves `taboo` OUT of `NON_PORTABLE_INSTANCE_SETTING_KEYS`, so it
        // exports with `.qtap` instance-settings and rides full backups.
        assert!(!NON_PORTABLE_INSTANCE_SETTING_KEYS.contains(&KEY_TABOO));
        let c = store();
        set_taboo_settings(&c, &v(&["weight-bearing"])).unwrap();
        let portable = list_portable_instance_settings(&c).unwrap();
        assert!(portable
            .iter()
            .any(|(k, val)| k == KEY_TABOO && val == r#"{"phrases":["weight-bearing"]}"#));
    }
}

#[cfg(test)]
mod p4d57_brahma_console_tests {
    //! Mirrors v4's
    //! `__tests__/unit/lib/instance-settings/brahma-console.test.ts` (`6452e2c3`).

    use super::*;

    fn store() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
        )
        .unwrap();
        c
    }

    // ---- getBrahmaConsoleSettings ----

    #[test]
    fn get_returns_the_50_turn_default_when_unset() {
        assert_eq!(get_brahma_console_settings(&store()).unwrap(), 50);
    }

    #[test]
    fn get_returns_the_stored_value() {
        let c = store();
        write_setting(&c, KEY_BRAHMA_CONSOLE, r#"{"maxAgentTurns":120}"#).unwrap();
        assert_eq!(get_brahma_console_settings(&c).unwrap(), 120);
    }

    #[test]
    fn get_falls_back_on_unparseable_json() {
        let c = store();
        write_setting(&c, KEY_BRAHMA_CONSOLE, "not json").unwrap();
        assert_eq!(get_brahma_console_settings(&c).unwrap(), 50);
    }

    #[test]
    fn get_falls_back_on_out_of_range_values() {
        let c = store();
        write_setting(&c, KEY_BRAHMA_CONSOLE, r#"{"maxAgentTurns":1}"#).unwrap();
        assert_eq!(get_brahma_console_settings(&c).unwrap(), 50);
    }

    #[test]
    fn get_defaults_an_absent_key_via_the_schema_default() {
        // A present object OMITTING maxAgentTurns parses via Zod `.default(50)`.
        let c = store();
        write_setting(&c, KEY_BRAHMA_CONSOLE, r#"{}"#).unwrap();
        assert_eq!(get_brahma_console_settings(&c).unwrap(), 50);
    }

    #[test]
    fn get_falls_back_when_the_stored_shape_is_wrong() {
        let c = store();
        write_setting(&c, KEY_BRAHMA_CONSOLE, r#"42"#).unwrap();
        assert_eq!(get_brahma_console_settings(&c).unwrap(), 50);
    }

    #[test]
    fn get_falls_back_when_the_table_is_missing() {
        // v4's `readSetting` catch arm — the read itself resolves to null.
        let c = Connection::open_in_memory().unwrap();
        assert_eq!(get_brahma_console_settings(&c).unwrap(), 50);
    }

    // ---- setBrahmaConsoleSettings ----

    #[test]
    fn set_round_trips_through_get() {
        let c = store();
        assert_eq!(set_brahma_console_settings(&c, 75).unwrap(), Some(75));
        assert_eq!(get_brahma_console_settings(&c).unwrap(), 75);
    }

    #[test]
    fn set_accepts_the_boundary_values() {
        let c = store();
        assert_eq!(set_brahma_console_settings(&c, 5).unwrap(), Some(5));
        assert_eq!(set_brahma_console_settings(&c, 200).unwrap(), Some(200));
    }

    #[test]
    fn set_rejects_out_of_range_values_without_writing() {
        let c = store();
        assert_eq!(set_brahma_console_settings(&c, 4).unwrap(), None);
        assert_eq!(set_brahma_console_settings(&c, 201).unwrap(), None);
        // v4's `expect(mockRawQuery).not.toHaveBeenCalled()` — nothing stored.
        assert_eq!(read_setting(&c, KEY_BRAHMA_CONSOLE), None);
    }

    // ---- the schema's parse (v4's route arms depend on it) ----

    #[test]
    fn parse_defaults_an_absent_key_and_rejects_bad_shapes() {
        assert_eq!(
            parse_brahma_console_settings(&serde_json::json!({})),
            Some(50)
        );
        // A non-object throws; `null` / a non-integer / out-of-range all fail.
        assert_eq!(parse_brahma_console_settings(&serde_json::json!(50)), None);
        assert_eq!(
            parse_brahma_console_settings(&serde_json::json!({ "maxAgentTurns": null })),
            None
        );
        assert_eq!(
            parse_brahma_console_settings(&serde_json::json!({ "maxAgentTurns": 12.5 })),
            None
        );
        assert_eq!(
            parse_brahma_console_settings(&serde_json::json!({ "maxAgentTurns": "fifty" })),
            None
        );
        assert_eq!(
            parse_brahma_console_settings(&serde_json::json!({ "maxAgentTurns": 4 })),
            None
        );
        assert_eq!(
            parse_brahma_console_settings(&serde_json::json!({ "maxAgentTurns": 201 })),
            None
        );
        // Boundaries are inclusive.
        assert_eq!(
            parse_brahma_console_settings(&serde_json::json!({ "maxAgentTurns": 5 })),
            Some(5)
        );
        assert_eq!(
            parse_brahma_console_settings(&serde_json::json!({ "maxAgentTurns": 200 })),
            Some(200)
        );
    }

    #[test]
    fn brahma_console_is_portable_by_default() {
        // v4 leaves `brahmaConsole` OUT of `NON_PORTABLE_INSTANCE_SETTING_KEYS`,
        // so the setting exports with `.qtap` and rides full backups (Tier 2).
        assert!(!NON_PORTABLE_INSTANCE_SETTING_KEYS.contains(&KEY_BRAHMA_CONSOLE));
        let c = store();
        set_brahma_console_settings(&c, 80).unwrap();
        let portable = list_portable_instance_settings(&c).unwrap();
        assert!(portable
            .iter()
            .any(|(k, val)| k == KEY_BRAHMA_CONSOLE && val == r#"{"maxAgentTurns":80}"#));
    }
}

#[cfg(test)]
mod p41b_tests {
    use super::*;

    #[test]
    fn user_uploads_mount_point_id_reads() {
        let c = Connection::open_in_memory().unwrap();
        assert_eq!(get_user_uploads_mount_point_id(&c).unwrap(), None);
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);\
             INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('userUploadsMountPointId', 'mp-uploads-1');",
        )
        .unwrap();
        assert_eq!(
            get_user_uploads_mount_point_id(&c).unwrap(),
            Some("mp-uploads-1".to_string())
        );
    }
}
