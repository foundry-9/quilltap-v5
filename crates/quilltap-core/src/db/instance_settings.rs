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
//! migration writes the key on first boot), logging the WARN `[InstanceSettings]
//! Failed to read setting` `{ key, error }` as it does; a missing row or a NULL
//! `value` is `null` with no line at all. We reproduce that (P4.113 — before it,
//! v5 swallowed every error silently while this paragraph claimed the warning).

use rusqlite::{params, Connection};

use super::DbError;
use crate::api::zod_issues::ZodIssue;

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
/// stale-chat window that governs the daily maintenance sweep's cache collapse
/// and image collapse; conversation embeddings are always kept warm regardless
/// of it — v4 `f7f3d7bf0`).
const KEY_DATA_RETENTION: &str = "dataRetention";

/// v4 `DEFAULT_DATA_RETENTION_SETTINGS.staleChatDays` — the documented default.
pub const DEFAULT_STALE_CHAT_DAYS: i64 = 30;

/// v4 `DataRetentionSettingsSchema` bound: `z.number().int().min(1).max(3650)`.
pub const STALE_CHAT_DAYS_MIN: i64 = 1;
/// v4 `DataRetentionSettingsSchema` bound: `z.number().int().min(1).max(3650)`.
pub const STALE_CHAT_DAYS_MAX: i64 = 3650;

/// v4 `readSetting(key)` — read one `instance_settings` value, or `None`.
///
/// Faithful to v4 (`lib/instance-settings/index.ts:100-114`): `rows[0]?.value
/// ?? null`, so a missing row (`QueryReturnedNoRows`) and a NULL `value` (read
/// as `Option`, so it never becomes a type error) are `None` SILENTLY; EVERY
/// other failure — a missing table first among them — is `None` with v4's
/// WARN `[InstanceSettings] Failed to read setting` `{ key, error }` (P4.113;
/// `.ok()` had swallowed all three alike). v4's backend ALSO logs ERROR `Raw
/// query failed` `{ query, error }` before rethrowing
/// (`backends/sqlite/backend.ts:861-886`); v5 has no `rawQuery` layer, so that
/// line is v4-only — recorded and pinned both ways in
/// `scenario_builder_mount_pool_equivalence`, not ported.
fn read_setting(main: &Connection, key: &str) -> Option<String> {
    match main.query_row(
        "SELECT \"value\" FROM \"instance_settings\" WHERE \"key\" = ?1",
        params![key],
        |row| row.get::<_, Option<String>>(0),
    ) {
        Ok(value) => value,
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(err) => {
            tracing::warn!(key, error = %err, "[InstanceSettings] Failed to read setting");
            None
        }
    }
}

/// v4 `readJsonSetting(key, schema, defaults)` (`index.ts:130-145`) — the ONE
/// home of the JSON-setting read for its five getters (P4.113): an unwritten
/// row is `defaults` SILENTLY; a stored value that fails `JSON.parse` or the
/// schema (`parse` answering `None` — each getter's Zod twin) is `defaults`
/// with v4's WARN `` `[InstanceSettings] ${key} failed to parse — using
/// defaults` `` `{ error }`. v4's `error` carries V8's `JSON.parse` wording or a
/// `ZodError`'s issue JSON; v5's carries serde's wording or a schema sentence —
/// the FIELD is v4's, the text is not transcribed (compared by presence in
/// `instance_settings_json_warns_equivalence`). Before this, three getters fell
/// back silently and two warned without the `error` field.
fn read_json_setting<T>(
    main: &Connection,
    key: &str,
    parse: impl FnOnce(&serde_json::Value) -> Option<T>,
    defaults: impl FnOnce() -> T,
) -> T {
    let Some(raw) = read_setting(main, key) else {
        return defaults();
    };
    let parsed = serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|e| e.to_string())
        .and_then(|v| {
            parse(&v).ok_or_else(|| format!("the stored {key} value does not match its schema"))
        });
    match parsed {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "[InstanceSettings] {} failed to parse — using defaults",
                key
            );
            defaults()
        }
    }
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
/// `readJsonSetting`: `schema.parse` → `catch` → WARN + default, P4.113 — a
/// non-object value fails too, where v5 had read its absent keys as defaults). The Zod schema has `scopePolicy` enum
/// `['down-weight','exclude']` (`.default('down-weight')`), `expandRelated`
/// boolean (`.default(false)`) and `perTurnConversationSummaries` boolean
/// (`.default(false)`); an out-of-enum / non-bool value fails the parse (a
/// `.default`-carrying key means a bad *value* still fails, not defaults —
/// faithful to `.parse` throwing on a present-but-wrong value).
pub fn get_memory_recall_settings(main: &Connection) -> Result<MemoryRecallSettings, DbError> {
    Ok(read_json_setting(
        main,
        KEY_MEMORY_RECALL,
        parse_memory_recall_settings,
        MemoryRecallSettings::default,
    ))
}

