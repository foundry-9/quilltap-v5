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

use rusqlite::{Connection, Row};
use serde_json::{Map, Value};

use super::chats::ChatParticipant;
use super::js_number_to_json;
use super::DbError;

/// All 96 columns, in `ChatMetadataBaseSchema` field order (= DDL / SELECT order).
const ALL_COLUMNS: &str = "id, userId, participants, title, contextSummary, sillyTavernMetadata, \
     tags, roleplayTemplateId, timestampConfig, lastTurnParticipantId, messageCount, lastMessageAt, \
     lastRenameCheckInterchange, compactionGeneration, lastSummaryTurn, lastSummaryTokens, \
     lastFullRebuildTurn, summaryAnchorMessageIds, isPaused, isManuallyRenamed, \
     impersonatingParticipantIds, activeTypingParticipantId, allLLMPauseTurnCount, turnQueue, \
     spokenThisCycleParticipantIds, documentEditingMode, documentMode, dividerPosition, terminalMode, \
     activeTerminalSessionId, rightPaneVerticalSplit, projectId, scenarioText, totalPromptTokens, \
     totalCompletionTokens, estimatedCostUSD, priceSource, showSystemEventsOverride, \
     requestFullContextOnNextMessage, disabledTools, disabledToolGroups, forceToolsOnNextMessage, \
     allowCrossCharacterVaultReads, pendingOutfitNotifications, state, compressionCache, \
     agentModeEnabled, agentTurnCount, storyBackgroundImageId, lastBackgroundGeneratedAt, \
     imageProfileId, alertCharactersOfLanternImages, isDangerousChat, dangerScore, dangerCategories, \
     dangerClassifiedAt, dangerClassifiedAtMessageCount, conciergeOverride, sceneState, \
     renderedMarkdown, equippedOutfit, characterAvatars, avatarGenerationEnabled, chatType, \
     helpPageUrl, consoleConnectionProfileId, compiledIdentityStacks, courierCheckpoints, \
     commonplaceSceneCache, commonplaceRecallHistory, budgetMaxTurns, budgetMaxTokens, \
     budgetMaxWallClockMs, budgetEstimatedSpendCapUSD, scheduleCron, scheduleFreshnessWindowMs, \
     scheduleNextRunAt, scheduleLastRunAt, runState, currentRunId, runStateMessage, runStartedAt, \
     runEndedAt, runPausedAt, runPausedAccumMs, runTurnsConsumed, runTokensConsumed, \
     runMilestonesAnnounced, runDestructiveToolsAllowed, budgetExcludeCacheHits, runVisibility, \
     coreWhisperEnabled, coreWhisperInterval, showThinking, createdAt, updatedAt, \
     answerConfirmationOverride";

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
    obj.insert(
        "documentEditingMode".into(),
        Value::Bool(row.get::<_, Option<i64>>(25)?.unwrap_or(0) == 1),
    );
    obj.insert(
        "documentMode".into(),
        Value::String(
            row.get::<_, Option<String>>(26)?
                .unwrap_or_else(|| "normal".into()),
        ),
    );
    obj.insert("dividerPosition".into(), number_or(row.get(27)?, 45.0));
    obj.insert(
        "terminalMode".into(),
        Value::String(
            row.get::<_, Option<String>>(28)?
                .unwrap_or_else(|| "normal".into()),
        ),
    );
    put_opt_string(&mut obj, "activeTerminalSessionId", row.get(29)?);
    obj.insert(
        "rightPaneVerticalSplit".into(),
        number_or(row.get(30)?, 50.0),
    );
    put_opt_string(&mut obj, "projectId", row.get(31)?);
    put_opt_string(&mut obj, "scenarioText", row.get(32)?);
    obj.insert("totalPromptTokens".into(), number_or(row.get(33)?, 0.0));
    obj.insert("totalCompletionTokens".into(), number_or(row.get(34)?, 0.0));
    put_opt_number(&mut obj, "estimatedCostUSD", row.get(35)?);
    put_opt_string(&mut obj, "priceSource", row.get(36)?);
    put_opt_bool(&mut obj, "showSystemEventsOverride", row.get(37)?);
    obj.insert(
        "requestFullContextOnNextMessage".into(),
        Value::Bool(row.get::<_, Option<i64>>(38)?.unwrap_or(0) == 1),
    );
    obj.insert("disabledTools".into(), array_or_empty(row.get(39)?));
    obj.insert("disabledToolGroups".into(), array_or_empty(row.get(40)?));
    obj.insert(
        "forceToolsOnNextMessage".into(),
        Value::Bool(row.get::<_, Option<i64>>(41)?.unwrap_or(0) == 1),
    );
    obj.insert(
        "allowCrossCharacterVaultReads".into(),
        Value::Bool(row.get::<_, Option<i64>>(42)?.unwrap_or(0) == 1),
    );
    put_opt_json(&mut obj, "pendingOutfitNotifications", row.get(43)?);
    obj.insert("state".into(), object_or_empty(row.get(44)?));
    put_opt_json(&mut obj, "compressionCache", row.get(45)?);
    put_opt_bool(&mut obj, "agentModeEnabled", row.get(46)?);
    obj.insert("agentTurnCount".into(), number_or(row.get(47)?, 0.0));
    put_opt_string(&mut obj, "storyBackgroundImageId", row.get(48)?);
    put_opt_string(&mut obj, "lastBackgroundGeneratedAt", row.get(49)?);
    put_opt_string(&mut obj, "imageProfileId", row.get(50)?);
    put_opt_bool(&mut obj, "alertCharactersOfLanternImages", row.get(51)?);
    put_opt_bool(&mut obj, "isDangerousChat", row.get(52)?);
    put_opt_number(&mut obj, "dangerScore", row.get(53)?);
    obj.insert("dangerCategories".into(), array_or_empty(row.get(54)?));
    put_opt_string(&mut obj, "dangerClassifiedAt", row.get(55)?);
    put_opt_number(&mut obj, "dangerClassifiedAtMessageCount", row.get(56)?);
    put_opt_string(&mut obj, "conciergeOverride", row.get(57)?);
    put_opt_json(&mut obj, "sceneState", row.get(58)?);
    put_opt_string(&mut obj, "renderedMarkdown", row.get(59)?);
    put_opt_json(&mut obj, "equippedOutfit", row.get(60)?);
    put_opt_json(&mut obj, "characterAvatars", row.get(61)?);
    put_opt_bool(&mut obj, "avatarGenerationEnabled", row.get(62)?);
    obj.insert(
        "chatType".into(),
        Value::String(
            row.get::<_, Option<String>>(63)?
                .unwrap_or_else(|| "salon".into()),
        ),
    );
    put_opt_string(&mut obj, "helpPageUrl", row.get(64)?);
    put_opt_string(&mut obj, "consoleConnectionProfileId", row.get(65)?);
    put_opt_json(&mut obj, "compiledIdentityStacks", row.get(66)?);
    put_opt_json(&mut obj, "courierCheckpoints", row.get(67)?);
    put_opt_json(&mut obj, "commonplaceSceneCache", row.get(68)?);
    put_opt_json(&mut obj, "commonplaceRecallHistory", row.get(69)?);
    put_opt_number(&mut obj, "budgetMaxTurns", row.get(70)?);
    put_opt_number(&mut obj, "budgetMaxTokens", row.get(71)?);
    put_opt_number(&mut obj, "budgetMaxWallClockMs", row.get(72)?);
    put_opt_number(&mut obj, "budgetEstimatedSpendCapUSD", row.get(73)?);
    put_opt_string(&mut obj, "scheduleCron", row.get(74)?);
    put_opt_number(&mut obj, "scheduleFreshnessWindowMs", row.get(75)?);
    put_opt_string(&mut obj, "scheduleNextRunAt", row.get(76)?);
    put_opt_string(&mut obj, "scheduleLastRunAt", row.get(77)?);
    put_opt_string(&mut obj, "runState", row.get(78)?);
    put_opt_string(&mut obj, "currentRunId", row.get(79)?);
    put_opt_string(&mut obj, "runStateMessage", row.get(80)?);
    put_opt_string(&mut obj, "runStartedAt", row.get(81)?);
    put_opt_string(&mut obj, "runEndedAt", row.get(82)?);
    put_opt_string(&mut obj, "runPausedAt", row.get(83)?);
    put_opt_number(&mut obj, "runPausedAccumMs", row.get(84)?);
    put_opt_number(&mut obj, "runTurnsConsumed", row.get(85)?);
    put_opt_number(&mut obj, "runTokensConsumed", row.get(86)?);
    obj.insert(
        "runMilestonesAnnounced".into(),
        number_or(row.get(87)?, 0.0),
    );
    obj.insert(
        "runDestructiveToolsAllowed".into(),
        number_or(row.get(88)?, 0.0),
    );
    obj.insert(
        "budgetExcludeCacheHits".into(),
        number_or(row.get(89)?, 1.0),
    );
    put_opt_string(&mut obj, "runVisibility", row.get(90)?);
    put_opt_bool(&mut obj, "coreWhisperEnabled", row.get(91)?);
    put_opt_number(&mut obj, "coreWhisperInterval", row.get(92)?);
    put_opt_bool(&mut obj, "showThinking", row.get(93)?);
    obj.insert("createdAt".into(), Value::String(row.get::<_, String>(94)?));
    obj.insert("updatedAt".into(), Value::String(row.get::<_, String>(95)?));
    put_opt_string(&mut obj, "answerConfirmationOverride", row.get(96)?);

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
