//! The `chats` **read path** (the conversation capstone, sub-unit 2). Ports the
//! slim-row read marshaling — the inverse of sub-unit 1's ~96-column write
//! ([`super::chats`]) — plus the `findBy*` queries of v4's
//! `lib/database/repositories/chats.repository.ts`. `chats` has **no vault
//! overlay** (unlike `characters`), so every read is a single-connection SELECT +
//! marshal.
//!
//! ## The marshaling: row → `ChatMetadata` (v4 `_findById` = hydrateRow + Zod parse)
//!
//! v4 reads a row through `SQLiteCollection.hydrateRow` (parse JSON columns,
//! coerce `is*` INTEGER columns to bool, `NULL` → `undefined`) then
//! `ChatMetadataBaseSchema.parse` (apply `.default(...)`, drop the `undefined`
//! optionals). The net per-column result:
//!
//!   - required strings (`id`/`userId`/`title`/`createdAt`/`updatedAt`): present.
//!   - `*.nullable().optional()` columns (TEXT/UUID/enum/number/bool/JSON): a
//!     `NULL` cell → the key is **omitted** (v4 emits `undefined`, dropped by
//!     `JSON.stringify`); a non-null cell → present (JSON parsed, bool coerced,
//!     number rendered the JS way via [`js_number_to_json`]).
//!   - `.default(N)` numbers / `.default(false)` bools / `.default('salon'|'normal')`
//!     enums / `.default('[]')` strings: **always present** at the stored value
//!     (or the default when the cell is `NULL`).
//!   - `.default([])` array columns: present (parsed; `NULL`/empty → `[]`).
//!   - `state` (`JsonSchema.default({})`): present (parsed; `NULL` → `{}`).
//!   - `participants` (`.default([])` array of [`ChatParticipant`]): present —
//!     each element re-parsed through the participant schema so its own
//!     `.default(...)`s materialize (`controlledBy: 'llm'`, `displayOrder: 0`,
//!     `isActive: true`, `status: 'active'`, `hasHistoryAccess: false`) and its
//!     `nullable().optional()` fields drop when absent.
//!
//! Comparison in the read-differential is over `serde_json::Value` (key-order
//! independent), so JSON-object columns are parsed straight into `Value` and the
//! write-side typed-struct key-order discipline does not apply here.
//!
//! ## The queries
//!
//! `find_by_id` / `find_all` / `find_by_user_id` / `find_by_character_id` /
//! `find_by_type` / `find_recent_summarized_by_character`. The
//! `participants.characterId` filter is the nested `json_each` + `json_extract`
//! match v4's query translator emits; `find_recent_summarized_by_character`
//! reproduces v4's `$exists`/`$nin`/`$ne` → `IS NOT NULL` / `NOT IN` / `!=` plus
//! `ORDER BY "lastMessageAt" DESC` + `LIMIT`.
//!
//! ## `cycleOrderParticipantIds` — the ONE column whose two v4 shapes AGREE
//!
//! P4.D171 (v4 `2aca73ad6`) — measured at the `78b381a96` re-dump: BOTH the
//! migration (`addColumnIfMissing('chats', 'cycleOrderParticipantIds',
//! "TEXT DEFAULT '[]'")`) and `generateDDL` (`z.string().default('[]')` in
//! `ChatMetadataBaseSchema`) declare the identical type and default. Every
//! OTHER column this port has carried across a schema move has disagreed
//! between the two v4 shapes (the P4.D77/D78/D135 pattern, most recently
//! `chat_messages.routeTrail` beside this one — bare `TEXT` vs `TEXT DEFAULT
//! NULL`) — so [`super::chats_cycle_order_repair`]'s ensure carries only ONE
//! DDL, not two. Do not carry two shapes on reflex for the next column move;
//! re-measure at the new pin first (`git show <sha> --
//! migrations/scripts/<name>.ts lib/schemas/chat.types.ts`).

use rusqlite::{Connection, Row};
use serde_json::{Map, Value};

