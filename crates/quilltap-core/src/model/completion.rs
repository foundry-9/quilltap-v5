//! The completion half of the model boundary — the tier-3 seam for the
//! cheap-LLM task pipeline (memory extraction, summarization, titling). Mirrors
//! the subset of v4's text-provider contract those tasks consume:
//! `provider.sendMessage(params, apiKey)` from
//! `packages/plugin-types/src/providers/text.ts` (`LLMParams` / `LLMResponse`),
//! as driven by `lib/memory/cheap-llm-tasks/core-execution.ts`.
//!
//! The boundary sits at the provider call itself — everything above it
//! (temperature fallback, the uncensored-provider retry, response parsing) is
//! ported orchestration that must be *inside* the differential, so the seam is
//! the same one the v4 oracle mocks (`createLLMProvider`'s returned provider).
//! API-key acquisition stays host-side (v4's `getApiKeyForCheapLLMSelection`
//! resolves before the provider call; the canned responder needs no key and the
//! real adapter arrives with the Phase-4 transport work).

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

/// A chat message sent to the completion provider. The cheap-LLM path only ever
/// builds `system` / `user` messages with plain string content, but the role set
/// mirrors v4's `LLMMessage.role` union in full.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompletionRole {
    System,
    User,
    Assistant,
    Tool,
}

impl CompletionRole {
    /// The v4 wire string for this role.
    pub fn as_str(self) -> &'static str {
        match self {
            CompletionRole::System => "system",
            CompletionRole::User => "user",
            CompletionRole::Assistant => "assistant",
            CompletionRole::Tool => "tool",
        }
    }
}

/// One message of a completion request (v4 `LLMMessage`, role + content subset).
#[derive(Clone, Debug, PartialEq)]
pub struct CompletionMessage {
    pub role: CompletionRole,
    pub content: String,
}

/// A file attachment carried on a completion request (v4 `FileAttachment`, the
/// subset the image-description vision call sends + the canned key needs). Only
/// the chat file/attachment path (W4.4b `generate_image_description`) populates
/// [`CompletionParams::attachments`]; every other completion caller leaves it
/// empty, so the canned key is byte-identical to the pre-W4.4b key there.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct CompletionAttachment {
    /// v4 `FileAttachment.id` — the file id the wire's `attachmentResults`
    /// reports sent/failed by (P4.21). Off the canned key (the key predates it
    /// and the v4 oracle's recorded keys must keep matching).
    #[serde(skip)]
    pub id: String,
    pub filename: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// The base64 payload the real adapter sends (unused by the canned key — kept
    /// off the key so the resized bytes don't have to be reproduced there;
    /// `filename`+`mimeType` distinguish the corpus's images).
    #[serde(skip)]
    pub data: String,
}

impl CompletionMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: CompletionRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: CompletionRole::User,
            content: content.into(),
        }
    }
}

/// Parameters of a completion call — the fields the cheap-LLM execution path
/// actually sets on v4's `LLMParams`. `strict_max_tokens` is always `true` on
/// this path (background tasks must not inherit reasoning-model minimums);
/// `cache_key` is v4's per-character prompt-cache identifier;
/// `profile_parameters` forwards the connection profile's provider extras
/// (e.g. DeepSeek `thinking` off) verbatim.
#[derive(Clone, Debug, PartialEq)]
pub struct CompletionParams {
    pub messages: Vec<CompletionMessage>,
    pub model: String,
    /// `None` when the profile is known not to support a custom temperature
    /// (v4 omits the field entirely rather than sending a default).
    pub temperature: Option<f64>,
    pub max_tokens: i64,
    pub strict_max_tokens: bool,
    pub cache_key: Option<String>,
    pub profile_parameters: Option<serde_json::Value>,
    /// Attachments carried on the (single, last) user message — only the W4.4b
    /// image-description vision call sets this (v4 `messages[].attachments`).
    /// Empty for every other completion caller.
    pub attachments: Vec<CompletionAttachment>,
}

/// Token usage of a completion (v4 `TokenUsage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CompletionUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

/// Response from a completion call — the `LLMResponse` subset the cheap-LLM
/// path consumes (`content` + `usage`).
#[derive(Clone, Debug, PartialEq)]
pub struct CompletionResponse {
    pub content: String,
    pub usage: Option<CompletionUsage>,
    /// v4 `response.finishReason` — consulted by the W4.4b image-description
    /// refusal heuristic (`finishReason === 'length'` on a reasoning model) and
    /// recorded by `logLLMCall`. `None` for callers that don't populate it.
    pub finish_reason: Option<String>,
    /// v4 `response.attachmentResults` (P4.21) — the builder's format-time
    /// sent/failed report, carried for parity. `Some` from the real
    /// `execute_completion` composition; `None` from the canned tier-3
    /// provider (the v4 oracle's mock returns none either — no differential
    /// diffs it on this path, and no v4 caller consumes it here).
    pub attachment_results: Option<crate::model::stream::StreamAttachmentResults>,
}

