//! LLM logging service — v4 `lib/services/llm-logging.service.ts` — the standing
//! host-side deferral, now closed. Writes one row per LLM call into the llm-logs
//! sibling DB (via the ported [`crate::db::llm_logs`] repo), summarizing the
//! request/response and stamping the cache-prefix hashes, respecting the user's
//! `llmLoggingSettings.enabled`.
//!
//! ## The autonomous-run id (no thread-locals)
//!
//! v4 threads the autonomous-run id via `AsyncLocalStorage`
//! (`autonomous-run-context.ts`) so any `llm_logs` row written while an autonomous
//! turn is on the stack is stamped with it. The Rust core is scheduler-free and
//! must not carry ambient task-local state, so the run id is an **explicit**
//! [`LogContext`] field the caller passes: the enclave / job runner supplies it
//! when Unit 4 lands; every request-path caller passes [`LogContext::none`].
//!
//! ## Never throws
//!
//! Faithful to v4: a settings-read error defaults to **enabled** (still logs), and
//! any failure building or writing the row is swallowed — [`log_llm_call`] returns
//! `None` rather than propagating. Logging must never break the main flow.

use serde_json::Value;

use crate::db::chat_settings::LlmLoggingSettings;
use crate::db::llm_logs::{
    CreateOptions, LlCreate, LlmLogCacheUsage, LlmLogMessageSummary, LlmLogRequestHashes,
    LlmLogRequestSummary, LlmLogResponseSummary, LlmLogTokenUsage, LlmLogToolCall,
};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::jsstr::utf16_len;

/// v4 `DEFAULT_LOGGING_SETTINGS` — logging on, 30-day retention.
pub fn default_logging_settings() -> LlmLoggingSettings {
    LlmLoggingSettings {
        enabled: true,
        verbose_mode: false,
        retention_days: 30,
    }
}

/// The explicit ambient context replacing v4's `AsyncLocalStorage`. `None` outside
/// an autonomous turn (every request-path caller).
#[derive(Clone, Debug, Default)]
pub struct LogContext {
    /// Stamped onto the row's `autonomousRunId`; lets the room budget sum spend by
    /// run id. `None` outside an autonomous turn.
    pub autonomous_run_id: Option<String>,
}

impl LogContext {
    /// The request-path context (no autonomous run).
    pub fn none() -> Self {
        Self::default()
    }
}

// ============================================================================
// Params (mirror v4 `LogLLMCallParams`)
// ============================================================================

/// One request message the caller supplies (v4's `{ role, content, attachments? }`).
#[derive(Clone, Debug)]
pub struct LogRequestMessage {
    pub role: String,
    pub content: String,
    /// Presence + non-emptiness drives `hasAttachments`.
    pub attachments: Option<Vec<Value>>,
}

/// The request half (v4 `LogLLMCallParams['request']`).
#[derive(Clone, Debug, Default)]
pub struct LogRequest {
    pub messages: Vec<LogRequestMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    /// Only its length is summarized (`toolCount`).
    pub tools: Option<Vec<Value>>,
}

/// One native tool call (v4 `{ name, arguments }`).
#[derive(Clone, Debug)]
pub struct LogResponseToolCall {
    pub name: String,
    pub arguments: Value,
}

/// The response half (v4 `LogLLMCallParams['response']`).
#[derive(Clone, Debug, Default)]
pub struct LogResponse {
    pub content: String,
    pub error: Option<String>,
    pub finish_reason: Option<String>,
    /// Mapped `{ name, arguments }` only when present + non-empty.
    pub tool_calls: Option<Vec<LogResponseToolCall>>,
}

/// Token usage (each field optional; v4 `?? 0` on write when the object is built).
#[derive(Clone, Debug, Default)]
pub struct LogUsage {
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}

/// Prompt-cache usage (each field optional, passed through as-is — no `?? 0`).
#[derive(Clone, Debug, Default)]
pub struct LogCacheUsage {
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
}

