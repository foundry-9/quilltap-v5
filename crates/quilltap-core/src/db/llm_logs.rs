//! The llm-logs repository — the **second sibling-DB partition** of Phase 2
//! (after the mount-index family) and the **widest repo to date** (18 columns,
//! FIVE nested JSON-object columns). Ports v4's
//! `lib/database/repositories/llm-logs.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! ## What makes this repo special: the llm-logs sibling DB
//!
//! In v4 this repo overrides `getCollection()` to route all reads/writes to the
//! **dedicated llm-logs database** (`quilltap-llm-logs.db`) via
//! `getRawLLMLogsDatabase()`, isolating high-churn debug data from the main DB so
//! corruption there can never threaten characters/chats/memories. As with the
//! mount-index repos, that routing is **not** a property of the repo in the Rust
//! port — it is the file the [`super::Writer`] was opened against.
//! `Writer::open_writable` opens any ChaCha20 file by path, so a writer opened on
//! the llm-logs DB exposes this repo exactly as a main-DB writer exposes
//! `users`/`folders`. Only the harness points it at the llm-logs fixture (the
//! tier-2 case + builder target `SQLITE_LLM_LOGS_PATH` and read back through
//! `getRawLLMLogsDatabase()`).
//!
//! Scope: `create`, `update`, and `delete` (the three CRUD methods delegating to
//! the base `_create`/`_update`/`_delete`). The many `findBy*` / `countBy*` /
//! token-aggregation / cleanup helpers are out of scope.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! The **five nested JSON-object columns**, each stored as compact JSON via v4's
//! `JSON.stringify` of the Zod-parsed value — so the Rust side uses **typed
//! structs in schema field order** (never `serde_json::Value`, whose `BTreeMap`
//! would sort keys and diverge):
//!
//!   - **`request`** (`LlmLogRequestSummary`, REQUIRED) — an object containing an
//!     **array of message-summary objects** (`messages`). Its `temperature`
//!     (genuinely fractional) is the only `f64` nested field; the rest of the
//!     nested numbers are **integer-valued and modeled `i64`** so serde renders
//!     `3` (not `3.0`) to match `JSON.stringify`. `messageCount` is `i64`;
//!     `toolCount` is a Zod `.default(0)` field (always serialized, plain `i64`).
//!     The deprecated `fullMessages` is omitted from the corpus.
//!   - **`response`** (`LlmLogResponseSummary`, REQUIRED). `toolCalls` is omitted
//!     from the corpus (avoids the open-JSON `arguments` seam).
//!   - **`usage`** (`Option<LlmLogTokenUsage>`) — nullable JSON; `None` → SQL NULL.
//!   - **`cacheUsage`** (`Option<LlmLogCacheUsage>`) — nullable JSON.
//!   - **`rawProviderUsage`** (`Option<serde_json::Value>`) — the one **open-JSON**
//!     column (`z.record`). serde_json sorts object keys vs v4's insertion-order
//!     `JSON.stringify`, so the corpus is **constrained to null / `{}` /
//!     single-key only** (tracked open-JSON seam #5).
//!   - **`requestHashes`** (`Option<LlmLogRequestHashes>`) — nullable JSON.
//!
//! Plus: a `durationMs` **REAL column, nullable** (`Option<f64>`; an integer-valued
//! ms value collapses back to a JSON integer in the dump via
//! [`super::js_number_to_json`], matching v4), the `type` **enum TEXT** column
//! (`LLMLogTypeEnum`, 18 variants), four **nullable UUID-as-TEXT** columns
//! (`messageId` / `chatId` / `characterId` / `autonomousRunId`), and the plain
//! `provider` / `modelName` / `userId` strings.
//!
//! ### Nested-JSON rendering rules (must match v4 byte-for-byte)
//!
//! All nested structs carry `#[serde(rename_all = "camelCase")]` with fields in
//! SCHEMA field order. Zod `.optional()` (no default) nested fields are
//! `Option<T>` with `skip_serializing_if = "Option::is_none"` (the CORPUS omits
//! them, so "absent" == "omitted" on both sides). Integer-valued nested numbers
//! are `i64` (serde `3` matches `JSON.stringify(3)`); the only genuinely
//! fractional field, `temperature`, is `f64` and serializes via
//! [`js_number_to_json`] (an integer-valued float bare — `JSON.stringify(1.0)` =
//! `"1"` — a whole-number temperature is common on the CHAT_MESSAGE log path,
//! W4.11b). Zod `.default(X)`
//! fields (`hasAttachments` false, `toolCount` 0) are modeled plain (non-Option)
//! and always provided by the corpus.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form the prior
//! Phase-2 repos use.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::DbError;