use super::chats::ChatParticipant;
use super::js_number_to_json;
use super::DbError;

/// All 100 columns, in `ChatMetadataBaseSchema` field order (= DDL / SELECT
/// order). The count, the order and `marshal_row`'s index table are pinned
/// against the D23 dump by this module's `alignment_census` tests.
/// `timelineMode` (v4 8bf3cb5f, the episodic spine) sits between
/// `commonplaceRecallHistory` and `budgetMaxTurns` — its generateDDL/Zod-shape
/// position.
const ALL_COLUMNS: &str = "id, userId, participants, title, contextSummary, sillyTavernMetadata, \
     tags, roleplayTemplateId, timestampConfig, lastTurnParticipantId, messageCount, lastMessageAt, \
     lastRenameCheckInterchange, compactionGeneration, lastSummaryTurn, lastSummaryTokens, \
     lastFullRebuildTurn, summaryAnchorMessageIds, isPaused, isManuallyRenamed, \
     impersonatingParticipantIds, activeTypingParticipantId, allLLMPauseTurnCount, turnQueue, \
     spokenThisCycleParticipantIds, cycleOrderParticipantIds, documentEditingMode, documentMode, \
     dividerPosition, terminalMode, \
     activeTerminalSessionId, rightPaneVerticalSplit, projectId, scenarioText, totalPromptTokens, \
     totalCompletionTokens, estimatedCostUSD, priceSource, showSystemEventsOverride, \
     requestFullContextOnNextMessage, disabledTools, disabledToolGroups, forceToolsOnNextMessage, \
     allowCrossCharacterVaultReads, pendingOutfitNotifications, state, compressionCache, \
     agentModeEnabled, agentTurnCount, storyBackgroundImageId, lastBackgroundGeneratedAt, \
     imageProfileId, alertCharactersOfLanternImages, isDangerousChat, dangerScore, dangerCategories, \
     dangerClassifiedAt, dangerClassifiedAtMessageCount, conciergeOverride, sceneState, \
     renderedMarkdown, equippedOutfit, characterAvatars, avatarGenerationEnabled, chatType, \
     helpPageUrl, consoleConnectionProfileId, compiledIdentityStacks, courierCheckpoints, \
     commonplaceSceneCache, commonplaceRecallHistory, timelineMode, budgetMaxTurns, budgetMaxTokens, \
     budgetMaxWallClockMs, budgetEstimatedSpendCapUSD, scheduleCron, scheduleFreshnessWindowMs, \
     scheduleNextRunAt, scheduleLastRunAt, runState, currentRunId, runStateMessage, runStartedAt, \
     runEndedAt, runPausedAt, runPausedAccumMs, runTurnsConsumed, runTokensConsumed, \
     runMilestonesAnnounced, runDestructiveToolsAllowed, budgetExcludeCacheHits, runVisibility, \
     coreWhisperEnabled, coreWhisperInterval, showThinking, createdAt, updatedAt, \
     answerConfirmationOverride, turnSkippingEnabled";

/// Insert a nullable-optional TEXT/UUID/enum value: `Some` → string, `None` → omit.
fn put_opt_string(obj: &mut Map<String, Value>, key: &str, v: Option<String>) {
    if let Some(s) = v {
        obj.insert(key.to_string(), Value::String(s));
    }
}

/// Insert a nullable-optional boolean column (`NULL` → omit, `0`/`1` → bool).
fn put_opt_bool(obj: &mut Map<String, Value>, key: &str, v: Option<i64>) {
    if let Some(n) = v {
        obj.insert(key.to_string(), Value::Bool(n == 1));
    }
}

/// Insert a nullable-optional number column (`NULL` → omit, else the JS rendering).
fn put_opt_number(obj: &mut Map<String, Value>, key: &str, v: Option<f64>) {
    if let Some(n) = v {
        obj.insert(key.to_string(), js_number_to_json(n));
    }
}

