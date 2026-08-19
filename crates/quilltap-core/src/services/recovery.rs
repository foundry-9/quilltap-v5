//! The request-limit recovery service (Phase-3 Unit-3 wave 3) — v4
//! `recovery.service.ts` (`attemptRequestLimitRecovery`, the recovery-message
//! builders, the static fallback).
//!
//! When the primary stream fails with a recoverable request error (token limit,
//! PDF page cap, etc.), this service sends a small hand-built message to the LLM
//! explaining what happened so it can respond helpfully in character:
//!
//!   1. `build_recovery_system_prompt` / `build_recovery_user_message` compose a
//!      minimal prompt (small enough to always fit) that names the limit and the
//!      attachments; the recovery stream runs with `maxTokens 1000`, tools empty.
//!   2. On success the recovery response is written as an assistant message with
//!      a `recoveryType` (`token_limit` / `content_limit`) and a `done` event with
//!      `null` usage is emitted — the whole request is finalized, so the
//!      orchestrator short-circuits.
//!   3. On failure it falls back to `build_static_fallback_message`, streamed in
//!      50-char content chunks (to simulate typing) and written with a `_static`
//!      recovery type.
//!
//! ## Boundary notes
//!
//! The recovery response content is streamed as [`ChatEvent::Content`] deltas and
//! the finalization as a [`ChatEvent::Done`]; the committed message goes through
//! the [`Db`] writer (`addMessage` — the `recoveryType` column is already on
//! `db::chats_messages`'s `MessageEventInput`). The message-builder strings are
//! byte-exact (they reach the DB); `format_bytes` is the ported
//! [`crate::format_bytes::format_bytes`], and `to_locale_string` /
//! `parse_*_limit_error` are the ported [`super::primary_stream`] helpers.

use crate::db::runtime::Db;
use crate::format_bytes::format_bytes;
use crate::model::stream::StreamMessage;
use crate::model::stream::{StreamError, StreamParams, StreamingCompletionProvider};

use super::chat_events::{ChatEvent, DonePayload, EventSink};
use super::primary_stream::{
    content_limit_description, is_token_limit_error, parse_content_limit_error,
    parse_token_limit_error, to_locale_string, ContentLimitType, EffectiveProfile,
};

/// A file attached to the message (v4 `AttachedFile` — the recovery-relevant
/// subset: `filename` / `mimeType` / `size`).
#[derive(Clone, Debug, PartialEq)]
pub struct AttachedFile {
    pub filename: String,
    pub mime_type: String,
    pub size: f64,
}

/// Everything `attempt_request_limit_recovery` needs (v4 `RecoveryContext` minus
/// the controller/encoder, which become the [`EventSink`], and the repos, which
/// become the [`Db`]). `connection_profile` mirrors `streaming.effectiveProfile`.
#[derive(Clone, Debug)]
pub struct RecoveryContext {
    pub chat_id: String,
    pub character_name: String,
    pub connection_profile: Option<EffectiveProfile>,
    pub api_key: String,
    pub attached_files: Vec<AttachedFile>,
    pub original_message: Option<String>,
    /// The stream error message that triggered recovery (v4 passes the error
    /// object; we carry its message — the classifiers key on it).
    pub error_message: String,
    pub character_participant_id: String,
}

/// The result of a recovery attempt (v4 `RecoveryResult`).
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryResult {
    pub success: bool,
    pub response: Option<String>,
    pub message_id: Option<String>,
    pub is_static_fallback: bool,
}

/// v4 `buildRecoverySystemPrompt`.
pub fn build_recovery_system_prompt(character_name: &str) -> String {
    format!(
        "You are {character_name}. A technical issue occurred with the user's message. \
         Please respond helpfully and in character."
    )
}