/// `MemoryRecallSettingsSchema.parse` — `None` is the throw (a non-object —
/// `null`, an array, a scalar — or a present field of the wrong shape).
fn parse_memory_recall_settings(value: &serde_json::Value) -> Option<MemoryRecallSettings> {
    let obj = value.as_object()?; // `z.object` throws on a non-object
                                  // scopePolicy: enum, `.default('down-weight')` (absent → default; present but
                                  // out-of-enum → parse fails → whole-object default).
    let scope_policy = match obj.get("scopePolicy") {
        None => MemoryRecallSettings::default().scope_policy,
        Some(serde_json::Value::String(s)) if s == "down-weight" || s == "exclude" => s.clone(),
        Some(_) => return None,
    };
    // expandRelated: boolean, `.default(false)`.
    let expand_related = match obj.get("expandRelated") {
        None => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(_) => return None,
    };
    // perTurnConversationSummaries: boolean, `.default(false)` — same arm shape.
    let per_turn_conversation_summaries = match obj.get("perTurnConversationSummaries") {
        None => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(_) => return None,
    };
    Some(MemoryRecallSettings {
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
/// integer) v4 logs a warning and falls back to the default — reproduced here
/// since P4.113 (this doc claimed it before the code did).
/// A stored object that OMITS `staleChatDays` parses via Zod `.default(30)` → 30.
pub fn get_data_retention_settings(main: &Connection) -> Result<i64, DbError> {
    Ok(read_json_setting(
        main,
        KEY_DATA_RETENTION,
        parse_data_retention_settings,
        || DEFAULT_STALE_CHAT_DAYS,
    ))
}

/// `DataRetentionSettingsSchema.parse` — `None` is the throw.
fn parse_data_retention_settings(value: &serde_json::Value) -> Option<i64> {
    let obj = value.as_object()?; // `z.object` throws on a non-object
    match obj.get("staleChatDays") {
        // Zod `.default(30)` — a present object with the key absent.
        None => Some(DEFAULT_STALE_CHAT_DAYS),
        Some(v) => validate_stale_chat_days(v),
    }
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
    Ok(read_json_setting(
        main,
        KEY_TABOO,
        parse_taboo_settings,
        Vec::new,
    ))
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
    Ok(read_json_setting(
        main,
        KEY_BRAHMA_CONSOLE,
        parse_brahma_console_settings,
        || DEFAULT_BRAHMA_MAX_AGENT_TURNS,
    ))
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
/// softFloor}` object; documented defaults `{false, 20, 0.7, 0.7}` when unset,
/// and — with v4's WARN — when the stored value fails `MemoryExtractionLimitsSchema`
/// (P4.113: v5 had returned ANY stored object verbatim — unknown keys kept,
/// absent keys missing, out-of-range values passed through — and fell back
/// silently only on bad JSON or a non-object).
pub fn get_memory_extraction_limits(main: &Connection) -> Result<serde_json::Value, DbError> {
    Ok(read_json_setting(
        main,
        KEY_MEMORY_EXTRACTION_LIMITS,
        parse_memory_extraction_limits,
        || parse_memory_extraction_limits(&serde_json::json!({})).expect("the schema defaults"),
    ))
}

/// `MemoryExtractionLimitsSchema.parse` (`lib/schemas/settings.types.ts:218`) —
/// `None` is the throw. The OUTPUT is Zod's: the four keys in schema order,
/// every absent one `.default`ed, unknown keys stripped, numbers rendered as JS
/// renders them (`20.0` → `20`).
///
/// ```text
/// enabled:           z.boolean().default(false)
/// maxPerHour:        z.number().int().positive().max(10_000).default(20)
/// softStartFraction: z.number().min(0).max(1).default(0.7)
/// softFloor:         z.number().min(0).max(1).default(0.7)
/// ```
fn parse_memory_extraction_limits(value: &serde_json::Value) -> Option<serde_json::Value> {
    // P4.D288: ONE parse — the typed one with Zod's issues (v4 `c2795733f`'s
    // `maxPerHour` `.max(10_000)` included).
    parse_memory_extraction_limits_settings(value)
        .ok()
        .map(|limits| limits.to_json())
}

/// P4.D288 (C1-5): [`get_memory_extraction_limits`] typed — the same read,
/// the same WARN + defaults.
pub fn get_memory_extraction_limits_settings(main: &Connection) -> MemoryExtractionLimits {
    read_json_setting(
        main,
        KEY_MEMORY_EXTRACTION_LIMITS,
        |v| parse_memory_extraction_limits_settings(v).ok(),
        MemoryExtractionLimits::default,
    )
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

// ============================================================================
// P4.D265 (v4 `d58548051`) — the two memory-programme settings:
// `memoryConsolidation` and `memoryExtractionMode`
// (`lib/schemas/settings.types.ts:219-262`, `lib/instance-settings/index.ts`).
// The accessors and the Zod twin only; what the settings DO is P4.D270's /
// P4.D271's, and the routes' `.partial()` semantics (v4's clobber, §R.4(c))
// are P4.D271's.
// ============================================================================

/// v4 `KEY_MEMORY_CONSOLIDATION` (`index.ts:43`).
pub const KEY_MEMORY_CONSOLIDATION: &str = "memoryConsolidation";
/// v4 `KEY_MEMORY_EXTRACTION_MODE` (`index.ts:44`).
pub const KEY_MEMORY_EXTRACTION_MODE: &str = "memoryExtractionMode";

/// v4 `MemoryExtractionModeSettingsSchema.otherPass`'s enum, in schema order.
pub const MEMORY_EXTRACTION_OTHER_PASS: [&str; 3] = ["turn", "fold", "hybrid"];

/// v4 `MemoryConsolidationSettings` — the consolidation job's controls.
/// Declaration order = wire order = v4's schema key order.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryConsolidationSettings {
    /// Master switch for the scheduled sweep and the watermark trigger.
    pub enabled: bool,
    /// Connection profile for the consolidation call; `None` → the cheap LLM.
    pub connection_profile_id: Option<String>,
    /// Cosine similarity at which two memories join a cluster (`0..=1`).
    pub cluster_threshold: f64,
    /// P4.D288 (v4 `4ae0c4ca6`): the same, when the default embedding profile
    /// is the Built-in (TF-IDF) one, whose cosines run far lower (`0..=1`,
    /// default 0.30).
    pub tfidf_cluster_threshold: f64,
    /// Smallest cluster worth a digest (int `>= 2`).
    pub min_cluster_size: i64,
    /// Largest cluster handed to one call (int `>= 2`).
    pub max_cluster_size: i64,
    /// Rows younger than this many days are left alone (`>= 0`).
    pub mature_after_days: f64,
    /// Upper bound on consolidation calls per run (int `> 0`).
    pub max_clusters_per_run: i64,
    /// Enqueue a run once this many hot rows were never considered (int `> 0`).
    pub watermark: i64,
    /// Delete superseded cold non-MANUAL rows older than this many days; `None`
    /// → never delete (int `> 0` or null).
    pub cold_retention_days: Option<i64>,
}

impl Default for MemoryConsolidationSettings {
    /// v4 `MemoryConsolidationSettingsSchema.parse({})` (`index.ts:96-97`).
    fn default() -> Self {
        Self {
            enabled: false,
            connection_profile_id: None,
            cluster_threshold: 0.72,
            tfidf_cluster_threshold: 0.30,
            min_cluster_size: 3,
            max_cluster_size: 30,
            mature_after_days: 7.0,
            max_clusters_per_run: 40,
            watermark: 150,
            cold_retention_days: None,
        }
    }
}

impl MemoryConsolidationSettings {
    /// v4's stored / returned JSON (`JSON.stringify(validated)`): declaration
    /// order, `null` for the two nullables, numbers JS-rendered.
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert("enabled".into(), serde_json::Value::Bool(self.enabled));
        m.insert(
            "connectionProfileId".into(),
            self.connection_profile_id
                .clone()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        m.insert(
            "clusterThreshold".into(),
            super::js_number_to_json(self.cluster_threshold),
        );
        m.insert(
            "tfidfClusterThreshold".into(),
            super::js_number_to_json(self.tfidf_cluster_threshold),
        );
        m.insert("minClusterSize".into(), self.min_cluster_size.into());
        m.insert("maxClusterSize".into(), self.max_cluster_size.into());
        m.insert(
            "matureAfterDays".into(),
            super::js_number_to_json(self.mature_after_days),
        );
        m.insert("maxClustersPerRun".into(), self.max_clusters_per_run.into());
        m.insert("watermark".into(), self.watermark.into());
        m.insert(
            "coldRetentionDays".into(),
            self.cold_retention_days
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null),
        );
        serde_json::Value::Object(m)
    }
}

/// The `.partial()` input of [`MemoryConsolidationSettings`] — every field
/// optional, the two nullables `Option<Option<…>>` (`Some(None)` = an explicit
/// `null`). What it MEANS when merged is P4.D271's (v4's clobber).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryConsolidationSettingsInput {
    pub enabled: Option<bool>,
    pub connection_profile_id: Option<Option<String>>,
    pub cluster_threshold: Option<f64>,
    pub tfidf_cluster_threshold: Option<f64>,
    pub min_cluster_size: Option<i64>,
    pub max_cluster_size: Option<i64>,
    pub mature_after_days: Option<f64>,
    pub max_clusters_per_run: Option<i64>,
    pub watermark: Option<i64>,
    pub cold_retention_days: Option<Option<i64>>,
}

impl MemoryConsolidationSettingsInput {
    /// The present keys, in declaration order — a patch for
    /// [`set_memory_consolidation_settings`].
    pub fn to_patch_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        if let Some(v) = self.enabled {
            m.insert("enabled".into(), v.into());
        }
        if let Some(v) = &self.connection_profile_id {
            m.insert("connectionProfileId".into(), v.clone().into());
        }
        if let Some(v) = self.cluster_threshold {
            m.insert("clusterThreshold".into(), super::js_number_to_json(v));
        }
        if let Some(v) = self.tfidf_cluster_threshold {
            m.insert("tfidfClusterThreshold".into(), super::js_number_to_json(v));
        }
        if let Some(v) = self.min_cluster_size {
            m.insert("minClusterSize".into(), v.into());
        }
        if let Some(v) = self.max_cluster_size {
            m.insert("maxClusterSize".into(), v.into());
        }
        if let Some(v) = self.mature_after_days {
            m.insert("matureAfterDays".into(), super::js_number_to_json(v));
        }
        if let Some(v) = self.max_clusters_per_run {
            m.insert("maxClustersPerRun".into(), v.into());
        }
        if let Some(v) = self.watermark {
            m.insert("watermark".into(), v.into());
        }
        if let Some(v) = self.cold_retention_days {
            m.insert("coldRetentionDays".into(), v.into());
        }
        serde_json::Value::Object(m)
    }
}

/// v4 `MemoryExtractionModeSettings` — the grain of the extractor's OTHER pass.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryExtractionModeSettings {
    /// `'turn'` | `'fold'` | `'hybrid'`.
    pub other_pass: String,
    /// Per-turn OTHER candidates below this importance are dropped in
    /// `'hybrid'` (`0..=1`).
    pub per_turn_other_floor: f64,
    /// Fold-grain OTHER candidates kept per (observer, subject) (int `> 0`).
    pub fold_candidates_per_subject: i64,
}