/// Insert a nullable-optional JSON column (`NULL`/empty/`"null"` → omit, else
/// parsed — v4 `fromJsonSafe` + the `.optional()` drop).
fn put_opt_json(obj: &mut Map<String, Value>, key: &str, v: Option<String>) {
    let Some(raw) = v else { return };
    if raw.is_empty() || raw == "null" {
        return;
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
        if !parsed.is_null() {
            obj.insert(key.to_string(), parsed);
        }
    }
}

/// A `.default(default)` number column: stored value (JS-rendered) or the default.
fn number_or(v: Option<f64>, default: f64) -> Value {
    js_number_to_json(v.unwrap_or(default))
}

/// A `.default([])` array column: parsed array, or `[]` when `NULL`/empty/invalid.
fn array_or_empty(v: Option<String>) -> Value {
    v.as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .filter(Value::is_array)
        .unwrap_or_else(|| Value::Array(Vec::new()))
}

/// `state` (`JsonSchema.default({})`): parsed object, or `{}` when `NULL`/invalid.
fn object_or_empty(v: Option<String>) -> Value {
    v.as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Map::new()))
}

/// `participants`: re-parse each element through [`ChatParticipant`] so its
/// `.default(...)`s materialize and `nullable().optional()` fields drop when
/// absent, mirroring v4's per-participant Zod parse. `NULL`/empty/invalid → `[]`.
fn marshal_participants(v: Option<String>) -> Value {
    let parsed = v
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str::<Vec<ChatParticipant>>(s).ok())
        .unwrap_or_default();
    serde_json::to_value(parsed).unwrap_or_else(|_| Value::Array(Vec::new()))
}