/// v4 `buildRecoveryUserMessage` — explains what happened, byte-exact (reaches
/// the DB as the recovery request the model answers).
pub fn build_recovery_user_message(
    error_message: &str,
    attached_files: &[AttachedFile],
    original_message: Option<&str>,
) -> String {
    let error_is_token = is_token_limit_error(error_message);
    let mut parts: Vec<String> = Vec::new();

    parts.push("[Automatic System Notice]".into());
    parts.push(String::new());

    if error_is_token {
        let (req, max) = parse_token_limit_error(error_message);
        if let (Some(req), Some(max)) = (req, max) {
            parts.push(format!(
                "The user's message could not be processed because the total request size \
                 ({} tokens) exceeded the model's limit ({} tokens).",
                to_locale_string(req),
                to_locale_string(max)
            ));
        } else {
            parts.push(
                "The user's message could not be processed because it exceeded the model's \
                 token limit."
                    .into(),
            );
        }
    } else {
        let (kind, max, desc) = parse_content_limit_error(error_message);
        if let Some(desc) = desc.as_deref() {
            parts.push(format!("The user's message could not be processed: {desc}"));
        } else if let (Some(max), ContentLimitType::PdfPages) = (max, kind) {
            parts.push(format!(
                "The user's message could not be processed because the attached PDF exceeds \
                 the maximum of {max} pages."
            ));
        } else {
            parts.push(format!(
                "The user's message could not be processed because an attachment exceeded the \
                 {}.",
                content_limit_description(kind)
            ));
        }
    }
    parts.push(String::new());

    if !attached_files.is_empty() {
        parts.push("Attached file details:".into());
        for f in attached_files {
            parts.push(format!("- Filename: {}", f.filename));
            parts.push(format!("  Type: {}", f.mime_type));
            parts.push(format!("  Size: {}", format_bytes(f.size)));
        }
        parts.push(String::new());
    } else if error_is_token {
        parts.push(
            "No files were attached. The limit was likely exceeded due to a long conversation \
             history."
                .into(),
        );
        parts.push(String::new());
    }

    if let Some(orig) = original_message {
        if !crate::jsstr::js_trim(orig).is_empty() {
            parts.push("The user's original message was:".into());
            parts.push(format!("\"{orig}\""));
            parts.push(String::new());
        }
    }

    if !error_is_token {
        let (kind, _max, _desc) = parse_content_limit_error(error_message);
        if kind == ContentLimitType::PdfPages {
            parts.push(
                "Please acknowledge this limitation and suggest how the user might proceed \
                 (e.g., splitting the PDF into smaller parts with fewer pages, extracting \
                 specific sections, or summarizing the document themselves and asking specific \
                 questions about it)."
                    .into(),
            );
        } else {
            parts.push(
                "Please acknowledge this limitation and suggest how the user might proceed \
                 (e.g., using a smaller file, compressing the content, or describing what they \
                 need help with)."
                    .into(),
            );
        }
    } else {
        parts.push(
            "Please acknowledge this limitation and suggest how the user might proceed (e.g., \
             breaking the document into smaller sections, asking about specific parts, or \
             starting a new conversation with a shorter history)."
                .into(),
        );
    }

    parts.join("\n")
}

/// v4 `buildStaticFallbackMessage` — byte-exact.
pub fn build_static_fallback_message(
    attached_files: &[AttachedFile],
    error_message: &str,
) -> String {
    let error_is_token = is_token_limit_error(error_message);
    let mut parts: Vec<String> = Vec::new();

    parts.push("I apologize, but your message could not be processed.".into());
    parts.push(String::new());

    if error_is_token {
        let (req, max) = parse_token_limit_error(error_message);
        if let (Some(req), Some(max)) = (req, max) {
            parts.push(format!(
                "Your request contained {} tokens, but the maximum allowed is {} tokens.",
                to_locale_string(req),
                to_locale_string(max)
            ));
        } else {
            parts.push("Your request exceeded the maximum token limit.".into());
        }
    } else {
        let (kind, max, desc) = parse_content_limit_error(error_message);
        if let (ContentLimitType::PdfPages, Some(max)) = (kind, max) {
            parts.push(format!(
                "The attached PDF exceeds the maximum of {max} pages."
            ));
        } else if let Some(desc) = desc.as_deref() {
            parts.push(desc.to_string());
        } else {
            parts.push("An attached file exceeded the allowed limits.".into());
        }
    }

    if !attached_files.is_empty() {
        parts.push(String::new());
        parts.push(format!("You attached {} file(s):", attached_files.len()));
        for f in attached_files {
            parts.push(format!("- {} ({})", f.filename, format_bytes(f.size)));
        }
        parts.push(String::new());
        parts.push("Please try one of the following:".into());

        let (kind, _max, _desc) = parse_content_limit_error(error_message);
        if kind == ContentLimitType::PdfPages {
            parts.push("- Split the PDF into smaller parts (under 100 pages each)".into());
            parts.push("- Extract only the specific pages you need".into());
            parts.push("- Summarize the key points yourself and ask specific questions".into());
        } else {
            parts.push("- Remove the attachment and summarize the key points yourself".into());
            parts.push("- Break the document into smaller sections".into());
            parts.push("- Use a smaller or compressed version of the file".into());
        }
    } else if error_is_token {
        parts.push(String::new());
        parts.push("This conversation may have become too long. Please try:".into());
        parts.push("- Starting a new conversation".into());
        parts.push("- Asking a shorter question".into());
    }

    parts.join("\n")
}