impl Default for MemoryExtractionModeSettings {
    /// v4 `MemoryExtractionModeSettingsSchema.parse({})` (`index.ts:98-99`).
    fn default() -> Self {
        Self {
            other_pass: "hybrid".to_string(),
            per_turn_other_floor: 0.75,
            fold_candidates_per_subject: 3,
        }
    }
}

impl MemoryExtractionModeSettings {
    /// v4's stored / returned JSON, declaration order.
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert("otherPass".into(), self.other_pass.clone().into());
        m.insert(
            "perTurnOtherFloor".into(),
            super::js_number_to_json(self.per_turn_other_floor),
        );
        m.insert(
            "foldCandidatesPerSubject".into(),
            self.fold_candidates_per_subject.into(),
        );
        serde_json::Value::Object(m)
    }
}

/// The `.partial()` input of [`MemoryExtractionModeSettings`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryExtractionModeSettingsInput {
    pub other_pass: Option<String>,
    pub per_turn_other_floor: Option<f64>,
    pub fold_candidates_per_subject: Option<i64>,
}

impl MemoryExtractionModeSettingsInput {
    /// The present keys, in declaration order.
    pub fn to_patch_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        if let Some(v) = &self.other_pass {
            m.insert("otherPass".into(), v.clone().into());
        }
        if let Some(v) = self.per_turn_other_floor {
            m.insert("perTurnOtherFloor".into(), super::js_number_to_json(v));
        }
        if let Some(v) = self.fold_candidates_per_subject {
            m.insert("foldCandidatesPerSubject".into(), v.into());
        }
        serde_json::Value::Object(m)
    }
}

/// One Zod 4 number field check, issues pushed in Zod's order. `int` is
/// `.int()` (the integer gate ABORTS — `1.5` answers one issue; the
/// safe-integer bounds then the range checks CONTINUE); `min` is `(bound,
/// inclusive)` — `.positive()` is `(0, false)`; `max` is inclusive. `None`
/// answer → the field failed (absent is the caller's arm).
fn zod_number_field(
    got: &serde_json::Value,
    k: &str,
    int: bool,
    min: Option<(f64, bool)>,
    max: Option<f64>,
    issues: &mut Vec<ZodIssue>,
) -> Option<f64> {
    zod_number_at(
        got,
        vec![crate::api::zod_issues::key(k)],
        int,
        min,
        max,
        issues,
    )
}

/// [`zod_number_field`] at any `path` (P4.D288: a cap OVERRIDE sits at
/// `["perCharacterCapOverrides", <characterId>]`).
fn zod_number_at(
    got: &serde_json::Value,
    path: Vec<serde_json::Value>,
    int: bool,
    min: Option<(f64, bool)>,
    max: Option<f64>,
    issues: &mut Vec<ZodIssue>,
) -> Option<f64> {
    use crate::api::zod_issues::MAX_SAFE_INTEGER;
    let at = || path.clone();
    let Some(n) = got.as_f64() else {
        issues.push(ZodIssue::invalid_type("number", at(), Some(got)));
        return None;
    };
    let before = issues.len();
    if int {
        if n.fract() != 0.0 {
            issues.push(ZodIssue::invalid_int_type(at(), Some(got)));
            return None;
        }
        if n > MAX_SAFE_INTEGER {
            issues.push(ZodIssue::too_big_int(at()));
        } else if n < -MAX_SAFE_INTEGER {
            issues.push(ZodIssue::too_small_int(at()));
        }
    }
    if let Some((bound, inclusive)) = min {
        let fails = if inclusive { n < bound } else { n <= bound };
        if fails {
            let minimum = super::js_number_to_json(bound);
            issues.push(if inclusive {
                ZodIssue::too_small_number(minimum, at())
            } else {
                ZodIssue::TooSmall {
                    origin: "number",
                    code: "too_small",
                    message: format!("Too small: expected number to be >{minimum}"),
                    minimum,
                    inclusive: false,
                    path: at(),
                }
            });
        }
    }
    if let Some(bound) = max {
        if n > bound {
            issues.push(ZodIssue::too_big_number(
                super::js_number_to_json(bound),
                at(),
            ));
        }
    }
    (issues.len() == before).then_some(n)
}

/// v4 `MemoryConsolidationSettingsSchema.safeParse` — the parsed settings or
/// Zod's issue list in schema key order (every field checked; a non-object
/// root is ONE `invalid_type object` issue at `[]`). Every bound and byte is a
/// recorded row of `memory_settings_schemas_equivalence` (v4's REAL schema).
pub fn parse_memory_consolidation_settings(
    value: &serde_json::Value,
) -> Result<MemoryConsolidationSettings, Vec<ZodIssue>> {
    use crate::api::zod_issues::key;
    let Some(obj) = value.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(value))]);
    };
    let mut issues = Vec::new();
    let mut out = MemoryConsolidationSettings::default();
    match obj.get("enabled") {
        None => {}
        Some(serde_json::Value::Bool(b)) => out.enabled = *b,
        got => issues.push(ZodIssue::invalid_type("boolean", vec![key("enabled")], got)),
    }
    match obj.get("connectionProfileId") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(s)) => out.connection_profile_id = Some(s.clone()),
        got => issues.push(ZodIssue::invalid_type(
            "string",
            vec![key("connectionProfileId")],
            got,
        )),
    }
    if let Some(v) = obj.get("clusterThreshold") {
        if let Some(n) = zod_number_field(
            v,
            "clusterThreshold",
            false,
            Some((0.0, true)),
            Some(1.0),
            &mut issues,
        ) {
            out.cluster_threshold = n;
        }
    }
    if let Some(v) = obj.get("tfidfClusterThreshold") {
        if let Some(n) = zod_number_field(
            v,
            "tfidfClusterThreshold",
            false,
            Some((0.0, true)),
            Some(1.0),
            &mut issues,
        ) {
            out.tfidf_cluster_threshold = n;
        }
    }
    if let Some(v) = obj.get("minClusterSize") {
        if let Some(n) = zod_number_field(
            v,
            "minClusterSize",
            true,
            Some((2.0, true)),
            None,
            &mut issues,
        ) {
            out.min_cluster_size = n as i64;
        }
    }
    if let Some(v) = obj.get("maxClusterSize") {
        if let Some(n) = zod_number_field(
            v,
            "maxClusterSize",
            true,
            Some((2.0, true)),
            None,
            &mut issues,
        ) {
            out.max_cluster_size = n as i64;
        }
    }
    if let Some(v) = obj.get("matureAfterDays") {
        if let Some(n) = zod_number_field(
            v,
            "matureAfterDays",
            false,
            Some((0.0, true)),
            None,
            &mut issues,
        ) {
            out.mature_after_days = n;
        }
    }
    if let Some(v) = obj.get("maxClustersPerRun") {
        if let Some(n) = zod_number_field(
            v,
            "maxClustersPerRun",
            true,
            Some((0.0, false)),
            None,
            &mut issues,
        ) {
            out.max_clusters_per_run = n as i64;
        }
    }
    if let Some(v) = obj.get("watermark") {
        if let Some(n) =
            zod_number_field(v, "watermark", true, Some((0.0, false)), None, &mut issues)
        {
            out.watermark = n as i64;
        }
    }
    match obj.get("coldRetentionDays") {
        None | Some(serde_json::Value::Null) => {}
        Some(v) => {
            if let Some(n) = zod_number_field(
                v,
                "coldRetentionDays",
                true,
                Some((0.0, false)),
                None,
                &mut issues,
            ) {
                out.cold_retention_days = Some(n as i64);
            }
        }
    }
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