/// The full call params (v4 `LogLLMCallParams`).
#[derive(Clone, Debug)]
pub struct LogLlmCallParams {
    pub user_id: String,
    /// `LLMLogType` (TEXT column). Use [`map_task_type_to_log_type`] / the
    /// [`log_type`] constants.
    pub log_type: String,
    pub message_id: Option<String>,
    pub chat_id: Option<String>,
    pub character_id: Option<String>,
    pub provider: String,
    pub model_name: String,
    pub request: LogRequest,
    pub response: LogResponse,
    pub usage: Option<LogUsage>,
    pub cache_usage: Option<LogCacheUsage>,
    /// v4 `rawProviderUsage !== undefined ? rawProviderUsage : null` — a JSON `null`
    /// and "absent" both land as SQL NULL, so `None`/`Some(Null)` both → NULL.
    pub raw_provider_usage: Option<Value>,
    pub request_hashes: Option<LlmLogRequestHashes>,
    pub duration_ms: Option<f64>,
}

// ============================================================================
// Pure summarizers
// ============================================================================

/// v4 `summarizeRequest`: FULL content (no truncation); `contentLength` is the
/// UTF-16 code-unit count; `hasAttachments` when the array is present + non-empty;
/// `toolCount` is the tools-array length (0 when absent).
pub fn summarize_request(request: &LogRequest) -> LlmLogRequestSummary {
    let messages = request
        .messages
        .iter()
        .map(|m| LlmLogMessageSummary {
            role: m.role.clone(),
            content: m.content.clone(),
            content_preview: None,
            content_length: utf16_len(&m.content) as i64,
            has_attachments: m.attachments.as_ref().is_some_and(|a| !a.is_empty()),
        })
        .collect();

    LlmLogRequestSummary {
        message_count: request.messages.len() as i64,
        messages,
        temperature: request.temperature,
        max_tokens: request.max_tokens,
        tool_count: request.tools.as_ref().map_or(0, |t| t.len()) as i64,
        full_messages: None,
    }
}

/// v4 `summarizeResponse`: FULL content; `contentLength` UTF-16; `error`/
/// `finishReason` `?? null`; `toolCalls` mapped `{ name, arguments }` only when
/// present + non-empty.
pub fn summarize_response(response: &LogResponse) -> LlmLogResponseSummary {
    let tool_calls = match &response.tool_calls {
        Some(tcs) if !tcs.is_empty() => Some(
            tcs.iter()
                .map(|tc| LlmLogToolCall {
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                })
                .collect(),
        ),
        _ => None,
    };

    LlmLogResponseSummary {
        content: response.content.clone(),
        content_preview: None,
        content_length: utf16_len(&response.content) as i64,
        full_content: None,
        // v4 `summarizeResponse` ALWAYS sets these (`response.error ?? null`), so
        // they store present-as-`null` when absent — `Some(None)`, not the outer
        // `None` that would skip the key.
        error: Some(response.error.clone()),
        finish_reason: Some(response.finish_reason.clone()),
        tool_calls,
    }
}

// ============================================================================
// is_logging_enabled
// ============================================================================

/// v4 `isLoggingEnabled`: returns the settings when logging is on (so the caller
/// knows to log), or `None` ONLY when settings exist and
/// `llmLoggingSettings.enabled` is explicitly false. Missing settings, a missing
/// `llmLoggingSettings` sub-object, and a read error all default to **enabled**.
pub async fn is_logging_enabled(db: &Db, user_id: &str) -> Option<LlmLoggingSettings> {
    let user_id = user_id.to_string();
    let read = db.read_main(move |conn| crate::db::chat_settings::find_by_user_id(conn, &user_id));

    let settings = match read {
        Ok(v) => v,
        // v4: read error -> default to enabled (still logs).
        Err(_) => return Some(default_logging_settings()),
    };

    match settings {
        // No chat_settings row -> default (enabled).
        None => Some(default_logging_settings()),
        Some(obj) => match obj.get("llmLoggingSettings") {
            // Missing sub-object -> default (enabled).
            None | Some(Value::Null) => Some(default_logging_settings()),
            Some(sub) => {
                let enabled = sub.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                if enabled {
                    // Return the real settings (fall back to default if it won't
                    // parse — the write only needs "is it Some").
                    Some(
                        serde_json::from_value::<LlmLoggingSettings>(sub.clone())
                            .unwrap_or_else(|_| default_logging_settings()),
                    )
                } else {
                    // Settings exist and logging is explicitly disabled.
                    None
                }
            }
        },
    }
}

// ============================================================================
// The writer
// ============================================================================