/// Error from a completion call. The message text matters: v4's execution path
/// inspects it (a message containing `temperature` or `does not support`
/// triggers the retry-without-temperature fallback).
#[derive(Clone, Debug)]
pub struct CompletionError {
    pub message: String,
}

impl CompletionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CompletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompletionError {}

/// The completion boundary — every completion call goes through this trait.
///
/// `provider` / `base_url` mirror v4's `createLLMProvider(selection.provider,
/// selection.baseUrl)` factory arguments; folding them into the call keeps the
/// trait a single seam (the canned responder keys on them, the real adapter
/// dispatches on them). Async and generic-consumed like
/// [`EmbeddingProvider`](crate::model::embedding::EmbeddingProvider) — no
/// boxing, futures stay `Send`.
pub trait CompletionProvider {
    fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl Future<Output = Result<CompletionResponse, CompletionError>> + Send;
}

/// `Arc<T>` is a [`CompletionProvider`] whenever `T` is (delegating to the inner
/// value) — the production-shaped ownership answer (W4.11a), letting one provider
/// be shared by value across a borrowed dep and an owned erased seam.
impl<T: CompletionProvider> CompletionProvider for Arc<T> {
    fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        (**self).send_message(provider, base_url, params)
    }
}

/// The canonical lookup key for a canned completion: provider, model,
/// temperature (v4 omits vs sends — `None` keys as `-`), and the messages as
/// the same `[{role, content}]` JSON v4 logs. The v4 oracle's mock computes the
/// identical string, so a call either matches an entry registered on both sides
/// or surfaces as a corpus omission on both sides.
pub fn canned_completion_key(
    provider: &str,
    model: &str,
    temperature: Option<f64>,
    messages: &[CompletionMessage],
) -> String {
    canned_key_from_parts(
        provider,
        model,
        temperature,
        messages
            .iter()
            .map(|m| (m.role.as_str(), m.content.as_str())),
    )
}

/// A message type whose role + content project into the canned tier-3 key.
/// Implemented by [`CompletionMessage`] (the oracle-recorded registration shape)
/// and the stream half's `StreamMessage` (the carrying type), so a canned stream
/// can be registered from either while both hash to the identical key bytes.
pub trait CannedKeyMessage {
    fn key_role(&self) -> &str;
    fn key_content(&self) -> &str;
}

impl CannedKeyMessage for CompletionMessage {
    fn key_role(&self) -> &str {
        self.role.as_str()
    }
    fn key_content(&self) -> &str {
        &self.content
    }
}

/// The shared byte layout behind [`canned_completion_key`] and the stream half's
/// `canned_stream_key`: provider, model, temperature, and the messages projected
/// to `[{role, content}]` JSON. One renderer so the two key functions cannot
/// drift — a richer stream message keys identically to its role+content twin.
pub(crate) fn canned_key_from_parts<'a>(
    provider: &str,
    model: &str,
    temperature: Option<f64>,
    parts: impl Iterator<Item = (&'a str, &'a str)>,
) -> String {
    let temp = match temperature {
        // f64 Display matches JS Number→string for the plain literals used here
        // (e.g. `0.3`); the key only needs equality, not JS formatting fidelity.
        Some(t) => t.to_string(),
        None => "-".to_string(),
    };
    let rendered: Vec<serde_json::Value> = parts
        .map(|(role, content)| {
            serde_json::json!({
                "role": role,
                "content": content,
            })
        })
        .collect();
    let messages_json =
        serde_json::to_string(&rendered).expect("completion messages serialize infallibly");
    format!("{provider}|{model}|{temp}|{messages_json}")
}

/// The canned key extended with the request's attachments (W4.4b image
/// description). Byte-identical to [`canned_completion_key`] when `attachments` is
/// empty (so every pre-W4.4b caller keys unchanged); otherwise appends
/// `|atts:[{"filename":…,"mimeType":…}]` (the base64 data is deliberately off the
/// key — the corpus's images are distinguished by filename+mimeType). The v4
/// file-attachment oracle computes the identical string from the recorded
/// `messageParams.messages[0].attachments`.
pub fn canned_completion_key_with_attachments(
    provider: &str,
    model: &str,
    temperature: Option<f64>,
    messages: &[CompletionMessage],
    attachments: &[CompletionAttachment],
) -> String {
    let base = canned_completion_key(provider, model, temperature, messages);
    if attachments.is_empty() {
        return base;
    }
    let atts_json =
        serde_json::to_string(attachments).expect("completion attachments serialize infallibly");
    format!("{base}|atts:{atts_json}")
}