/// v4 `MemoryExtractionModeSettingsSchema.safeParse` — as
/// [`parse_memory_consolidation_settings`].
pub fn parse_memory_extraction_mode_settings(
    value: &serde_json::Value,
) -> Result<MemoryExtractionModeSettings, Vec<ZodIssue>> {
    use crate::api::zod_issues::key;
    let Some(obj) = value.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(value))]);
    };
    let mut issues = Vec::new();
    let mut out = MemoryExtractionModeSettings::default();
    match obj.get("otherPass") {
        None => {}
        Some(serde_json::Value::String(s))
            if MEMORY_EXTRACTION_OTHER_PASS.contains(&s.as_str()) =>
        {
            out.other_pass = s.clone()
        }
        Some(_) => issues.push(ZodIssue::invalid_value(
            &MEMORY_EXTRACTION_OTHER_PASS,
            vec![key("otherPass")],
        )),
    }
    if let Some(v) = obj.get("perTurnOtherFloor") {
        if let Some(n) = zod_number_field(
            v,
            "perTurnOtherFloor",
            false,
            Some((0.0, true)),
            Some(1.0),
            &mut issues,
        ) {
            out.per_turn_other_floor = n;
        }
    }
    if let Some(v) = obj.get("foldCandidatesPerSubject") {
        if let Some(n) = zod_number_field(
            v,
            "foldCandidatesPerSubject",
            true,
            Some((0.0, false)),
            None,
            &mut issues,
        ) {
            out.fold_candidates_per_subject = n as i64;
        }
    }
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

/// `{...current, ...patch}` over two JSON objects (a non-object patch spreads
/// no schema key — its index keys, if any, are stripped by the parse).
fn merge_patch(current: serde_json::Value, patch: &serde_json::Value) -> serde_json::Value {
    let mut merged = match current {
        serde_json::Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    if let Some(p) = patch.as_object() {
        for (k, v) in p {
            merged.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(merged)
}

/// v4 `getMemoryConsolidationSettings()` (`index.ts:245-251`): unwritten →
/// the defaults SILENTLY; a stored value that fails `JSON.parse` or the schema
/// → the defaults with v4's WARN `[InstanceSettings] memoryConsolidation failed
/// to parse — using defaults` `{error}` ([`read_json_setting`]).
pub fn get_memory_consolidation_settings(main: &Connection) -> MemoryConsolidationSettings {
    read_json_setting(
        main,
        KEY_MEMORY_CONSOLIDATION,
        |v| parse_memory_consolidation_settings(v).ok(),
        MemoryConsolidationSettings::default,
    )
}

/// v4 `setMemoryConsolidationSettings(value)` (`index.ts:253-261`): read the
/// current settings, merge `{...current, ...patch}`, validate
/// (`writeJsonSetting`'s `schema.parse` — a failure is `Err` carrying the
/// `ZodError.message` bytes, unreachable from v4's route, which pre-parses the
/// patch), store `JSON.stringify(validated)` in schema key order, answer the
/// validated settings.
pub fn set_memory_consolidation_settings(
    main: &Connection,
    patch: &serde_json::Value,
) -> Result<MemoryConsolidationSettings, DbError> {
    let merged = merge_patch(get_memory_consolidation_settings(main).to_json(), patch);
    let validated = parse_memory_consolidation_settings(&merged)
        .map_err(|issues| DbError::Internal(crate::api::zod_issues::zod_error_message(&issues)))?;
    write_setting(
        main,
        KEY_MEMORY_CONSOLIDATION,
        &validated.to_json().to_string(),
    )?;
    Ok(validated)
}

/// v4 `getMemoryExtractionModeSettings()` (`index.ts:267-273`) — as
/// [`get_memory_consolidation_settings`].
pub fn get_memory_extraction_mode_settings(main: &Connection) -> MemoryExtractionModeSettings {
    read_json_setting(
        main,
        KEY_MEMORY_EXTRACTION_MODE,
        |v| parse_memory_extraction_mode_settings(v).ok(),
        MemoryExtractionModeSettings::default,
    )
}

/// v4 `setMemoryExtractionModeSettings(value)` (`index.ts:275-283`) — as
/// [`set_memory_consolidation_settings`].
pub fn set_memory_extraction_mode_settings(
    main: &Connection,
    patch: &serde_json::Value,
) -> Result<MemoryExtractionModeSettings, DbError> {
    let merged = merge_patch(get_memory_extraction_mode_settings(main).to_json(), patch);
    let validated = parse_memory_extraction_mode_settings(&merged)
        .map_err(|issues| DbError::Internal(crate::api::zod_issues::zod_error_message(&issues)))?;
    write_setting(
        main,
        KEY_MEMORY_EXTRACTION_MODE,
        &validated.to_json().to_string(),
    )?;
    Ok(validated)
}

// === P4.D288 (C1-5) — the memory maintenance settings document, the typed
// extraction limits, the bug-244 PATCH parsers and the legacy translator (v4
// `e0ac7493a` / `4ae0c4ca6` / `104e986c7` at the `83334b9a5` pin:
// `lib/schemas/settings.types.ts:130-330`, `lib/schemas/patch-schema.ts`,
// `lib/memory/settings-legacy.ts`, `lib/instance-settings/index.ts:46,266-283`)
// (append-only) ===

/// v4 `KEY_MEMORY_MAINTENANCE` (`index.ts:46`).
pub const KEY_MEMORY_MAINTENANCE: &str = "memoryMaintenance";
/// v4 `MEMORY_MAINTENANCE_CAP_MIN` (`settings.types.ts:135`).
pub const MEMORY_MAINTENANCE_CAP_MIN: i64 = 100;
/// v4 `MEMORY_MAINTENANCE_CAP_MAX` (`settings.types.ts:136`).
pub const MEMORY_MAINTENANCE_CAP_MAX: i64 = 100_000;
/// v4 `MemoryMaintenanceSettingsSchema.parse({}).perCharacterCap` — 2000 (NOT
/// the 2,500 a Friday run once used; §R.4(e)).
pub const MEMORY_MAINTENANCE_DEFAULT_CAP: i64 = 2000;

/// v4 `MemoryMaintenancePhaseTogglesSchema` (`settings.types.ts:153-160`): the
/// maintenance job's own per-phase switches, each `.default(true)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryMaintenancePhaseToggles {
    /// Fold near-duplicate AUTO rows into one survivor (LLM-free).
    pub fold_near_duplicates: bool,
    /// Send the lowest-protection demotable rows cold when over the cap.
    pub demote: bool,
    /// Delete superseded cold rows past `coldRetentionDays`.
    pub retire: bool,
}

impl Default for MemoryMaintenancePhaseToggles {
    fn default() -> Self {
        Self {
            fold_near_duplicates: true,
            demote: true,
            retire: true,
        }
    }
}

impl MemoryMaintenancePhaseToggles {
    /// v4's JSON, schema key order.
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert(
            "foldNearDuplicates".into(),
            self.fold_near_duplicates.into(),
        );
        m.insert("demote".into(), self.demote.into());
        m.insert("retire".into(), self.retire.into());
        serde_json::Value::Object(m)
    }
}

/// v4 `MemoryMaintenanceSettings` (`settings.types.ts:170-185`), stored under
/// `instance_settings.memoryMaintenance` (moved there from the per-user
/// `chat_settings.autoHousekeepingSettings` by `move-memory-maintenance-
/// settings-v1`, `db::memory_maintenance_settings_move`). "Every default lives
/// here, once."
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryMaintenanceSettings {
    /// Maintenance runs automatically (post-write watermark + scheduled sweep).
    pub enabled: bool,
    /// Global per-character cap on demotable memories (int `100..=100000`).
    pub per_character_cap: i64,
    /// Per-character cap overrides keyed by characterId — an insertion-ordered
    /// map whose values are integer caps (`serde_json::Number`s). Its JSON
    /// emits V8's own-key order (O-11 — [`v8_own_key_order`]).
    pub per_character_cap_overrides: serde_json::Map<String, serde_json::Value>,
    /// Which of the job's own phases run when nobody asked for them by hand.
    pub phases: MemoryMaintenancePhaseToggles,
}

impl Default for MemoryMaintenanceSettings {
    /// v4 `MemoryMaintenanceSettingsSchema.parse({})`.
    fn default() -> Self {
        Self {
            enabled: false,
            per_character_cap: MEMORY_MAINTENANCE_DEFAULT_CAP,
            per_character_cap_overrides: serde_json::Map::new(),
            phases: MemoryMaintenancePhaseToggles::default(),
        }
    }
}

impl MemoryMaintenanceSettings {
    /// v4's stored / returned JSON (`JSON.stringify(validated)`): `enabled,
    /// perCharacterCap, perCharacterCapOverrides, phases{foldNearDuplicates,
    /// demote, retire}`; the overrides in V8's own-key order.
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert("enabled".into(), self.enabled.into());
        m.insert("perCharacterCap".into(), self.per_character_cap.into());
        m.insert(
            "perCharacterCapOverrides".into(),
            serde_json::Value::Object(v8_own_key_order(&self.per_character_cap_overrides)),
        );
        m.insert("phases".into(), self.phases.to_json());
        serde_json::Value::Object(m)
    }

    /// v4 `resolveMemoryCap(settings, characterId)` (`lib/memory/settings.ts:
    /// 64-73`) — the ONE per-character cap resolver: the override, else
    /// `perCharacterCap`.
    pub fn cap_for(&self, character_id: &str) -> i64 {
        self.per_character_cap_overrides
            .get(character_id)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(self.per_character_cap)
    }
}