/// v4 `logLLMCall`: check the user's logging setting, summarize, and write one
/// `llm_logs` row (minting id + `createdAt`/`updatedAt`, stamping the run id from
/// `ctx`). Returns the minted row id, or `None` when logging is disabled or any
/// step failed (never throws — logging must not break the main flow).
pub async fn log_llm_call(db: &Db, params: LogLlmCallParams, ctx: &LogContext) -> Option<String> {
    // Disabled -> skip.
    is_logging_enabled(db, &params.user_id).await?;

    let request = summarize_request(&params.request);
    let response = summarize_response(&params.response);

    // v4: build the usage object only when any token field is present, then `?? 0`.
    let usage = params.usage.as_ref().and_then(|u| {
        if u.prompt_tokens.is_some() || u.completion_tokens.is_some() || u.total_tokens.is_some() {
            Some(LlmLogTokenUsage {
                prompt_tokens: u.prompt_tokens.unwrap_or(0),
                completion_tokens: u.completion_tokens.unwrap_or(0),
                total_tokens: u.total_tokens.unwrap_or(0),
            })
        } else {
            None
        }
    });

    // Cache usage: built only when a field is present; values passed through as-is.
    let cache_usage = params.cache_usage.as_ref().and_then(|c| {
        if c.cache_creation_input_tokens.is_some() || c.cache_read_input_tokens.is_some() {
            Some(LlmLogCacheUsage {
                cache_creation_input_tokens: c.cache_creation_input_tokens,
                cache_read_input_tokens: c.cache_read_input_tokens,
            })
        } else {
            None
        }
    });

    // Request hashes: kept only when any tier is present.
    let request_hashes = params.request_hashes.filter(|h| {
        h.system_block1_hash.is_some()
            || h.system_block2_hash.is_some()
            || h.system_block3_hash.is_some()
            || h.tools_array_hash.is_some()
            || h.history_tail_hash.is_some()
    });

    // rawProviderUsage: undefined OR JSON null -> SQL NULL.
    let raw_provider_usage = params.raw_provider_usage.filter(|v| !v.is_null());

    let id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    let data = LlCreate {
        user_id: params.user_id,
        log_type: params.log_type,
        message_id: params.message_id,
        chat_id: params.chat_id,
        character_id: params.character_id,
        autonomous_run_id: ctx.autonomous_run_id.clone(),
        provider: params.provider,
        model_name: params.model_name,
        request,
        response,
        usage,
        cache_usage,
        raw_provider_usage,
        request_hashes,
        duration_ms: params.duration_ms,
    };
    let opts = CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };

    let written = db
        .write(move |writers| {
            let Some(writer) = writers.llm_logs() else {
                return Err(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::LlmLogs,
                ));
            };
            writer.llm_logs().create(&data, &opts)
        })
        .await;

    match written {
        Ok(()) => Some(id),
        // v4's outer catch -> log + null. Never propagate.
        Err(_) => None,
    }
}

// ============================================================================
// Log types + task-type mapping
// ============================================================================

/// The `LLMLogType` string constants (the TEXT column values). `TOOL_CONTINUATION`
/// exists in v4's enum but has **no emitter** (UI-only) — carried for completeness,
/// never written by this service.
pub mod log_type {
    pub const CHAT_MESSAGE: &str = "CHAT_MESSAGE";
    pub const MEMORY_EXTRACTION: &str = "MEMORY_EXTRACTION";
    pub const TITLE_GENERATION: &str = "TITLE_GENERATION";
    pub const CONTEXT_COMPRESSION: &str = "CONTEXT_COMPRESSION";
    pub const SUMMARIZATION: &str = "SUMMARIZATION";
    pub const IMAGE_PROMPT_CRAFTING: &str = "IMAGE_PROMPT_CRAFTING";
    pub const IMAGE_DESCRIPTION: &str = "IMAGE_DESCRIPTION";
    pub const IMAGE_GENERATION: &str = "IMAGE_GENERATION";
    pub const APPEARANCE_RESOLUTION: &str = "APPEARANCE_RESOLUTION";
    pub const SCENE_STATE_TRACKING: &str = "SCENE_STATE_TRACKING";
    pub const ANSWER_CONFIRMATION: &str = "ANSWER_CONFIRMATION";
    pub const DANGER_CLASSIFICATION: &str = "DANGER_CLASSIFICATION";
    /// UI-only in v4; no emitter.
    pub const TOOL_CONTINUATION: &str = "TOOL_CONTINUATION";
}