/// A deterministic [`CompletionProvider`] for the tier-3 differential. Returns a
/// fixed [`CompletionResponse`] keyed by the **exact** call input
/// ([`canned_completion_key`]), or an explicit failure for keys registered as
/// failing. The Rust test and the v4 oracle build the same key→response map, so
/// the model call is pinned identically on both sides.
///
/// An unregistered input is an error (a corpus omission — surfaced, never
/// silently answered). A failure entry fails on **every** call with its
/// registered message, so error-inspecting fallbacks (the temperature retry,
/// the uncensored-provider retry) can be driven deterministically.
#[derive(Clone, Default)]
pub struct CannedCompletionProvider {
    responses: HashMap<String, CompletionResponse>,
    failures: HashMap<String, String>,
}

impl CannedCompletionProvider {
    /// A fresh provider with no canned responses.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a response for the exact call `(provider, model, temperature,
    /// messages)` (no attachments, no finish reason — the pre-W4.4b shape).
    #[allow(clippy::too_many_arguments)]
    pub fn with_response(
        mut self,
        provider: &str,
        model: &str,
        temperature: Option<f64>,
        messages: &[CompletionMessage],
        content: impl Into<String>,
        usage: Option<CompletionUsage>,
    ) -> Self {
        let key = canned_completion_key(provider, model, temperature, messages);
        self.responses.insert(
            key,
            CompletionResponse {
                content: content.into(),
                usage,
                finish_reason: None,
                attachment_results: None,
            },
        );
        self
    }

    /// Register a full response for a call keyed WITH its attachments (the W4.4b
    /// image-description vision call), carrying `finish_reason`.
    #[allow(clippy::too_many_arguments)]
    pub fn with_response_full(
        mut self,
        provider: &str,
        model: &str,
        temperature: Option<f64>,
        messages: &[CompletionMessage],
        attachments: &[CompletionAttachment],
        response: CompletionResponse,
    ) -> Self {
        let key = canned_completion_key_with_attachments(
            provider,
            model,
            temperature,
            messages,
            attachments,
        );
        self.responses.insert(key, response);
        self
    }

    /// Register a failure keyed WITH its attachments (the W4.4b vision path — a
    /// timeout / provider error surfaced as a thrown error in v4).
    #[allow(clippy::too_many_arguments)]
    pub fn with_failure_full(
        mut self,
        provider: &str,
        model: &str,
        temperature: Option<f64>,
        messages: &[CompletionMessage],
        attachments: &[CompletionAttachment],
        message: impl Into<String>,
    ) -> Self {
        let key = canned_completion_key_with_attachments(
            provider,
            model,
            temperature,
            messages,
            attachments,
        );
        self.failures.insert(key, message.into());
        self
    }

    /// Register a failure (with its exact error message) for the call key.
    pub fn with_failure(
        mut self,
        provider: &str,
        model: &str,
        temperature: Option<f64>,
        messages: &[CompletionMessage],
        message: impl Into<String>,
    ) -> Self {
        let key = canned_completion_key(provider, model, temperature, messages);
        self.failures.insert(key, message.into());
        self
    }
}