/// V8's own-key order for a plain object (O-11): every canonical array-index
/// key (the decimal string of a `u32` below `2^32 − 1`, no leading zero) first
/// in ascending numeric order, then every other key in insertion order —
/// what `JSON.stringify` of a parsed object emits. Character ids are UUIDs, so
/// no real override moves; the corpus row with `"10"` / `"2"` pins the rule.
fn v8_own_key_order(
    map: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let index = |k: &str| -> Option<u32> {
        let n: u32 = k.parse().ok()?;
        (n != u32::MAX && k == n.to_string()).then_some(n)
    };
    let mut indexed: Vec<(u32, &String)> = map
        .keys()
        .filter_map(|k| index(k).map(|n| (n, k)))
        .collect();
    indexed.sort_by_key(|(n, _)| *n);
    let mut out = serde_json::Map::new();
    for (_, k) in indexed {
        out.insert(k.clone(), map[k].clone());
    }
    for (k, v) in map {
        if index(k).is_none() {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// `MemoryMaintenanceCapSchema` (`z.number().int().min(100).max(100000)`) at
/// `path`.
fn maintenance_cap_issues(
    got: &serde_json::Value,
    path: Vec<serde_json::Value>,
    issues: &mut Vec<ZodIssue>,
) -> Option<i64> {
    zod_number_at(
        got,
        path,
        true,
        Some((MEMORY_MAINTENANCE_CAP_MIN as f64, true)),
        Some(MEMORY_MAINTENANCE_CAP_MAX as f64),
        issues,
    )
    .map(|n| n as i64)
}

/// `MemoryMaintenanceCapOverridesSchema` (`z.record(z.string(), cap)`) at
/// `perCharacterCapOverrides`: a non-object is ONE `invalid_type` `expected
/// record` issue; each entry's value is checked in input order.
fn maintenance_overrides_issues(
    got: &serde_json::Value,
    issues: &mut Vec<ZodIssue>,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    use crate::api::zod_issues::key;
    let Some(obj) = got.as_object() else {
        issues.push(ZodIssue::invalid_type(
            "record",
            vec![key("perCharacterCapOverrides")],
            Some(got),
        ));
        return None;
    };
    let before = issues.len();
    let mut out = serde_json::Map::new();
    for (character_id, cap) in obj {
        if let Some(n) = maintenance_cap_issues(
            cap,
            vec![key("perCharacterCapOverrides"), key(character_id)],
            issues,
        ) {
            out.insert(character_id.clone(), n.into());
        }
    }
    (issues.len() == before).then_some(out)
}

/// The phase toggles at `phases`: a non-object is `invalid_type` `expected
/// object`; each present key must be a boolean. `defaults` decides whether an
/// ABSENT key reads as `true` (the settings schema's `.default(true)`) or
/// stays absent (the PATCH schema — `patchSchemaOf(Toggles)` strips the
/// defaults). Returns the present / defaulted keys, schema order.
fn phase_toggle_issues(
    got: &serde_json::Value,
    defaults: bool,
    issues: &mut Vec<ZodIssue>,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    use crate::api::zod_issues::key;
    let Some(obj) = got.as_object() else {
        issues.push(ZodIssue::invalid_type(
            "object",
            vec![key("phases")],
            Some(got),
        ));
        return None;
    };
    let before = issues.len();
    let mut out = serde_json::Map::new();
    for k in ["foldNearDuplicates", "demote", "retire"] {
        match obj.get(k) {
            None if defaults => {
                out.insert(k.into(), true.into());
            }
            None => {}
            Some(serde_json::Value::Bool(b)) => {
                out.insert(k.into(), (*b).into());
            }
            got => issues.push(ZodIssue::invalid_type(
                "boolean",
                vec![key("phases"), key(k)],
                got,
            )),
        }
    }
    (issues.len() == before).then_some(out)
}

/// v4 `MemoryMaintenanceSettingsSchema.safeParse` — the parsed settings or
/// Zod's issue list in schema key order. `phases` is `.prefault({})`: ABSENT
/// reads every phase ON (a value stored before the toggles existed); a present
/// `null` is an issue (a prefault, like a default, replaces `undefined` only).
/// Unknown keys (the retired merge knobs) are stripped.
pub fn parse_memory_maintenance_settings(
    value: &serde_json::Value,
) -> Result<MemoryMaintenanceSettings, Vec<ZodIssue>> {
    use crate::api::zod_issues::key;
    let Some(obj) = value.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(value))]);
    };
    let mut issues = Vec::new();
    let mut out = MemoryMaintenanceSettings::default();
    match obj.get("enabled") {
        None => {}
        Some(serde_json::Value::Bool(b)) => out.enabled = *b,
        got => issues.push(ZodIssue::invalid_type("boolean", vec![key("enabled")], got)),
    }
    if let Some(v) = obj.get("perCharacterCap") {
        if let Some(n) = maintenance_cap_issues(v, vec![key("perCharacterCap")], &mut issues) {
            out.per_character_cap = n;
        }
    }
    if let Some(v) = obj.get("perCharacterCapOverrides") {
        if let Some(m) = maintenance_overrides_issues(v, &mut issues) {
            out.per_character_cap_overrides = m;
        }
    }
    let phases = obj
        .get("phases")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(m) = phase_toggle_issues(&phases, true, &mut issues) {
        let flag = |k: &str| {
            m.get(k)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true)
        };
        out.phases = MemoryMaintenancePhaseToggles {
            fold_near_duplicates: flag("foldNearDuplicates"),
            demote: flag("demote"),
            retire: flag("retire"),
        };
    }
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