/// Marshal one `chats` row into a `ChatMetadata` JSON object.
fn marshal_row(row: &Row) -> Result<Value, rusqlite::Error> {
    let mut obj = Map::new();

    obj.insert("id".into(), Value::String(row.get::<_, String>(0)?));
    obj.insert("userId".into(), Value::String(row.get::<_, String>(1)?));
    obj.insert("participants".into(), marshal_participants(row.get(2)?));
    obj.insert("title".into(), Value::String(row.get::<_, String>(3)?));
    put_opt_string(&mut obj, "contextSummary", row.get(4)?);
    put_opt_json(&mut obj, "sillyTavernMetadata", row.get(5)?);
    obj.insert("tags".into(), array_or_empty(row.get(6)?));
    put_opt_string(&mut obj, "roleplayTemplateId", row.get(7)?);
    put_opt_json(&mut obj, "timestampConfig", row.get(8)?);
    put_opt_string(&mut obj, "lastTurnParticipantId", row.get(9)?);
    obj.insert("messageCount".into(), number_or(row.get(10)?, 0.0));
    put_opt_string(&mut obj, "lastMessageAt", row.get(11)?);
    obj.insert(
        "lastRenameCheckInterchange".into(),
        number_or(row.get(12)?, 0.0),
    );
    obj.insert("compactionGeneration".into(), number_or(row.get(13)?, 0.0));
    obj.insert("lastSummaryTurn".into(), number_or(row.get(14)?, 0.0));
    obj.insert("lastSummaryTokens".into(), number_or(row.get(15)?, 0.0));
    obj.insert("lastFullRebuildTurn".into(), number_or(row.get(16)?, 0.0));
    obj.insert(
        "summaryAnchorMessageIds".into(),
        array_or_empty(row.get(17)?),
    );
    obj.insert(
        "isPaused".into(),
        Value::Bool(row.get::<_, Option<i64>>(18)?.unwrap_or(0) == 1),
    );
    obj.insert(
        "isManuallyRenamed".into(),
        Value::Bool(row.get::<_, Option<i64>>(19)?.unwrap_or(0) == 1),
    );
    obj.insert(
        "impersonatingParticipantIds".into(),
        array_or_empty(row.get(20)?),
    );
    put_opt_string(&mut obj, "activeTypingParticipantId", row.get(21)?);
    obj.insert("allLLMPauseTurnCount".into(), number_or(row.get(22)?, 0.0));
    obj.insert(
        "turnQueue".into(),
        Value::String(row.get::<_, Option<String>>(23)?.unwrap_or_else(empty_arr)),
    );
    obj.insert(
        "spokenThisCycleParticipantIds".into(),
        Value::String(row.get::<_, Option<String>>(24)?.unwrap_or_else(empty_arr)),
    );
    // The drawn speaking order for this cycle (v4 2aca73ad6): the raw JSON
    // string, as v4 spreads the row — always present (`.default('[]')`), same
    // shape as `turnQueue`/`spokenThisCycleParticipantIds` beside it.
    obj.insert(
        "cycleOrderParticipantIds".into(),
        Value::String(row.get::<_, Option<String>>(25)?.unwrap_or_else(empty_arr)),
    );
    obj.insert(
        "documentEditingMode".into(),
        Value::Bool(row.get::<_, Option<i64>>(26)?.unwrap_or(0) == 1),
    );
    obj.insert(
        "documentMode".into(),
        Value::String(
            row.get::<_, Option<String>>(27)?
                .unwrap_or_else(|| "normal".into()),
        ),
    );
    obj.insert("dividerPosition".into(), number_or(row.get(28)?, 45.0));
    obj.insert(
        "terminalMode".into(),
        Value::String(
            row.get::<_, Option<String>>(29)?
                .unwrap_or_else(|| "normal".into()),
        ),
    );
    put_opt_string(&mut obj, "activeTerminalSessionId", row.get(30)?);
    obj.insert(
        "rightPaneVerticalSplit".into(),
        number_or(row.get(31)?, 50.0),
    );
    put_opt_string(&mut obj, "projectId", row.get(32)?);
    put_opt_string(&mut obj, "scenarioText", row.get(33)?);
    obj.insert("totalPromptTokens".into(), number_or(row.get(34)?, 0.0));
    obj.insert("totalCompletionTokens".into(), number_or(row.get(35)?, 0.0));
    put_opt_number(&mut obj, "estimatedCostUSD", row.get(36)?);
    put_opt_string(&mut obj, "priceSource", row.get(37)?);
    put_opt_bool(&mut obj, "showSystemEventsOverride", row.get(38)?);
    obj.insert(
        "requestFullContextOnNextMessage".into(),
        Value::Bool(row.get::<_, Option<i64>>(39)?.unwrap_or(0) == 1),
    );
    obj.insert("disabledTools".into(), array_or_empty(row.get(40)?));
    obj.insert("disabledToolGroups".into(), array_or_empty(row.get(41)?));
    obj.insert(
        "forceToolsOnNextMessage".into(),
        Value::Bool(row.get::<_, Option<i64>>(42)?.unwrap_or(0) == 1),
    );
    obj.insert(
        "allowCrossCharacterVaultReads".into(),
        Value::Bool(row.get::<_, Option<i64>>(43)?.unwrap_or(0) == 1),
    );
    put_opt_json(&mut obj, "pendingOutfitNotifications", row.get(44)?);
    obj.insert("state".into(), object_or_empty(row.get(45)?));
    put_opt_json(&mut obj, "compressionCache", row.get(46)?);
    put_opt_bool(&mut obj, "agentModeEnabled", row.get(47)?);
    obj.insert("agentTurnCount".into(), number_or(row.get(48)?, 0.0));
    put_opt_string(&mut obj, "storyBackgroundImageId", row.get(49)?);
    put_opt_string(&mut obj, "lastBackgroundGeneratedAt", row.get(50)?);
    put_opt_string(&mut obj, "imageProfileId", row.get(51)?);
    put_opt_bool(&mut obj, "alertCharactersOfLanternImages", row.get(52)?);
    put_opt_bool(&mut obj, "isDangerousChat", row.get(53)?);
    put_opt_number(&mut obj, "dangerScore", row.get(54)?);
    obj.insert("dangerCategories".into(), array_or_empty(row.get(55)?));
    put_opt_string(&mut obj, "dangerClassifiedAt", row.get(56)?);
    put_opt_number(&mut obj, "dangerClassifiedAtMessageCount", row.get(57)?);
    put_opt_string(&mut obj, "conciergeOverride", row.get(58)?);
    put_opt_json(&mut obj, "sceneState", row.get(59)?);
    put_opt_string(&mut obj, "renderedMarkdown", row.get(60)?);
    put_opt_json(&mut obj, "equippedOutfit", row.get(61)?);
    put_opt_json(&mut obj, "characterAvatars", row.get(62)?);
    put_opt_bool(&mut obj, "avatarGenerationEnabled", row.get(63)?);
    obj.insert(
        "chatType".into(),
        Value::String(
            row.get::<_, Option<String>>(64)?
                .unwrap_or_else(|| "salon".into()),
        ),
    );
    put_opt_string(&mut obj, "helpPageUrl", row.get(65)?);
    put_opt_string(&mut obj, "consoleConnectionProfileId", row.get(66)?);
    put_opt_json(&mut obj, "compiledIdentityStacks", row.get(67)?);
    put_opt_json(&mut obj, "courierCheckpoints", row.get(68)?);
    put_opt_json(&mut obj, "commonplaceSceneCache", row.get(69)?);
    put_opt_json(&mut obj, "commonplaceRecallHistory", row.get(70)?);
    // Episodic spine (v4 8bf3cb5f): 'realtime' | 'narrative'; NULL reads as
    // realtime and is omitted (v4's undefined dropped by JSON.stringify).
    put_opt_string(&mut obj, "timelineMode", row.get(71)?);
    put_opt_number(&mut obj, "budgetMaxTurns", row.get(72)?);
    put_opt_number(&mut obj, "budgetMaxTokens", row.get(73)?);
    put_opt_number(&mut obj, "budgetMaxWallClockMs", row.get(74)?);
    put_opt_number(&mut obj, "budgetEstimatedSpendCapUSD", row.get(75)?);
    put_opt_string(&mut obj, "scheduleCron", row.get(76)?);
    put_opt_number(&mut obj, "scheduleFreshnessWindowMs", row.get(77)?);
    put_opt_string(&mut obj, "scheduleNextRunAt", row.get(78)?);
    put_opt_string(&mut obj, "scheduleLastRunAt", row.get(79)?);
    put_opt_string(&mut obj, "runState", row.get(80)?);
    put_opt_string(&mut obj, "currentRunId", row.get(81)?);
    put_opt_string(&mut obj, "runStateMessage", row.get(82)?);
    put_opt_string(&mut obj, "runStartedAt", row.get(83)?);
    put_opt_string(&mut obj, "runEndedAt", row.get(84)?);
    put_opt_string(&mut obj, "runPausedAt", row.get(85)?);
    put_opt_number(&mut obj, "runPausedAccumMs", row.get(86)?);
    put_opt_number(&mut obj, "runTurnsConsumed", row.get(87)?);
    put_opt_number(&mut obj, "runTokensConsumed", row.get(88)?);
    obj.insert(
        "runMilestonesAnnounced".into(),
        number_or(row.get(89)?, 0.0),
    );
    obj.insert(
        "runDestructiveToolsAllowed".into(),
        number_or(row.get(90)?, 0.0),
    );
    obj.insert(
        "budgetExcludeCacheHits".into(),
        number_or(row.get(91)?, 1.0),
    );
    put_opt_string(&mut obj, "runVisibility", row.get(92)?);
    put_opt_bool(&mut obj, "coreWhisperEnabled", row.get(93)?);
    put_opt_number(&mut obj, "coreWhisperInterval", row.get(94)?);
    put_opt_bool(&mut obj, "showThinking", row.get(95)?);
    obj.insert("createdAt".into(), Value::String(row.get::<_, String>(96)?));
    obj.insert("updatedAt".into(), Value::String(row.get::<_, String>(97)?));
    put_opt_string(&mut obj, "answerConfirmationOverride", row.get(98)?);
    // "Nothing to add" turn-skipping toggle (nullable boolean; NULL → omitted,
    // v4's `undefined` dropped by `JSON.stringify`). v4 b90cd1f5.
    put_opt_bool(&mut obj, "turnSkippingEnabled", row.get(99)?);

    Ok(Value::Object(obj))
}