// ============================================================================
// Nested JSON-object structs (schema field order; compact serde JSON)
// ============================================================================

/// One message summary — an element of `request.messages`
/// (`LLMLogMessageSummarySchema` field order: role, content, contentPreview,
/// contentLength, hasAttachments). `contentPreview` is `.optional()` → omit in
/// corpus; `contentLength` is integer-valued → `i64`; `hasAttachments` is a
/// `.default(false)` field, always serialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLogMessageSummary {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
    pub content_length: i64,
    pub has_attachments: bool,
}

/// The `request` JSON object (`LLMLogRequestSummarySchema` field order:
/// messageCount, messages, temperature, maxTokens, toolCount, fullMessages).
/// `temperature` is the only genuinely-fractional nested number (`f64`,
/// serialized via [`ser_double_opt_jsnum`] so a whole number renders bare — `1`,
/// not `1.0` — matching v4's `JSON.stringify`); `maxTokens` is integer-valued
/// (`i64`); `toolCount` is `.default(0)` (plain `i64`); `fullMessages`
/// (deprecated) is omitted in the corpus.
///
/// `temperature`/`maxTokens` are v4 `.nullable().optional()` — the same
/// present-null-vs-absent double-`Option` as `response`'s `error`/`finishReason`.
/// v4's `summarizeRequest` ALWAYS sets them (`request.temperature ?? null`), so
/// the SUMMARIZE path stores them **present as `null`** (`Some(None)`) when
/// absent; a raw tier-2 write with the key absent stores them **absent** (`None`,
/// skipped).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLogRequestSummary {
    pub message_count: i64,
    pub messages: Vec<LlmLogMessageSummary>,
    #[serde(
        default,
        deserialize_with = "de_double_opt",
        serialize_with = "ser_double_opt_jsnum",
        skip_serializing_if = "Option::is_none"
    )]
    pub temperature: Option<Option<f64>>,
    #[serde(
        default,
        deserialize_with = "de_double_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_tokens: Option<Option<i64>>,
    pub tool_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_messages: Option<Value>,
}

/// The `response` JSON object (`LLMLogResponseSummarySchema` field order:
/// content, contentPreview, contentLength, fullContent, error, finishReason,
/// toolCalls). `contentPreview`/`fullContent`/`toolCalls` are plain `.optional()`
/// (omitted-when-None); `toolCalls` is kept out of the corpus (avoids the
/// open-JSON `arguments` seam).
///
/// `error`/`finishReason` are v4 `.nullable().optional()`, so they carry the
/// **present-null-vs-absent** distinction (the double-`Option` pattern, like
/// `chats`' `removedAt`). v4's `summarizeResponse` ALWAYS sets them
/// (`response.error ?? null`), so the SUMMARIZE path stores them **present as
/// `null`** (`Some(None)`); a raw tier-2 write with the key absent stores them
/// **absent** (`None`, skipped). The double-option deserializer forces
/// present→`Some(_)` so a stored `null` round-trips as `Some(None)` rather than
/// collapsing to the outer `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLogResponseSummary {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_preview: Option<String>,
    pub content_length: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_content: Option<String>,
    #[serde(
        default,
        deserialize_with = "de_double_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub error: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "de_double_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub finish_reason: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmLogToolCall>>,
}