/// v4 `getMemoryMaintenanceSettings()` (`index.ts:266-272`): unwritten → the
/// defaults SILENTLY; a stored value that fails `JSON.parse` or the schema →
/// the defaults with v4's WARN `[InstanceSettings] memoryMaintenance failed to
/// parse — using defaults` `{error}` ([`read_json_setting`]).
pub fn get_memory_maintenance_settings(main: &Connection) -> MemoryMaintenanceSettings {
    read_json_setting(
        main,
        KEY_MEMORY_MAINTENANCE,
        |v| parse_memory_maintenance_settings(v).ok(),
        MemoryMaintenanceSettings::default,
    )
}

/// v4 `setMemoryMaintenanceSettings(value)` (`index.ts:274-283`): the current
/// settings shallow-merged with `patch` (`{...current, ...value}` — a sent
/// `perCharacterCapOverrides` or `phases` REPLACES the stored one whole),
/// validated (`writeJsonSetting`'s `schema.parse` — a failure is `Err`
/// carrying the `ZodError.message` bytes), stored as `JSON.stringify` in schema
/// key order, and answered.
pub fn set_memory_maintenance_settings(
    main: &Connection,
    patch: &serde_json::Value,
) -> Result<MemoryMaintenanceSettings, DbError> {
    let merged = merge_patch(get_memory_maintenance_settings(main).to_json(), patch);
    let validated = parse_memory_maintenance_settings(&merged)
        .map_err(|issues| DbError::Internal(crate::api::zod_issues::zod_error_message(&issues)))?;
    write_setting(
        main,
        KEY_MEMORY_MAINTENANCE,
        &validated.to_json().to_string(),
    )?;
    Ok(validated)
}

/// v4 `MemoryExtractionLimits` (`settings.types.ts:308-322`) — the extraction
/// rate limiter, instance-wide (`instance_settings.memoryExtractionLimits`).
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryExtractionLimits {
    /// The limiter is active. Off by default.
    pub enabled: bool,
    /// Memories one character may accrue per hour (int `1..=10000`, v4
    /// `c2795733f`'s `.max(10_000)`).
    pub max_per_hour: i64,
    /// Fraction of `maxPerHour` where the graduated floor begins (`0..=1`).
    pub soft_start_fraction: f64,
    /// Importance floor in the graduated band (`0..=1`).
    pub soft_floor: f64,
}

impl Default for MemoryExtractionLimits {
    fn default() -> Self {
        Self {
            enabled: false,
            max_per_hour: 20,
            soft_start_fraction: 0.7,
            soft_floor: 0.7,
        }
    }
}

impl MemoryExtractionLimits {
    /// v4's JSON, schema key order, numbers JS-rendered.
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        m.insert("enabled".into(), self.enabled.into());
        m.insert("maxPerHour".into(), self.max_per_hour.into());
        m.insert(
            "softStartFraction".into(),
            super::js_number_to_json(self.soft_start_fraction),
        );
        m.insert(
            "softFloor".into(),
            super::js_number_to_json(self.soft_floor),
        );
        serde_json::Value::Object(m)
    }
}

/// v4 `MemoryExtractionLimitsSchema.safeParse` with Zod's issue list — the
/// `maxPerHour` bound `z.number().int().positive().max(10_000)`.
pub fn parse_memory_extraction_limits_settings(
    value: &serde_json::Value,
) -> Result<MemoryExtractionLimits, Vec<ZodIssue>> {
    use crate::api::zod_issues::key;
    let Some(obj) = value.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(value))]);
    };
    let mut issues = Vec::new();
    let mut out = MemoryExtractionLimits::default();
    match obj.get("enabled") {
        None => {}
        Some(serde_json::Value::Bool(b)) => out.enabled = *b,
        got => issues.push(ZodIssue::invalid_type("boolean", vec![key("enabled")], got)),
    }
    if let Some(v) = obj.get("maxPerHour") {
        if let Some(n) = zod_number_field(
            v,
            "maxPerHour",
            true,
            Some((0.0, false)),
            Some(10_000.0),
            &mut issues,
        ) {
            out.max_per_hour = n as i64;
        }
    }
    for (k, slot) in [
        ("softStartFraction", &mut out.soft_start_fraction),
        ("softFloor", &mut out.soft_floor),
    ] {
        if let Some(v) = obj.get(k) {
            if let Some(n) =
                zod_number_field(v, k, false, Some((0.0, true)), Some(1.0), &mut issues)
            {
                *slot = n;
            }
        }
    }
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

/// v4 `patchSchemaOf(schema)` (`lib/schemas/patch-schema.ts`, bug 244) over one
/// field list: every key OPTIONAL and NOT defaulted — "what the caller sent,
/// and nothing else" — bounds, enums and nullability kept, unknown keys
/// stripped, output in schema key order. `check(key, value, issues)` answers
/// the field's parsed value or pushes its issues.
fn patch_of(
    value: &serde_json::Value,
    keys: &[&str],
    mut check: impl FnMut(&str, &serde_json::Value, &mut Vec<ZodIssue>) -> Option<serde_json::Value>,
) -> Result<serde_json::Map<String, serde_json::Value>, Vec<ZodIssue>> {
    let Some(obj) = value.as_object() else {
        return Err(vec![ZodIssue::invalid_type("object", vec![], Some(value))]);
    };
    let mut issues = Vec::new();
    let mut out = serde_json::Map::new();
    for k in keys {
        if let Some(v) = obj.get(*k) {
            if let Some(parsed) = check(k, v, &mut issues) {
                out.insert((*k).to_string(), parsed);
            }
        }
    }
    if issues.is_empty() {
        Ok(out)
    } else {
        Err(issues)
    }
}

/// A `z.boolean()` patch field.
fn bool_field(
    k: &str,
    v: &serde_json::Value,
    issues: &mut Vec<ZodIssue>,
) -> Option<serde_json::Value> {
    if v.is_boolean() {
        Some(v.clone())
    } else {
        issues.push(ZodIssue::invalid_type(
            "boolean",
            vec![crate::api::zod_issues::key(k)],
            Some(v),
        ));
        None
    }
}

/// A number patch field rendered as JS renders it.
fn number_field(
    k: &str,
    v: &serde_json::Value,
    int: bool,
    min: Option<(f64, bool)>,
    max: Option<f64>,
    issues: &mut Vec<ZodIssue>,
) -> Option<serde_json::Value> {
    zod_number_field(v, k, int, min, max, issues).map(super::js_number_to_json)
}