impl CompletionProvider for CannedCompletionProvider {
    fn send_message(
        &self,
        provider: &str,
        _base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        // Resolve synchronously so the returned future owns its result and is
        // `'static` + `Send` (no borrow of `&self` escapes into the future).
        let key = canned_completion_key_with_attachments(
            provider,
            &params.model,
            params.temperature,
            &params.messages,
            &params.attachments,
        );
        let result =
            if let Some(message) = self.failures.get(&key) {
                Err(CompletionError::new(message.clone()))
            } else {
                match self.responses.get(&key) {
                    Some(r) => Ok(r.clone()),
                    None => {
                        // NB "temp", not "temperature": the cheap-LLM executor
                        // treats a provider error mentioning "temperature" (or
                        // "does not support") as a temperature REJECTION and
                        // caches the profile as no-custom-temperature, which
                        // would silently re-key every later canned call in the
                        // same run. A canned MISS must not look like one.
                        Err(CompletionError::new(format!(
                    "no canned completion registered for call (provider {provider}, model {}, \
                     temp {:?}, {} message(s), first {} chars)",
                    params.model,
                    params.temperature,
                    params.messages.len(),
                    params.messages.first().map(|m| m.content.len()).unwrap_or(0),
                )))
                    }
                }
            };
        async move { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(
        model: &str,
        temperature: Option<f64>,
        messages: Vec<CompletionMessage>,
    ) -> CompletionParams {
        CompletionParams {
            messages,
            model: model.to_string(),
            temperature,
            max_tokens: 2048,
            strict_max_tokens: true,
            cache_key: None,
            profile_parameters: None,
            attachments: Vec::new(),
        }
    }

    #[tokio::test]
    async fn returns_injected_response_for_known_input() {
        let messages = vec![
            CompletionMessage::system("extract memories"),
            CompletionMessage::user("the transcript"),
        ];
        let provider = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "claude-haiku-4-5-20251001",
            Some(0.3),
            &messages,
            "[]",
            Some(CompletionUsage {
                prompt_tokens: 100,
                completion_tokens: 2,
                total_tokens: 102,
            }),
        );
        let response = provider
            .send_message(
                "ANTHROPIC",
                None,
                &params("claude-haiku-4-5-20251001", Some(0.3), messages),
            )
            .await
            .unwrap();
        assert_eq!(response.content, "[]");
        assert_eq!(response.usage.unwrap().total_tokens, 102);
    }

    /// The `Arc<T>` blanket impl (W4.11a) delegates to the inner provider.
    #[tokio::test]
    async fn arc_delegates_to_inner() {
        let messages = vec![CompletionMessage::user("hi")];
        let inner = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "m",
            None,
            &messages,
            "ok",
            None,
        );
        let arc = std::sync::Arc::new(inner);
        let response = arc
            .send_message("ANTHROPIC", None, &params("m", None, messages))
            .await
            .unwrap();
        assert_eq!(response.content, "ok");
    }

    #[tokio::test]
    async fn key_distinguishes_temperature_presence() {
        // The temperature-fallback path re-issues the same messages WITHOUT a
        // temperature — the two calls must resolve to different entries.
        let messages = vec![CompletionMessage::user("hi")];
        let provider = CannedCompletionProvider::new()
            .with_failure(
                "OPENAI",
                "gpt-x",
                Some(0.3),
                &messages,
                "model does not support temperature",
            )
            .with_response("OPENAI", "gpt-x", None, &messages, "ok", None);

        let err = provider
            .send_message(
                "OPENAI",
                None,
                &params("gpt-x", Some(0.3), messages.clone()),
            )
            .await
            .unwrap_err();
        assert!(err.message.contains("temperature"));

        let response = provider
            .send_message("OPENAI", None, &params("gpt-x", None, messages))
            .await
            .unwrap();
        assert_eq!(response.content, "ok");
    }

    #[tokio::test]
    async fn key_distinguishes_provider_and_model() {
        // The uncensored fallback re-sends the same messages on a different
        // provider/model — each (provider, model) pair is its own entry.
        let messages = vec![CompletionMessage::user("dangerous transcript")];
        let provider = CannedCompletionProvider::new()
            .with_response("ANTHROPIC", "safe-model", Some(0.3), &messages, "", None)
            .with_response(
                "OLLAMA",
                "uncensored-model",
                Some(0.3),
                &messages,
                "[{}]",
                None,
            );

        let safe = provider
            .send_message(
                "ANTHROPIC",
                None,
                &params("safe-model", Some(0.3), messages.clone()),
            )
            .await
            .unwrap();
        assert_eq!(safe.content, "");

        let fallback = provider
            .send_message(
                "OLLAMA",
                Some("http://localhost:11434"),
                &params("uncensored-model", Some(0.3), messages),
            )
            .await
            .unwrap();
        assert_eq!(fallback.content, "[{}]");
    }

    #[tokio::test]
    async fn unregistered_input_is_an_error_not_a_silent_answer() {
        let provider = CannedCompletionProvider::new();
        let err = provider
            .send_message(
                "ANTHROPIC",
                None,
                &params("m", Some(0.3), vec![CompletionMessage::user("unknown")]),
            )
            .await
            .unwrap_err();
        assert!(err.message.contains("no canned completion registered"));
    }

    #[tokio::test]
    async fn failure_entry_errors_every_call_with_its_message() {
        let messages = vec![CompletionMessage::user("boom")];
        let provider = CannedCompletionProvider::new().with_failure(
            "OPENAI",
            "m",
            Some(0.3),
            &messages,
            "provider down",
        );
        // Deterministic on repeat calls (stateless).
        for _ in 0..2 {
            let err = provider
                .send_message("OPENAI", None, &params("m", Some(0.3), messages.clone()))
                .await
                .unwrap_err();
            assert_eq!(err.message, "provider down");
        }
    }
}