/// Double-option deserializer for the `.nullable().optional()` summary fields
/// (`error`/`finishReason`, `temperature`/`maxTokens`): a PRESENT field (even
/// JSON `null`) becomes `Some(_)`; an ABSENT field falls to `#[serde(default)]`
/// `None`. Serialization is serde-default (`Some(None)` → `null`, `Some(Some)` →
/// the value, `None` skipped). Generic over the inner type so `String`/`f64`/
/// `i64` all share it.
fn de_double_opt<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(de)?))
}

/// Serialize a `.nullable().optional()` **fractional** field the way v4's
/// `JSON.stringify` renders a JS number: an integer-valued float bare (`1`, not
/// `1.0`) via [`super::js_number_to_json`], a genuine fraction as-is; `Some(None)`
/// → `null`; `None` is skipped by `skip_serializing_if`. Closes the `temperature`
/// seam (W4.11b — the CHAT_MESSAGE log routinely carries a whole-number
/// temperature, e.g. `1.0`, which must store as `"1"`).
fn ser_double_opt_jsnum<S>(v: &Option<Option<f64>>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match v {
        Some(Some(f)) => super::js_number_to_json(*f).serialize(s),
        _ => s.serialize_none(),
    }
}

/// One native tool call — an element of `response.toolCalls` (omitted in the
/// corpus, so `arguments` never serializes; modeled for completeness only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLogToolCall {
    pub name: String,
    pub arguments: Value,
}

/// The `usage` JSON object (`LLMLogTokenUsageSchema` field order: promptTokens,
/// completionTokens, totalTokens). All integer-valued → `i64`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLogTokenUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// The `cacheUsage` JSON object (`LLMLogCacheUsageSchema` field order:
/// cacheCreationInputTokens, cacheReadInputTokens). Both `.optional()` →
/// `Option<i64>`, omitted-when-None.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLogCacheUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i64>,
}

/// The `requestHashes` JSON object (`LLMLogRequestHashesSchema` field order:
/// systemBlock1Hash, systemBlock2Hash, systemBlock3Hash, toolsArrayHash,
/// historyTailHash). All `.optional()` → `Option<String>`, omitted-when-None.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmLogRequestHashes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_block1_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_block2_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_block3_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools_array_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_tail_hash: Option<String>,
}

// ============================================================================
// Create / update inputs
// ============================================================================