/// v4 `attemptRequestLimitRecovery` — send a simplified message explaining the
/// limit; on failure fall back to the static message. Returns whether recovery
/// succeeded (always `true` given the static fallback catches every LLM failure).
pub async fn attempt_request_limit_recovery<P, S>(
    db: &Db,
    provider: &P,
    sink: &S,
    ctx: RecoveryContext,
) -> RecoveryResult
where
    P: StreamingCompletionProvider,
    S: EventSink,
{
    let is_token_limit = is_token_limit_error(&ctx.error_message);

    let system_prompt = build_recovery_system_prompt(&ctx.character_name);
    let user_message = build_recovery_user_message(
        &ctx.error_message,
        &ctx.attached_files,
        ctx.original_message.as_deref(),
    );

    let recovery_messages = vec![
        StreamMessage::system(system_prompt),
        StreamMessage::user(user_message),
    ];

    let recovery_type = if is_token_limit {
        "token_limit"
    } else {
        "content_limit"
    };

    let profile = ctx.connection_profile.clone();
    let (provider_name, model, base_url) = match &profile {
        Some(p) => (p.provider.clone(), p.model_name.clone(), p.base_url.clone()),
        None => (String::new(), String::new(), None),
    };

    // v4: modelParams { temperature: 0.7, maxTokens: 1000 }, tools: [].
    let params = StreamParams {
        messages: recovery_messages,
        model,
        temperature: Some(0.7),
        max_tokens: Some(1000),
        top_p: None,
        tools: None,
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
        // v4 sets no `requestTimeoutMs` on any streaming call (P4.D83).
        request_timeout_ms: None,
    };

    let mut full_response = String::new();
    let stream_result = drain_recovery_stream(
        provider,
        &provider_name,
        base_url.as_deref(),
        &params,
        sink,
        &mut full_response,
    )
    .await;

    match stream_result {
        Ok(()) => {
            // Save the recovery response.
            let message_id = uuid::Uuid::new_v4().to_string();
            let now = crate::clock::now_iso();
            let chat_id = ctx.chat_id.clone();
            let participant_id = ctx.character_participant_id.clone();
            let content = full_response.clone();
            let rt = recovery_type.to_string();
            let mid = message_id.clone();

            let write = db
                .write(move |writers| {
                    let event =
                        build_recovery_message_event(&mid, &content, &now, &participant_id, &rt)?;
                    writers.main().chat_messages().add_message(&chat_id, &event)
                })
                .await;

            if let Err(_e) = write {
                // On a persist failure fall back to static (mirrors the LLM-failure
                // branch — the static path re-attempts a fresh write).
                return stream_static_fallback(db, sink, &ctx).await;
            }

            sink.emit(ChatEvent::done(DonePayload {
                message_id: Some(message_id.clone()),
                usage: None,
                cache_usage: None,
                attachment_results: None,
                tools_executed: false,
                ..Default::default()
            }));

            RecoveryResult {
                success: true,
                response: Some(full_response),
                message_id: Some(message_id),
                is_static_fallback: false,
            }
        }
        Err(_recovery_error) => stream_static_fallback(db, sink, &ctx).await,
    }
}

/// Drain the recovery stream: content deltas → [`ChatEvent::Content`] + the
/// accumulator. A mid-stream (or pre-first-chunk) error surfaces as `Err`.
async fn drain_recovery_stream<P, S>(
    provider: &P,
    provider_name: &str,
    base_url: Option<&str>,
    params: &StreamParams,
    sink: &S,
    full_response: &mut String,
) -> Result<(), StreamError>
where
    P: StreamingCompletionProvider,
    S: EventSink,
{
    let mut rx = provider
        .stream_message(provider_name, base_url, params)
        .await;
    while let Some(item) = rx.recv().await {
        let chunk = item?;
        if !chunk.content.is_empty() {
            full_response.push_str(&chunk.content);
            sink.emit(ChatEvent::content(chunk.content.clone()));
        }
    }
    Ok(())
}