/// v4 `mapTaskTypeToLogType` (core-execution.ts:36–63), verbatim including the
/// `|| 'SUMMARIZATION'` default (an unknown or absent task type → SUMMARIZATION).
pub fn map_task_type_to_log_type(task_type: Option<&str>) -> String {
    let mapped = match task_type.unwrap_or("") {
        "memory-extraction-self"
        | "memory-extraction-other"
        | "batch-memory-extraction"
        | "memory-keyword-extraction" => log_type::MEMORY_EXTRACTION,
        "title-chat" | "title-from-summary" | "consider-title-update" => log_type::TITLE_GENERATION,
        "compress-conversation-history" | "compress-system-prompt" | "compress-memories" => {
            log_type::CONTEXT_COMPRESSION
        }
        "summarize-chat"
        | "update-context-summary"
        | "fold-chat-summary"
        | "memory-recap-summarization"
        | "derive-scene-context"
        | "outfit-selection" => log_type::SUMMARIZATION,
        "craft-image-prompt" | "craft-story-background-prompt" => log_type::IMAGE_PROMPT_CRAFTING,
        "describe-attachment" => log_type::IMAGE_DESCRIPTION,
        "resolve-character-appearances" | "sanitize-appearance" => log_type::APPEARANCE_RESOLUTION,
        "scene-state-tracking" => log_type::SCENE_STATE_TRACKING,
        "answer-confirmation" | "answer-reaffirmation" => log_type::ANSWER_CONFIRMATION,
        _ => log_type::SUMMARIZATION,
    };
    mapped.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_type_map_matches_v4() {
        assert_eq!(
            map_task_type_to_log_type(Some("title-chat")),
            "TITLE_GENERATION"
        );
        assert_eq!(
            map_task_type_to_log_type(Some("compress-conversation-history")),
            "CONTEXT_COMPRESSION"
        );
        assert_eq!(
            map_task_type_to_log_type(Some("scene-state-tracking")),
            "SCENE_STATE_TRACKING"
        );
        // Unknown / absent -> SUMMARIZATION default.
        assert_eq!(map_task_type_to_log_type(Some("nonsense")), "SUMMARIZATION");
        assert_eq!(map_task_type_to_log_type(None), "SUMMARIZATION");
    }

    #[test]
    fn summarize_request_utf16_and_tool_count() {
        let req = LogRequest {
            messages: vec![
                LogRequestMessage {
                    role: "system".into(),
                    // "héllo" is 5 UTF-16 units (é is one BMP unit).
                    content: "héllo".into(),
                    attachments: None,
                },
                LogRequestMessage {
                    role: "user".into(),
                    content: "hi".into(),
                    attachments: Some(vec![serde_json::json!({"x":1})]),
                },
            ],
            temperature: Some(0.3),
            max_tokens: Some(2048),
            tools: Some(vec![serde_json::json!({"t":1}), serde_json::json!({"t":2})]),
        };
        let s = summarize_request(&req);
        assert_eq!(s.message_count, 2);
        assert_eq!(s.messages[0].content_length, 5);
        assert!(!s.messages[0].has_attachments);
        assert!(s.messages[1].has_attachments);
        assert_eq!(s.tool_count, 2);
        assert_eq!(s.temperature, Some(0.3));
    }

    #[test]
    fn summarize_response_omits_empty_tool_calls() {
        let r = LogResponse {
            content: "ok".into(),
            error: None,
            finish_reason: Some("stop".into()),
            tool_calls: Some(vec![]),
        };
        let s = summarize_response(&r);
        assert!(s.tool_calls.is_none());
        assert_eq!(s.content_length, 2);
    }

    // The full writer path (is_logging_enabled -> mint -> llm_logs.create) is
    // proven end-to-end by the tier-2 `compression_tier3` differential with
    // logging un-mocked (the work order's plan): an in-memory `:memory:` Db can't
    // exercise it here because the read pool and writer open SEPARATE `:memory:`
    // databases (nothing is shared), and an encrypted temp DB would need the
    // llm_logs schema materialized + a pepper.
}