/// Fields for creating an llm-log (the `Omit<LLMLog,'id'|timestamps>` shape), in
/// `LLMLogSchema` field order. The five nested-JSON columns are typed structs
/// (`request`/`response` required, the rest `Option` → SQL NULL when `None`);
/// `raw_provider_usage` is the one open-JSON column (constrained to null / `{}` /
/// single-key); `duration_ms` is the nullable REAL column.
pub struct LlCreate {
    pub user_id: String,
    /// `LLMLogTypeEnum` → TEXT (18 variants).
    pub log_type: String,
    /// `None` => SQL NULL.
    pub message_id: Option<String>,
    /// `None` => SQL NULL.
    pub chat_id: Option<String>,
    /// `None` => SQL NULL.
    pub character_id: Option<String>,
    /// `None` => SQL NULL.
    pub autonomous_run_id: Option<String>,
    pub provider: String,
    pub model_name: String,
    /// Required nested JSON object → compact JSON text.
    pub request: LlmLogRequestSummary,
    /// Required nested JSON object → compact JSON text.
    pub response: LlmLogResponseSummary,
    /// `None` => SQL NULL; `Some` => compact JSON.
    pub usage: Option<LlmLogTokenUsage>,
    /// `None` => SQL NULL; `Some` => compact JSON.
    pub cache_usage: Option<LlmLogCacheUsage>,
    /// `None` => SQL NULL; `Some` => compact JSON. Open-JSON: keep null / `{}` /
    /// single-key only (serde_json sorts keys; seam #5).
    pub raw_provider_usage: Option<Value>,
    /// `None` => SQL NULL; `Some` => compact JSON.
    pub request_hashes: Option<LlmLogRequestHashes>,
    /// `None` => SQL NULL; `Some` => REAL (integer-valued collapses in the dump).
    pub duration_ms: Option<f64>,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// An llm-log update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved, `updatedAt` is set explicitly. Each
/// `Some` field sets that column; for a nested-object column the patch carries the
/// whole new object (re-serialized).
#[derive(Default)]
pub struct LlUpdate {
    pub log_type: Option<String>,
    pub message_id: Option<String>,
    pub chat_id: Option<String>,
    pub character_id: Option<String>,
    pub autonomous_run_id: Option<String>,
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub request: Option<LlmLogRequestSummary>,
    pub response: Option<LlmLogResponseSummary>,
    pub usage: Option<LlmLogTokenUsage>,
    pub cache_usage: Option<LlmLogCacheUsage>,
    pub raw_provider_usage: Option<Value>,
    pub request_hashes: Option<LlmLogRequestHashes>,
    pub duration_ms: Option<f64>,
    pub updated_at: String,
}

/// The subset of an `llm_logs` row `self_inventory`'s lastTurn section reads: the
/// provider/model plus the parsed `usage` token counts and the log timestamp.
#[derive(Clone, Debug)]
pub struct LastTurnLog {
    pub provider: String,
    pub model_name: String,
    /// `None` when the log's `usage` column was NULL.
    pub usage: Option<LlmLogTokenUsage>,
    pub created_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct LLMLogsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> LLMLogsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert an llm-log with the given pinned id + timestamps. All 18 columns are
    /// written in `LLMLogSchema` field order; nested-JSON columns via
    /// `serde_json::to_string`, nullable JSON/REAL/UUID columns via `Option`.
    pub fn create(&self, data: &LlCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let request_json = serde_json::to_string(&data.request)
            .map_err(|e| DbError::Key(format!("request serialize: {e}")))?;
        let response_json = serde_json::to_string(&data.response)
            .map_err(|e| DbError::Key(format!("response serialize: {e}")))?;
        let usage_json = opt_json(&data.usage, "usage")?;
        let cache_usage_json = opt_json(&data.cache_usage, "cacheUsage")?;
        let raw_provider_usage_json = opt_json(&data.raw_provider_usage, "rawProviderUsage")?;
        let request_hashes_json = opt_json(&data.request_hashes, "requestHashes")?;

        self.conn.execute(
            "INSERT INTO llm_logs \
               (id, userId, type, messageId, chatId, characterId, autonomousRunId, \
                provider, modelName, request, response, usage, cacheUsage, \
                rawProviderUsage, requestHashes, durationMs, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                opts.id,
                data.user_id,
                data.log_type,
                data.message_id,
                data.chat_id,
                data.character_id,
                data.autonomous_run_id,
                data.provider,
                data.model_name,
                request_json,
                response_json,
                usage_json,
                cache_usage_json,
                raw_provider_usage_json,
                request_hashes_json,
                data.duration_ms,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// v4 `llmLogs.findByChatId(chatId)[0]` — the **most recent** log for a chat.
    /// v4 sorts `createdAt DESC` and the `self_inventory` lastTurn section takes the
    /// head; we push the same ORDER BY into SQL and `LIMIT 1`. Returns only the
    /// fields the section reads: `provider` / `modelName` / the parsed `usage`
    /// token counts / `createdAt`. `None` when the chat has no logs. The full
    /// LLMLog marshaling is deferred (no consumer needs more columns).
    ///
    /// The ORDER BY is v4's exact translated clause (`ORDER BY "createdAt" DESC`,
    /// no secondary key — see `query-translator.ts::translateSort`). The same
    /// SQLite engine runs both sides, so distinct `createdAt` values (which the
    /// fixture guarantees) make the head deterministic and byte-identical.
    pub fn find_last_by_chat_id(&self, chat_id: &str) -> Result<Option<LastTurnLog>, DbError> {
        let row = self
            .conn
            .query_row(
                "SELECT provider, modelName, usage, createdAt \
                   FROM llm_logs WHERE chatId = ?1 \
                  ORDER BY \"createdAt\" DESC LIMIT 1",
                params![chat_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        let Some((provider, model_name, usage_json, created_at)) = row else {
            return Ok(None);
        };

        // v4 `usage?.promptTokens ?? null` — parse the nested object when present.
        let usage = match usage_json {
            Some(s) => serde_json::from_str::<LlmLogTokenUsage>(&s)
                .map(Some)
                .map_err(|e| DbError::Key(format!("usage parse: {e}")))?,
            None => None,
        };

        Ok(Some(LastTurnLog {
            provider,
            model_name,
            usage,
            created_at,
        }))
    }

    /// Apply an update patch to the log `id`. Returns `Ok(false)` when no row
    /// matched (v4's "not found -> null"). id and createdAt are never touched;
    /// `updatedAt` is always set. Nested-object columns are re-serialized when
    /// provided.
    pub fn update(&self, id: &str, patch: &LlUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        let exists: bool = self
            .conn
            .query_row("SELECT 1 FROM llm_logs WHERE id = ?1", params![id], |_| {
                Ok(())
            })
            .map(|_| true)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other),
            })?;
        if !exists {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(log_type) = &patch.log_type {
            assignments.push(format!("type = ?{}", values.len() + 1));
            values.push(Box::new(log_type.clone()));
        }
        if let Some(message_id) = &patch.message_id {
            assignments.push(format!("messageId = ?{}", values.len() + 1));
            values.push(Box::new(message_id.clone()));
        }
        if let Some(chat_id) = &patch.chat_id {
            assignments.push(format!("chatId = ?{}", values.len() + 1));
            values.push(Box::new(chat_id.clone()));
        }
        if let Some(character_id) = &patch.character_id {
            assignments.push(format!("characterId = ?{}", values.len() + 1));
            values.push(Box::new(character_id.clone()));
        }
        if let Some(autonomous_run_id) = &patch.autonomous_run_id {
            assignments.push(format!("autonomousRunId = ?{}", values.len() + 1));
            values.push(Box::new(autonomous_run_id.clone()));
        }
        if let Some(provider) = &patch.provider {
            assignments.push(format!("provider = ?{}", values.len() + 1));
            values.push(Box::new(provider.clone()));
        }
        if let Some(model_name) = &patch.model_name {
            assignments.push(format!("modelName = ?{}", values.len() + 1));
            values.push(Box::new(model_name.clone()));
        }
        if let Some(request) = &patch.request {
            let json = serde_json::to_string(request)
                .map_err(|e| DbError::Key(format!("request serialize: {e}")))?;
            assignments.push(format!("request = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(response) = &patch.response {
            let json = serde_json::to_string(response)
                .map_err(|e| DbError::Key(format!("response serialize: {e}")))?;
            assignments.push(format!("response = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(usage) = &patch.usage {
            let json = serde_json::to_string(usage)
                .map_err(|e| DbError::Key(format!("usage serialize: {e}")))?;
            assignments.push(format!("usage = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(cache_usage) = &patch.cache_usage {
            let json = serde_json::to_string(cache_usage)
                .map_err(|e| DbError::Key(format!("cacheUsage serialize: {e}")))?;
            assignments.push(format!("cacheUsage = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(raw_provider_usage) = &patch.raw_provider_usage {
            let json = serde_json::to_string(raw_provider_usage)
                .map_err(|e| DbError::Key(format!("rawProviderUsage serialize: {e}")))?;
            assignments.push(format!("rawProviderUsage = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(request_hashes) = &patch.request_hashes {
            let json = serde_json::to_string(request_hashes)
                .map_err(|e| DbError::Key(format!("requestHashes serialize: {e}")))?;
            assignments.push(format!("requestHashes = ?{}", values.len() + 1));
            values.push(Box::new(json));
        }
        if let Some(duration_ms) = &patch.duration_ms {
            assignments.push(format!("durationMs = ?{}", values.len() + 1));
            values.push(Box::new(*duration_ms));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE llm_logs SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the log `id`. Returns `false` when no row matched (v4's `_delete`
    /// "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM llm_logs WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// v4 `getTotalTokenUsageForRun(autonomousRunId, { includeCacheHits })` —
    /// the per-run token sum the autonomous-room turn handler's post-turn
    /// accounting reads (U4.4). Sums every row tagged with this run's id: the
    /// run's conversational turns plus their agent-mode tool sub-calls.
    ///
    /// The filter is `{ autonomousRunId, usage: { $exists: true } }` — v4
    /// DELIBERATELY drops `$ne: null` here (see the long comment on v4's
    /// `getTotalTokenUsageForChatSince`): the SQLite translator would emit
    /// `usage != NULL`, which is unknown for every row and zeroes the sum.
    /// `$exists: true` translates to `usage IS NOT NULL`, reproduced verbatim.
    ///
    /// Per-row: `usage.promptTokens || 0` etc. (JS falsy — a 0/null/absent field
    /// contributes nothing). When `include_cache_hits` and the row's
    /// `cacheUsage.cacheReadInputTokens` is TRUTHY (0 does not add), the cache
    /// reads the provider plugins stripped from `usage` are added back into
    /// prompt + total (the "count every token" budget mode, per-room
    /// `budgetExcludeCacheHits = 0`).
    ///
    /// v4 wraps this in `safeQuery` with a `{0,0,0}` default — a read/parse
    /// failure returns zeros rather than erroring; reproduced here (a malformed
    /// `usage` cell zeroes the whole sum like v4's repo-level catch).
    pub fn get_total_token_usage_for_run(
        &self,
        autonomous_run_id: &str,
        include_cache_hits: bool,
    ) -> TokenUsageTotals {
        self.sum_usage_rows(
            "SELECT usage, cacheUsage FROM llm_logs \
              WHERE \"autonomousRunId\" = ?1 AND \"usage\" IS NOT NULL",
            params![autonomous_run_id],
            include_cache_hits,
        )
        .unwrap_or_default()
    }

    /// v4 `getTotalTokenUsageSince(userId, sinceTimestamp)` — the daily
    /// user-token spend read the autonomous-room budget gate consults.
    ///
    /// ⚠️ **BROKEN-BUT-EXACT** (the `memories` `$regex` precedent). v4's filter
    /// is `{ userId, usage: { $exists: true, $ne: null }, createdAt: { $gte } }`
    /// and the SQLite translator lowers `$ne: null` to `"usage" != ?` with a
    /// NULL bind — `usage != NULL` is UNKNOWN for every row under SQL NULL
    /// semantics, so the filter matches NOTHING and the sum is **always
    /// {0,0,0}** on v4-on-SQLite. Empirically pinned (U4.4 A-probe,
    /// 2026-07-08): four rows carrying 1,665 total tokens summed to 0 through
    /// v4's REAL repo, while `getTotalTokenUsageForRun` (no `$ne:null`) summed
    /// correctly. Consequence: the autonomous daily-token-budget gates NEVER
    /// bind through actual spend in v4-on-SQLite (`dailyTokensSpent` is always
    /// 0, and `dailyTokenBudget` is schema-`positive()` so `0 >= budget` never
    /// holds) — the daily-pause branches are dead code, banked as such by the
    /// U4.4 capstone differential. The translated SQL is executed verbatim
    /// (NULL bind included) rather than short-circuited, so the shape stays
    /// faithful to v4's query.
    pub fn get_total_token_usage_since(
        &self,
        user_id: &str,
        since_timestamp: &str,
    ) -> TokenUsageTotals {
        self.sum_usage_rows(
            "SELECT usage, cacheUsage FROM llm_logs \
              WHERE \"userId\" = ?1 AND \"usage\" IS NOT NULL AND \"usage\" != ?2 \
                AND \"createdAt\" >= ?3",
            params![user_id, rusqlite::types::Null, since_timestamp],
            false,
        )
        .unwrap_or_default()
    }

    /// The shared summation loop over `(usage, cacheUsage)` JSON cells —
    /// v4's `for (const log of logs)` body in the `getTotalTokenUsage*` family.
    fn sum_usage_rows(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
        include_cache_hits: bool,
    ) -> Result<TokenUsageTotals, DbError> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params, |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })?;

        let mut totals = TokenUsageTotals::default();
        for row in rows {
            let (usage_json, cache_json) = row?;
            // `if (log.usage)` — the SQL filter already guarantees non-NULL, but
            // keep the guard for shape fidelity (a JSON `null` cell is falsy too).
            if let Some(usage) = usage_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .filter(|v| !v.is_null())
            {
                // `usage.promptTokens || 0` — JS falsy (0 / null / absent) → 0.
                totals.prompt_tokens += js_number_or_zero(usage.get("promptTokens"));
                totals.completion_tokens += js_number_or_zero(usage.get("completionTokens"));
                totals.total_tokens += js_number_or_zero(usage.get("totalTokens"));
            }
            // `includeCacheHits && log.cacheUsage?.cacheReadInputTokens` — the
            // TRUTHY test means a 0 adds nothing (faithful).
            if include_cache_hits {
                let cache_reads = cache_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .and_then(|v| v.get("cacheReadInputTokens").and_then(Value::as_f64))
                    .unwrap_or(0.0);
                if cache_reads != 0.0 {
                    totals.prompt_tokens += cache_reads;
                    totals.total_tokens += cache_reads;
                }
            }
        }
        Ok(totals)
    }
}

/// The `{ promptTokens, completionTokens, totalTokens }` sum the
/// `getTotalTokenUsage*` family returns. JS numbers (the stored token counts are
/// integers; f64 keeps the arithmetic identical to v4's).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TokenUsageTotals {
    pub prompt_tokens: f64,
    pub completion_tokens: f64,
    pub total_tokens: f64,
}

/// JS `x || 0` over a JSON number field: `null` / absent / non-number / `0` → 0.
fn js_number_or_zero(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64).unwrap_or(0.0)
}

/// Serialize an optional value to compact JSON text, or `None` for SQL NULL.
fn opt_json<T: Serialize>(value: &Option<T>, label: &str) -> Result<Option<String>, DbError> {
    match value {
        Some(v) => {
            Ok(Some(serde_json::to_string(v).map_err(|e| {
                DbError::Key(format!("{label} serialize: {e}"))
            })?))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The columns the usage reads touch (the tier-2 differential runs on the real
    // generated schema; these unit tests only need the summation mechanics).
    const DDL: &str = "CREATE TABLE llm_logs (\
        id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, chatId TEXT, \
        characterId TEXT, autonomousRunId TEXT, provider TEXT, modelName TEXT, \
        request TEXT, response TEXT, usage TEXT, cacheUsage TEXT, \
        rawProviderUsage TEXT, requestHashes TEXT, durationMs REAL, \
        createdAt TEXT, updatedAt TEXT);";

    fn conn_with_rows() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(DDL).unwrap();
        let insert = |id: &str,
                      user: &str,
                      run: Option<&str>,
                      usage: Option<&str>,
                      cache: Option<&str>,
                      created: &str| {
            conn.execute(
                "INSERT INTO llm_logs (id, userId, type, autonomousRunId, provider, modelName, \
                 request, response, usage, cacheUsage, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'CHAT_MESSAGE', ?3, 'ANTHROPIC', 'm', '{}', '{}', ?4, ?5, ?6, ?6)",
                params![id, user, run, usage, cache, created],
            )
            .unwrap();
        };
        // Tagged with the run, plain usage.
        insert(
            "r1",
            "u1",
            Some("run-1"),
            Some(r#"{"promptTokens":100,"completionTokens":50,"totalTokens":150}"#),
            None,
            "2026-07-08T00:00:01.000Z",
        );
        // Tagged, with cache reads.
        insert(
            "r2",
            "u1",
            Some("run-1"),
            Some(r#"{"promptTokens":10,"completionTokens":5,"totalTokens":15}"#),
            Some(r#"{"cacheReadInputTokens":7}"#),
            "2026-07-08T00:00:02.000Z",
        );
        // Tagged, ZERO cache reads (JS truthy test — adds nothing).
        insert(
            "r3",
            "u1",
            Some("run-1"),
            Some(r#"{"promptTokens":1,"completionTokens":1,"totalTokens":2}"#),
            Some(r#"{"cacheReadInputTokens":0}"#),
            "2026-07-08T00:00:03.000Z",
        );
        // Tagged but usage NULL — filtered by `usage IS NOT NULL`.
        insert(
            "r4",
            "u1",
            Some("run-1"),
            None,
            None,
            "2026-07-08T00:00:04.000Z",
        );
        // A different run.
        insert(
            "r5",
            "u1",
            Some("run-2"),
            Some(r#"{"promptTokens":1000,"completionTokens":500,"totalTokens":1500}"#),
            None,
            "2026-07-08T00:00:05.000Z",
        );
        // Untagged user spend (would count toward a WORKING daily sum).
        insert(
            "r6",
            "u1",
            None,
            Some(r#"{"promptTokens":2000,"completionTokens":1000,"totalTokens":3000}"#),
            None,
            "2026-07-08T00:00:06.000Z",
        );
        conn
    }

    #[test]
    fn run_sum_excludes_cache_hits_by_default() {
        let conn = conn_with_rows();
        let repo = LLMLogsRepository::new(&conn);
        let t = repo.get_total_token_usage_for_run("run-1", false);
        assert_eq!(t.prompt_tokens, 111.0);
        assert_eq!(t.completion_tokens, 56.0);
        assert_eq!(t.total_tokens, 167.0);
    }

    #[test]
    fn run_sum_adds_truthy_cache_reads_when_included() {
        let conn = conn_with_rows();
        let repo = LLMLogsRepository::new(&conn);
        let t = repo.get_total_token_usage_for_run("run-1", true);
        // r2's 7 cache reads add to prompt+total; r3's ZERO cache reads add
        // nothing (v4's `cacheUsage?.cacheReadInputTokens` truthy test).
        assert_eq!(t.prompt_tokens, 118.0);
        assert_eq!(t.completion_tokens, 56.0);
        assert_eq!(t.total_tokens, 174.0);
    }

    #[test]
    fn run_sum_unknown_run_is_zero() {
        let conn = conn_with_rows();
        let repo = LLMLogsRepository::new(&conn);
        assert_eq!(
            repo.get_total_token_usage_for_run("no-such-run", true),
            TokenUsageTotals::default()
        );
    }

    /// The BROKEN-BUT-EXACT daily read: rows with real usage exist since the
    /// timestamp, and the sum is still zero (the `$ne: null` → `usage != NULL`
    /// translator bug, empirically pinned against v4 — see the method docs).
    #[test]
    fn since_sum_is_always_zero_on_sqlite() {
        let conn = conn_with_rows();
        let repo = LLMLogsRepository::new(&conn);
        let t = repo.get_total_token_usage_since("u1", "2000-01-01T00:00:00.000Z");
        assert_eq!(t, TokenUsageTotals::default());
        // Sanity: the same rows DO sum through the run read (the filter is the
        // only difference).
        assert_ne!(
            repo.get_total_token_usage_for_run("run-1", false),
            TokenUsageTotals::default()
        );
    }

    /// A read against a missing table returns the safeQuery default (zeros).
    #[test]
    fn safe_query_default_on_error() {
        let conn = Connection::open_in_memory().unwrap();
        let repo = LLMLogsRepository::new(&conn);
        assert_eq!(
            repo.get_total_token_usage_for_run("run-1", false),
            TokenUsageTotals::default()
        );
        assert_eq!(
            repo.get_total_token_usage_since("u1", "2000-01-01T00:00:00.000Z"),
            TokenUsageTotals::default()
        );
    }
}