/// v4 `streamStaticFallback` — stream the static message in 50-char chunks, save
/// it with a `_static` recovery type, emit a `done` with null usage.
async fn stream_static_fallback<S: EventSink>(
    db: &Db,
    sink: &S,
    ctx: &RecoveryContext,
) -> RecoveryResult {
    let error_is_token = is_token_limit_error(&ctx.error_message);
    let fallback_message = build_static_fallback_message(&ctx.attached_files, &ctx.error_message);

    // v4 streams in 50-char (UTF-16 unit) chunks. JS `.slice(i, i+50)` slices in
    // UTF-16 units; reproduce so a surrogate-pair split matches v4 byte-for-byte.
    for chunk in slice_utf16_chunks(&fallback_message, 50) {
        sink.emit(ChatEvent::content(chunk));
    }

    let recovery_type = if error_is_token {
        "token_limit_static"
    } else {
        "content_limit_static"
    };

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let chat_id = ctx.chat_id.clone();
    let participant_id = ctx.character_participant_id.clone();
    let content = fallback_message.clone();
    let rt = recovery_type.to_string();
    let mid = message_id.clone();

    let _ = db
        .write(move |writers| {
            let event = build_recovery_message_event(&mid, &content, &now, &participant_id, &rt)?;
            writers.main().chat_messages().add_message(&chat_id, &event)
        })
        .await;

    sink.emit(ChatEvent::done(DonePayload {
        message_id: Some(message_id.clone()),
        usage: None,
        cache_usage: None,
        attachment_results: None,
        tools_executed: false,
        ..Default::default()
    }));

    RecoveryResult {
        success: true,
        response: Some(fallback_message),
        message_id: Some(message_id),
        is_static_fallback: true,
    }
}

/// Build the `type:message` event v4's recovery `addMessage` writes: id / role
/// ASSISTANT / content / attachments [] / createdAt / participantId /
/// recoveryType. (No usage / rawResponse / reasoning — v4's recovery inserts only
/// these keys.)
fn build_recovery_message_event(
    message_id: &str,
    content: &str,
    now: &str,
    participant_id: &str,
    recovery_type: &str,
) -> Result<crate::db::chats_messages::ChatEventInput, crate::db::DbError> {
    let value = serde_json::json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "attachments": [],
        "createdAt": now,
        "participantId": participant_id,
        "recoveryType": recovery_type,
    });
    serde_json::from_value(value)
        .map_err(|e| crate::db::DbError::Internal(format!("recovery message marshal: {e}")))
}

/// `s` split into successive `n`-UTF-16-unit slices (v4's
/// `message.slice(i, i+chunkSize)` loop).
fn slice_utf16_chunks(s: &str, n: usize) -> Vec<String> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let end = (i + n).min(units.len());
        out.push(String::from_utf16_lossy(&units[i..end]));
        i = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_err() -> String {
        // A real token-limit message: matches `prompt is too long` /
        // `maximum.*tokens` AND carries the `>` form the parser reads.
        "prompt is too long: 210311 tokens > 200000 maximum".to_string()
    }

    #[test]
    fn recovery_system_prompt_is_exact() {
        assert_eq!(
            build_recovery_system_prompt("Friday"),
            "You are Friday. A technical issue occurred with the user's message. \
             Please respond helpfully and in character."
        );
    }

    #[test]
    fn token_limit_recovery_user_message_no_files() {
        let msg = build_recovery_user_message(&token_err(), &[], Some("summarize this book"));
        let expected = "[Automatic System Notice]\n\
             \n\
             The user's message could not be processed because the total request size \
             (210,311 tokens) exceeded the model's limit (200,000 tokens).\n\
             \n\
             No files were attached. The limit was likely exceeded due to a long conversation history.\n\
             \n\
             The user's original message was:\n\
             \"summarize this book\"\n\
             \n\
             Please acknowledge this limitation and suggest how the user might proceed (e.g., \
             breaking the document into smaller sections, asking about specific parts, or \
             starting a new conversation with a shorter history).";
        assert_eq!(msg, expected);
    }

    #[test]
    fn pdf_content_limit_recovery_user_message_with_file() {
        let err = "Exceeds the maximum of 100 PDF pages";
        let files = vec![AttachedFile {
            filename: "book.pdf".into(),
            mime_type: "application/pdf".into(),
            size: 5_242_880.0,
        }];
        let msg = build_recovery_user_message(err, &files, None);
        assert!(msg.contains(
            "The user's message could not be processed: PDF documents cannot exceed 100 pages"
        ));
        assert!(msg.contains("- Filename: book.pdf"));
        assert!(msg.contains("  Type: application/pdf"));
        assert!(msg.contains("  Size: 5.0 MB"));
        assert!(msg.contains("splitting the PDF into smaller parts"));
    }

    #[test]
    fn static_fallback_token_no_files() {
        let msg = build_static_fallback_message(&[], &token_err());
        let expected = "I apologize, but your message could not be processed.\n\
             \n\
             Your request contained 210,311 tokens, but the maximum allowed is 200,000 tokens.\n\
             \n\
             This conversation may have become too long. Please try:\n\
             - Starting a new conversation\n\
             - Asking a shorter question";
        assert_eq!(msg, expected);
    }

    #[test]
    fn utf16_chunking_splits_at_50_units() {
        let s = "x".repeat(120);
        let chunks = slice_utf16_chunks(&s, 50);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 50);
        assert_eq!(chunks[1].len(), 50);
        assert_eq!(chunks[2].len(), 20);
    }
}