/// `patchSchemaOf(MemoryConsolidationSettingsSchema)` (bug 244 — the
/// consolidation-config write and `maintain`'s `settings.consolidation`).
pub fn parse_memory_consolidation_patch(
    value: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, Vec<ZodIssue>> {
    patch_of(
        value,
        &[
            "enabled",
            "connectionProfileId",
            "clusterThreshold",
            "tfidfClusterThreshold",
            "minClusterSize",
            "maxClusterSize",
            "matureAfterDays",
            "maxClustersPerRun",
            "watermark",
            "coldRetentionDays",
        ],
        |k, v, issues| match k {
            "enabled" => bool_field(k, v, issues),
            "connectionProfileId" => match v {
                serde_json::Value::Null | serde_json::Value::String(_) => Some(v.clone()),
                _ => {
                    issues.push(ZodIssue::invalid_type(
                        "string",
                        vec![crate::api::zod_issues::key(k)],
                        Some(v),
                    ));
                    None
                }
            },
            "clusterThreshold" | "tfidfClusterThreshold" => {
                number_field(k, v, false, Some((0.0, true)), Some(1.0), issues)
            }
            "minClusterSize" | "maxClusterSize" => {
                number_field(k, v, true, Some((2.0, true)), None, issues)
            }
            "matureAfterDays" => number_field(k, v, false, Some((0.0, true)), None, issues),
            "maxClustersPerRun" | "watermark" => {
                number_field(k, v, true, Some((0.0, false)), None, issues)
            }
            _ => match v {
                serde_json::Value::Null => Some(serde_json::Value::Null),
                _ => number_field(k, v, true, Some((0.0, false)), None, issues),
            },
        },
    )
}

/// `patchSchemaOf(MemoryExtractionModeSettingsSchema)` (bug 244).
pub fn parse_memory_extraction_mode_patch(
    value: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, Vec<ZodIssue>> {
    patch_of(
        value,
        &["otherPass", "perTurnOtherFloor", "foldCandidatesPerSubject"],
        |k, v, issues| match k {
            "otherPass" => match v.as_str() {
                Some(s) if MEMORY_EXTRACTION_OTHER_PASS.contains(&s) => Some(v.clone()),
                _ => {
                    issues.push(ZodIssue::invalid_value(
                        &MEMORY_EXTRACTION_OTHER_PASS,
                        vec![crate::api::zod_issues::key(k)],
                    ));
                    None
                }
            },
            "perTurnOtherFloor" => number_field(k, v, false, Some((0.0, true)), Some(1.0), issues),
            _ => number_field(k, v, true, Some((0.0, false)), None, issues),
        },
    )
}

/// `patchSchemaOf(MemoryExtractionLimitsSchema)` (bug 244 — the
/// extraction-limits-config write).
pub fn parse_memory_extraction_limits_patch(
    value: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, Vec<ZodIssue>> {
    patch_of(
        value,
        &["enabled", "maxPerHour", "softStartFraction", "softFloor"],
        |k, v, issues| match k {
            "enabled" => bool_field(k, v, issues),
            "maxPerHour" => number_field(k, v, true, Some((0.0, false)), Some(10_000.0), issues),
            _ => number_field(k, v, false, Some((0.0, true)), Some(1.0), issues),
        },
    )
}

/// `patchSchemaOf(MemoryMaintenancePhaseTogglesSchema)` at the ROOT (the
/// patch-schema jest row `{demote: false}` → `{demote: false}`).
pub fn parse_memory_maintenance_phase_toggles_patch(
    value: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, Vec<ZodIssue>> {
    patch_of(
        value,
        &["foldNearDuplicates", "demote", "retire"],
        bool_field,
    )
}

/// The maintenance settings' PATCH: v4's `maintenanceConfigSchema`
/// (`app/api/v1/memories/route.ts:163-168`) — the ONE maintenance patch schema
/// at the pin (`patchSchemaOf(MemoryMaintenanceSettingsSchema)` itself has no
/// caller): `enabled`, `phases` (`patchSchemaOf(Toggles)` — only the sent
/// toggles; the route merges them into the STORED toggles), `perCharacterCap`
/// and `perCharacterCapOverrides` (whole), each optional; unknown keys (the
/// retired merge knobs) stripped. Note the schema's own key ORDER — `enabled,
/// phases, perCharacterCap, perCharacterCapOverrides` — is the route's, not the
/// settings schema's.
pub fn parse_memory_maintenance_patch(
    value: &serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, Vec<ZodIssue>> {
    patch_of(
        value,
        &[
            "enabled",
            "phases",
            "perCharacterCap",
            "perCharacterCapOverrides",
        ],
        |k, v, issues| match k {
            "enabled" => bool_field(k, v, issues),
            "phases" => phase_toggle_issues(v, false, issues).map(serde_json::Value::Object),
            "perCharacterCap" => {
                maintenance_cap_issues(v, vec![crate::api::zod_issues::key(k)], issues)
                    .map(serde_json::Value::from)
            }
            _ => maintenance_overrides_issues(v, issues).map(serde_json::Value::Object),
        },
    )
}

/// v4 `legacyHousekeepingToMaintenance(raw)` (`lib/memory/settings-legacy.ts:
/// 29-61`) — the `memoryMaintenance` value carried over from a legacy per-user
/// `chat_settings.autoHousekeepingSettings` (a JSON string or object): keeps
/// `enabled`, the cap and the overrides, clamps every cap into the schema's
/// bounds, drops the retired merge knobs. `None` for anything that is not an
/// object (a string that fails `JSON.parse`, an array, `null`, a scalar).
///
/// The VALUE out is exactly what v4 `JSON.stringify`s: `{enabled,
/// perCharacterCap, perCharacterCapOverrides}` — NO `phases` (they default at
/// read time through the schema's `.prefault({})`). `clampCap` is `typeof n
/// === 'number' ? n : Number(n)` (JS `Number()` over a string / boolean /
/// `null` — `Number(null)` is 0, clamped up to 100), non-finite → dropped,
/// else `min(100000, max(100, floor(n)))`; `enabled` is `v.enabled === true`.
/// An override that cannot be clamped is DROPPED; the rest keep source order
/// (V8's own-key order, which `JSON.parse` already produced).
pub fn legacy_housekeeping_to_maintenance(raw: &serde_json::Value) -> Option<serde_json::Value> {
    let parsed;
    let value = match raw {
        serde_json::Value::String(s) => {
            // `JSON.parse` failure → null (serde refusing what V8 takes — `1e400`,
            // a lone-surrogate escape — is the recorded `v8_json_parse_message` class).
            parsed = serde_json::from_str::<serde_json::Value>(s).ok()?;
            &parsed
        }
        other => other,
    };
    let v = value.as_object()?;
    let clamp_cap = |n: &serde_json::Value| -> Option<i64> {
        let num = match n {
            serde_json::Value::Number(x) => x.as_f64()?,
            other => crate::pascal::js_value::to_number(other),
        };
        if !num.is_finite() {
            return None;
        }
        Some(
            (num.floor().max(MEMORY_MAINTENANCE_CAP_MIN as f64))
                .min(MEMORY_MAINTENANCE_CAP_MAX as f64) as i64,
        )
    };
    // `v.perCharacterCapOverrides && typeof … === 'object'` admits an ARRAY
    // too: `Object.entries([150, 250])` is `[["0", 150], ["1", 250]]`.
    let entries: Vec<(String, serde_json::Value)> = match v.get("perCharacterCapOverrides") {
        Some(serde_json::Value::Object(o)) => v8_own_key_order(o).into_iter().collect(),
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .enumerate()
            .map(|(i, cap)| (i.to_string(), cap.clone()))
            .collect(),
        _ => Vec::new(),
    };
    let mut overrides = serde_json::Map::new();
    for (character_id, cap) in entries {
        if let Some(clamped) = clamp_cap(&cap) {
            overrides.insert(character_id, clamped.into());
        }
    }
    let mut out = serde_json::Map::new();
    out.insert(
        "enabled".into(),
        (v.get("enabled") == Some(&serde_json::Value::Bool(true))).into(),
    );
    out.insert(
        "perCharacterCap".into(),
        v.get("perCharacterCap")
            .and_then(clamp_cap)
            .unwrap_or(MEMORY_MAINTENANCE_DEFAULT_CAP)
            .into(),
    );
    out.insert(
        "perCharacterCapOverrides".into(),
        serde_json::Value::Object(overrides),
    );
    Some(serde_json::Value::Object(out))
}

/// v4 `DEAD_INSTANCE_SETTING_KEYS` (`settings-legacy.ts:65`): keys a backup may
/// carry that nothing reads any more.
pub const DEAD_INSTANCE_SETTING_KEYS: &[&str] = &["memoryExtractionConcurrency"];

/// v4 `withMemoryMaintenanceFromLegacy(instanceSettings, chatSettings)`
/// (`settings-legacy.ts:77-91`) — the RESTORE twin of the settings move: the
/// archive's instance-settings rows minus the dead keys; when none carries
/// `memoryMaintenance`, one DERIVED from the chat-settings row that HAS an
/// `autoHousekeepingSettings` property (KEY PRESENCE — a `null` value
/// qualifies, and then derives nothing) — the single user's, else the FIRST
/// such row (not the latest — the migration's fallback differs, both v4's) —
/// APPENDED LAST. The backup's own `memoryMaintenance` row wins. No log line.
pub fn with_memory_maintenance_from_legacy(
    instance_rows: &[(String, String)],
    chat_settings_rows: &[serde_json::Value],
) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = instance_rows
        .iter()
        .filter(|(k, _)| !DEAD_INSTANCE_SETTING_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    if rows.iter().any(|(k, _)| k == KEY_MEMORY_MAINTENANCE) {
        return rows;
    }
    let candidates: Vec<&serde_json::Map<String, serde_json::Value>> = chat_settings_rows
        .iter()
        .filter_map(serde_json::Value::as_object)
        .filter(|s| s.contains_key("autoHousekeepingSettings"))
        .collect();
    let source = candidates
        .iter()
        .find(|s| {
            s.get("userId").and_then(serde_json::Value::as_str)
                == Some(crate::services::provisioning::SINGLE_USER_ID)
        })
        .or_else(|| candidates.first());
    let Some(maintenance) =
        source.and_then(|s| legacy_housekeeping_to_maintenance(&s["autoHousekeepingSettings"]))
    else {
        return rows;
    };
    rows.push((KEY_MEMORY_MAINTENANCE.to_string(), maintenance.to_string()));
    rows
}

// === end P4.D288 ===

#[cfg(test)]
mod memory_programme_settings_tests {
    use super::*;
    use serde_json::json;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE \"instance_settings\" (\"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL)",
        )
        .unwrap();
        c
    }

    #[test]
    fn defaults_are_v4s_parse_of_an_empty_object() {
        let c = conn();
        assert_eq!(
            get_memory_consolidation_settings(&c).to_json().to_string(),
            r#"{"enabled":false,"connectionProfileId":null,"clusterThreshold":0.72,"tfidfClusterThreshold":0.3,"minClusterSize":3,"maxClusterSize":30,"matureAfterDays":7,"maxClustersPerRun":40,"watermark":150,"coldRetentionDays":null}"#
        );
        assert_eq!(
            get_memory_extraction_mode_settings(&c)
                .to_json()
                .to_string(),
            r#"{"otherPass":"hybrid","perTurnOtherFloor":0.75,"foldCandidatesPerSubject":3}"#
        );
    }

    #[test]
    fn a_round_trip_merges_and_stores_schema_order() {
        let c = conn();
        let s = set_memory_consolidation_settings(
            &c,
            &json!({"coldRetentionDays": 14, "enabled": true}),
        )
        .unwrap();
        assert_eq!(s.cold_retention_days, Some(14));
        let s = set_memory_consolidation_settings(&c, &json!({"coldRetentionDays": null})).unwrap();
        assert_eq!(s.cold_retention_days, None);
        assert!(s.enabled, "the earlier write survives the merge");
        let stored: String = c
            .query_row(
                "SELECT value FROM instance_settings WHERE key = 'memoryConsolidation'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            stored,
            r#"{"enabled":true,"connectionProfileId":null,"clusterThreshold":0.72,"tfidfClusterThreshold":0.3,"minClusterSize":3,"maxClusterSize":30,"matureAfterDays":7,"maxClustersPerRun":40,"watermark":150,"coldRetentionDays":null}"#
        );
    }

    #[test]
    fn refusals_carry_v4s_issue_bytes() {
        let issues = parse_memory_consolidation_settings(&json!({"watermark": 0})).unwrap_err();
        assert_eq!(
            serde_json::to_string(&issues).unwrap(),
            r#"[{"origin":"number","code":"too_small","minimum":0,"inclusive":false,"path":["watermark"],"message":"Too small: expected number to be >0"}]"#
        );
        let issues =
            parse_memory_consolidation_settings(&json!({"minClusterSize": 1.5})).unwrap_err();
        assert_eq!(
            serde_json::to_string(&issues).unwrap(),
            r#"[{"expected":"int","format":"safeint","code":"invalid_type","path":["minClusterSize"],"message":"Invalid input: expected int, received number"}]"#
        );
        let issues =
            parse_memory_extraction_mode_settings(&json!({"otherPass": "bogus"})).unwrap_err();
        assert_eq!(
            serde_json::to_string(&issues).unwrap(),
            r#"[{"code":"invalid_value","values":["turn","fold","hybrid"],"path":["otherPass"],"message":"Invalid option: expected one of \"turn\"|\"fold\"|\"hybrid\""}]"#
        );
        let c = conn();
        let err = set_memory_consolidation_settings(&c, &json!({"watermark": 0})).unwrap_err();
        assert!(err.to_string().starts_with("[\n  {\n"), "{err}");
    }

    #[test]
    fn a_corrupt_stored_value_reads_the_defaults_with_v4s_warn() {
        let c = conn();
        c.execute(
            "INSERT INTO instance_settings (key, value) VALUES ('memoryExtractionMode', '{nope')",
            [],
        )
        .unwrap();
        let (s, lines) =
            crate::test_support::captured_with(|| get_memory_extraction_mode_settings(&c));
        assert_eq!(s, MemoryExtractionModeSettings::default());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains(
                "[InstanceSettings] memoryExtractionMode failed to parse — using defaults"
            ),
            "{lines:?}"
        );
        // Unwritten: the defaults, silently.
        let (_, lines) =
            crate::test_support::captured_with(|| get_memory_consolidation_settings(&c));
        assert!(lines.is_empty(), "{lines:?}");
    }
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
    fn parse_measures_length_in_zods_code_point_window() {
        // Zod ≥ 4.5.4 (v4 `6e1a64ea6`): 101 astral characters are 202 UTF-16
        // units but 101 code points — ACCEPTED against the `.max(200)` (4.4.3's
        // UTF-16 rule refused them; `settings_routes_equivalence`'s
        // `taboo_put_astral_over_bound` is the oracle-driven twin). 201 fail in
        // both measures.
        let within = "🎩".repeat(101);
        assert!(parse_taboo_settings(&serde_json::json!({ "phrases": [within] })).is_some());
        let over = "🎩".repeat(201);
        assert_eq!(
            parse_taboo_settings(&serde_json::json!({ "phrases": [over] })),
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