fn empty_arr() -> String {
    "[]".to_string()
}

/// Run `SELECT <cols> FROM chats <tail>` and marshal each row.
fn run(
    conn: &Connection,
    tail: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Value>, DbError> {
    let sql = format!("SELECT {ALL_COLUMNS} FROM chats {tail}");
    let mut stmt = conn.prepare(sql.trim())?;
    let rows = stmt.query_map(params, marshal_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ============================================================================
// findBy* queries
// ============================================================================

/// Find a chat by id (v4 `findById` → `_findById`). `None` when absent.
pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<Value>, DbError> {
    Ok(run(conn, "WHERE id = ?1", &[&id])?.pop())
}

/// Find all chats (v4 `findAll`).
pub fn find_all(conn: &Connection) -> Result<Vec<Value>, DbError> {
    run(conn, "", &[])
}

/// Find chats by user id (v4 `findByUserId`).
pub fn find_by_user_id(conn: &Connection, user_id: &str) -> Result<Vec<Value>, DbError> {
    run(conn, "WHERE userId = ?1", &[&user_id])
}

/// Find chats that include a character as a participant (v4 `findByCharacterId` —
/// the nested `participants.characterId` match via `json_each` + `json_extract`).
pub fn find_by_character_id(conn: &Connection, character_id: &str) -> Result<Vec<Value>, DbError> {
    run(
        conn,
        "WHERE EXISTS (SELECT 1 FROM json_each(participants) \
             WHERE json_extract(value, '$.characterId') = ?1)",
        &[&character_id],
    )
}

/// Find chats by user id + chat type (v4 `findByType`).
pub fn find_by_type(
    conn: &Connection,
    user_id: &str,
    chat_type: &str,
) -> Result<Vec<Value>, DbError> {
    run(
        conn,
        "WHERE userId = ?1 AND chatType = ?2",
        &[&user_id, &chat_type],
    )
}

/// Find the N most-recent salon chats for a character that carry a `contextSummary`
/// (v4 `findRecentSummarizedByCharacter`). Reproduces the `$exists`/`$nin`/`$ne`
/// filter + `ORDER BY "lastMessageAt" DESC` + `LIMIT`.
pub fn find_recent_summarized_by_character(
    conn: &Connection,
    character_id: &str,
    limit: i64,
    exclude_chat_id: Option<&str>,
) -> Result<Vec<Value>, DbError> {
    let mut where_clause = String::from(
        "WHERE EXISTS (SELECT 1 FROM json_each(participants) \
             WHERE json_extract(value, '$.characterId') = ?1) \
         AND contextSummary IS NOT NULL \
         AND chatType NOT IN ('help', 'brahma')",
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&character_id];
    if let Some(excl) = exclude_chat_id.as_ref() {
        where_clause.push_str(" AND id != ?2");
        params.push(excl);
    }
    let tail = format!("{where_clause} ORDER BY \"lastMessageAt\" DESC LIMIT {limit}");
    run(conn, &tail, &params)
}

/// Scoped read of a chat's Core-whisper override columns (v4
/// `(enabled, interval)` per-chat Core-whisper overrides, each `None` when the
/// column is NULL (v4 `chat.coreWhisperEnabled` / `chat.coreWhisperInterval`).
pub type CoreWhisperOverrides = (Option<bool>, Option<i64>);

/// `chat.coreWhisperEnabled` / `chat.coreWhisperInterval`, both nullable) — the
/// per-chat inputs to `resolveCoreWhisperConfig`. Returns
/// `(enabled, interval)`, each `None` when the column is NULL. `Ok(None)` for a
/// missing chat row. Used by the build_context Core-whisper feeder (W4.6a).
pub fn find_core_whisper_overrides(
    conn: &Connection,
    chat_id: &str,
) -> Result<Option<CoreWhisperOverrides>, DbError> {
    conn.query_row(
        "SELECT coreWhisperEnabled, coreWhisperInterval FROM chats WHERE id = ?1",
        rusqlite::params![chat_id],
        |row| {
            let enabled: Option<i64> = row.get(0)?;
            let interval: Option<i64> = row.get(1)?;
            Ok((enabled.map(|v| v != 0), interval))
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
}

// ============================================================================
// The index↔column alignment census (P4.88 — P4.D171's named OPEN item)
// ============================================================================

#[cfg(test)]
mod alignment_census {
    use super::ALL_COLUMNS;
    use std::collections::BTreeSet;

    /// This file's own source. [`super::marshal_row`] reads its row by POSITION
    /// (`row.get(0)`, `row.get(1)`, …) into keys it names inline, so nothing but
    /// the source itself relates an index to the column it is supposed to be.
    /// The distinct-values fixture the read differential runs over can only
    /// catch a swap between two columns whose seeded values happen to differ in
    /// shape — two adjacent nullable TEXT columns, both NULL in the fixture,
    /// swap silently.
    const SOURCE: &str = include_str!("chats_read.rs");

    /// The D23 dump — the single source of truth for the `chats` DDL.
    const FRESH_SCHEMA: &str = include_str!("../services/provisioning/fresh_schema.json");

    /// `marshal_row`'s body, by brace balance from its signature, with `//`
    /// comment tails removed (a comment may carry a quoted word, and the pairing
    /// below reads string literals).
    fn marshal_row_body() -> String {
        // First occurrence: the definition sits above this module. (The needle
        // appears again in this function's own source, which is why the search
        // is `find` rather than an exactly-once assert.)
        let start = SOURCE
            .find("fn marshal_row(row: &Row)")
            .expect("marshal_row is defined in this file");
        let open = start + SOURCE[start..].find('{').expect("marshal_row has a body");
        let mut depth = 0usize;
        let mut end = None;
        for (offset, ch) in SOURCE[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &SOURCE[open..end.expect("unbalanced braces in marshal_row")];
        assert!(
            !body.contains("#[cfg(test)]"),
            "the scanned zone must be production code only"
        );
        body.lines()
            .map(|line| match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every `("key", …row.get(N)…)` pairing in `marshal_row`, in source order.
    ///
    /// Every statement in that function names its JSON key BEFORE it reads the
    /// row (`put_opt_string(&mut obj, "contextSummary", row.get(4)?)`,
    /// `obj.insert("tags".into(), array_or_empty(row.get(6)?))`), so pairing each
    /// `row.get` with the most recent quoted identifier is exact. A statement
    /// written the other way round would show up as a mismatch, not as silence.
    fn key_index_pairs() -> Vec<(usize, String)> {
        let body = marshal_row_body();
        let mut pairs: Vec<(usize, String)> = Vec::new();
        let mut pending: Option<String> = None;
        // `match_indices` yields char boundaries, and `marshal_row` carries no
        // escaped quotes (asserted by the pairing below going even).
        let quotes: Vec<usize> = body.match_indices('"').map(|(i, _)| i).collect();
        assert_eq!(quotes.len() % 2, 0, "unbalanced string literals");
        let mut literals: Vec<(usize, String)> = Vec::new();
        for pair in quotes.chunks(2) {
            literals.push((pair[0], body[pair[0] + 1..pair[1]].to_string()));
        }
        let reads: Vec<(usize, usize)> = body
            .match_indices("row.get")
            .filter_map(|(at, _)| {
                let rest = &body[at + "row.get".len()..];
                let paren = rest.find('(')?;
                let close = paren + rest[paren..].find(')')?;
                rest[paren + 1..close]
                    .parse::<usize>()
                    .ok()
                    .map(|n| (at, n))
            })
            .collect();
        let mut events: Vec<(usize, Option<String>, Option<usize>)> = Vec::new();
        events.extend(literals.into_iter().map(|(at, s)| (at, Some(s), None)));
        events.extend(reads.into_iter().map(|(at, n)| (at, None, Some(n))));
        events.sort_by_key(|(at, _, _)| *at);
        for (_, literal, index) in events {
            match (literal, index) {
                (Some(lit), _) => {
                    if !lit.is_empty()
                        && lit.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                        && lit.chars().all(|c| c.is_ascii_alphanumeric())
                    {
                        pending = Some(lit);
                    }
                }
                (None, Some(n)) => {
                    let key = pending
                        .take()
                        .unwrap_or_else(|| panic!("row.get({n}) with no key before it"));
                    pairs.push((n, key));
                }
                _ => unreachable!(),
            }
        }
        pairs
    }

    /// The `chats` column names from the D23 dump's `CREATE TABLE`.
    fn schema_columns() -> Vec<String> {
        let schema: serde_json::Value =
            serde_json::from_str(FRESH_SCHEMA).expect("fresh_schema.json parses");
        let ddl = schema["main"]
            .as_array()
            .expect("fresh_schema.json has a main partition")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .find(|s| s.starts_with("CREATE TABLE \"chats\" ("))
            .expect("the dump carries the chats DDL");
        let inner = &ddl[ddl.find('(').unwrap() + 1..ddl.rfind(')').unwrap()];
        inner
            .lines()
            .filter_map(|line| {
                let t = line.trim();
                t.strip_prefix('"')
                    .and_then(|r| r.find('"').map(|e| r[..e].to_string()))
            })
            .collect()
    }

    fn selected_columns() -> Vec<String> {
        ALL_COLUMNS
            .split(',')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect()
    }

    /// The SELECT list names every column the D23 dump declares, and nothing
    /// else. ORDER deliberately differs: `ALL_COLUMNS` follows the Zod field
    /// order this port transcribed, which puts `answerConfirmationOverride` and
    /// `turnSkippingEnabled` at the end where `generateDDL` interleaves them —
    /// harmless, because an explicit SELECT list fixes the positions
    /// `marshal_row` reads. What must never happen is a column in one and not
    /// the other: a D23 re-dump that adds a column lands HERE first.
    #[test]
    fn all_columns_names_exactly_the_d23_dump_chats_columns() {
        let selected: BTreeSet<String> = selected_columns().into_iter().collect();
        let declared: BTreeSet<String> = schema_columns().into_iter().collect();
        let missing: Vec<&String> = declared.difference(&selected).collect();
        let extra: Vec<&String> = selected.difference(&declared).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "declared-but-unselected {missing:?}; selected-but-undeclared {extra:?}"
        );
        assert_eq!(selected_columns().len(), schema_columns().len());
    }

    /// …and every `row.get(N)` in `marshal_row` marshals the column at position
    /// N of that SELECT list. Swapping two indices reddens this.
    #[test]
    fn marshal_row_reads_each_column_at_its_own_index() {
        let cols = selected_columns();
        let pairs = key_index_pairs();
        assert_eq!(
            pairs.len(),
            cols.len(),
            "one `row.get` per selected column ({} columns, {} reads)",
            cols.len(),
            pairs.len()
        );
        let mut indices: Vec<usize> = pairs.iter().map(|(i, _)| *i).collect();
        indices.sort_unstable();
        assert_eq!(
            indices,
            (0..cols.len()).collect::<Vec<_>>(),
            "every index 0..{} read exactly once",
            cols.len()
        );
        for (index, key) in &pairs {
            assert_eq!(
                key, &cols[*index],
                "row.get({index}) is marshalled as {key:?} but SELECT position \
                 {index} is {:?}",
                cols[*index]
            );
        }
    }
}
