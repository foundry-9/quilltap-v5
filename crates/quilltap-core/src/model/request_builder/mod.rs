//! Request builders + the four `RequestTransform` hooks (W4.7c part 2).
//!
//! The **sans-IO** request-side counterpart to [`crate::model::decoders`]: turn a
//! provider-agnostic [`RequestInput`] (the `LLMParams` subset the chat path
//! passes) into a [`BuiltRequest`] *value* — method / url / headers / body — that
//! the host transport (W4.7d) actually sends. No HTTP here.
//!
//! Dispatched by the manifest (the W4.7a registry replaces `getProvider`): the
//! `baseUrl` + `endpoints.chat` give the url, `auth` gives the header shape, and
//! each provider's body is built by the family builder its wire protocol selects.
//! The four **`RequestTransform`** hooks live inside those builders:
//!
//! - **anthropic** ([`anthropic`]) — the mid-history cache breakpoint, the
//!   tool-result batching, and the adaptive-thinking / sampling-param-rejection
//!   rules for Sonnet 5 / Opus 4.7+ / Fable / Mythos.
//! - **openai** ([`responses_api`]) — `previous_response_id` conversation chaining
//!   (send only the last user message). The chaining *fallback-to-full-input* is a
//!   transport concern (it happens on a send error) → deferred to W4.7d.
//! - **google** ([`google`]) — the recursive JSON-Schema sanitizer + the
//!   `thoughtSignature` round-trip. **The genai-SDK `config → generationConfig`
//!   wire framing is deferred to the transport** (the SDK owns that mechanical
//!   serialization); this module ports + verifies the Quilltap-side request LOGIC
//!   (sanitizer, message `contents`/`systemInstruction`, the `config` bag).
//! - **deepseek** ([`chat_completions`]) — echo the prior turn's
//!   `reasoning_content` back on a tool-call turn.
//!
//! **Byte fidelity.** Every SDK the plugins use (OpenAI, Anthropic, the Responses
//! API, raw fetch) sends `JSON.stringify(body)` verbatim — the object key order is
//! insertion order (confirmed by `record-request-envelopes.mjs`). So the body is
//! built as a `serde_json::Map` (preserve_order) with keys inserted in the SAME
//! order v4 assigns them; integer-valued numbers stay integers to match
//! `JSON.stringify` (via [`crate::db::js_number_to_json`] where a passthrough
//! number could be integer-valued).

mod anthropic;
mod chat_completions;
pub mod google;
mod responses_api;

use serde_json::Value;

use crate::provider_manifest::Registry;

pub use chat_completions::openrouter_non_streaming_is_vision;
// The Ollama profile-parameter tables (P4.D83, v4 `93ed8abf`). Re-exported as
// the provider's declared contract: what a connection profile may set, and the
// three control keys read but never forwarded.
pub use chat_completions::{
    ollama_profile_timeout_ms, OLLAMA_CONTROL_PARAMS, OLLAMA_DEFAULT_REQUEST_TIMEOUT_SECONDS,
    OLLAMA_OPTION_PARAM_ALLOWLIST, OLLAMA_THINK_LEVELS, OLLAMA_TOP_LEVEL_PARAM_ALLOWLIST,
};

/// The per-request wall-clock budget a connection PROFILE names, if its provider
/// offers one (P4.D83, v4 `93ed8abf`/`d89babc4`). Only Ollama does at the pin —
/// its endpoint is a local box whose "how long is too long" nobody else can
/// answer — so this is a one-arm match rather than a table; a second provider
/// that grows the field adds an arm here.
///
/// `None` means "the profile named none", leaving the process-wide policy (its
/// timeout AND its retry count) in charge.
pub fn provider_profile_timeout_ms(
    provider: &str,
    profile_parameters: Option<&Value>,
) -> Option<u64> {
    match provider {
        "OLLAMA" => ollama_profile_timeout_ms(profile_parameters),
        _ => None,
    }
}
pub use google::sanitize_schema_for_google;

// ============================================================================
// Input value (the LLMParams subset the chat path passes)
// ============================================================================

// The message representation is the ONE carrying type of the provider-I/O
// seam (P4.13 unit 5): the same [`StreamMessage`] the loops build flows into
// the builders unconverted, so a field can no longer be dropped at a boundary
// the corpus does not execute (findings #24/#25's structural cause). The old
// `RequestMessage`/`ToolCallMsg` bag — whose all-optional fields made an
// id-less tool result representable — is GONE; re-exported here for the
// builder family modules.
pub use crate::model::stream::{
    StreamAttachmentFailure, StreamAttachmentResults, StreamMessage, ToolCallFunction,
    ToolCallPayload,
};

/// The provider-agnostic request input (v4 `LLMParams` subset). Optional fields
/// map to v4's `?? default` / `if present` handling per provider.
#[derive(Clone, Debug, Default)]
pub struct RequestInput {
    pub model: String,
    /// v4 `LLMMessage[]` — the carrying enum. A user message carries its
    /// `attachments` (v4's `FileAttachment` JSON bags, stamped by the file
    /// subsystem onto the last user message); each provider's builder emits its
    /// wire image/document parts from them — or drops them and reports the
    /// failure — exactly as v4's plugin `formatMessages*` does, and the
    /// sent/failed outcome rides [`BuiltRequest::attachment_results`] (P4.21).
    pub messages: Vec<StreamMessage>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub top_p: Option<f64>,
    /// v4 `params.stop` — normalized to a list on input (v4 accepts `string |
    /// string[]`; the chat path passes an array).
    pub stop: Option<Vec<String>>,
    /// Provider tools (already in the provider's `formatTools` shape — universal
    /// for chat-completions, `{name,description,parameters}` for google/anthropic
    /// per the W4.7c-part-1 reshape). Carried as `Value`.
    pub tools: Option<Vec<Value>>,
    /// v4 `params.toolChoice`.
    pub tool_choice: Option<Value>,
    /// v4 `params.responseFormat` (`{ type, jsonSchema? }`).
    pub response_format: Option<Value>,
    pub web_search_enabled: bool,
    /// v4 `params.profileParameters` (a provider-specific bag).
    pub profile_parameters: Option<Value>,
    /// v4 `params.cacheKey` (per-character cache routing hint).
    pub cache_key: Option<String>,
    /// v4 `params.previousResponseId` (openai chaining).
    pub previous_response_id: Option<String>,
    /// v4 `params.strictMaxTokens` (openai background-task reasoning floor).
    pub strict_max_tokens: bool,
    /// Which v4 plugin method is being reproduced: `streamMessage` (`true`) or
    /// `sendMessage` (`false`). Defaults to the streaming path.
    ///
    /// This is NOT a body field to flip — the two methods are separate literals
    /// in every plugin, and the deltas differ per provider (P4.11; each is pinned
    /// by a `mode`-tagged vector in `request-envelopes.recorded.ndjson`).
    pub stream: bool,
}

// ============================================================================
// Output value
// ============================================================================

/// A built request value (v4's `fetch(url, { method, headers, body })` args). The
/// `body` serializes (preserve_order) to the exact bytes the SDK/fetch sends.
#[derive(Clone, Debug)]
pub struct BuiltRequest {
    pub method: String,
    pub url: String,
    /// The auth + content-type + provider headers (from the manifest `auth` +
    /// fixed content-type). Volatile values (api key, user-agent version) are the
    /// transport's to fill — carried here as the manifest declares them.
    pub headers: Vec<(String, String)>,
    /// The request body as a JSON value; serialize with `serde_json::to_string`
    /// (compact, preserve_order) for the wire bytes.
    pub body: Value,
    /// v4 `attachmentResults` — computed at format time by every plugin (sent
    /// ids + per-attachment failure strings) and attached to the RESPONSE /
    /// final stream chunk, never the request body. The transport-side
    /// composers copy this onto `StreamChunk::attachment_results` /
    /// `CompletionResponse` (P4.21).
    pub attachment_results: StreamAttachmentResults,
}

impl BuiltRequest {
    /// The body serialized to the wire byte string (compact JSON, insertion order).
    pub fn body_string(&self) -> String {
        serde_json::to_string(&self.body).expect("request body serializes")
    }
}

/// A typed error for an unknown provider.
#[derive(Debug)]
pub enum BuildError {
    /// The provider id is not in the registry.
    UnknownProvider(String),
    /// The provider's request builder is deferred (e.g. google's genai wire
    /// framing).
    Deferred(String),
    /// v4's plugin (or its SDK) refuses these params CLIENT-SIDE and sends no
    /// request at all — reproduced so v5 fails where v4 fails instead of
    /// inventing a body v4 would never put on the wire.
    ProviderRefused(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::UnknownProvider(p) => write!(f, "unknown provider: {p}"),
            BuildError::Deferred(p) => write!(f, "request builder deferred: {p}"),
            BuildError::ProviderRefused(m) => write!(f, "provider refused the request: {m}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Build the request envelope for `provider` (the canonical UPPERCASE id) over the
/// built-in manifest registry. Dispatches to the family builder the provider's
/// wire protocol selects; the url/headers come from the manifest.
pub fn build_request(provider: &str, input: &RequestInput) -> Result<BuiltRequest, BuildError> {
    build_request_with_registry(Registry::built_in(), provider, input)
}

/// Build the request envelope over an arbitrary registry (third-party path).
pub fn build_request_with_registry(
    registry: &Registry,
    provider: &str,
    input: &RequestInput,
) -> Result<BuiltRequest, BuildError> {
    use crate::model::provider_io::ProviderKind;
    let manifest = registry
        .get_provider(provider)
        .ok_or_else(|| BuildError::UnknownProvider(provider.to_string()))?;
    let kind = ProviderKind::of(provider)
        .ok_or_else(|| BuildError::UnknownProvider(provider.to_string()))?;
    let headers = auth_headers(manifest);

    // Every builder collects v4's `attachmentResults` while formatting (sent
    // ids + failure strings, in message order) — carried on the BuiltRequest
    // for the response/final-chunk report, never in the body.
    let mut attachment_results = StreamAttachmentResults::default();

    // Google's genai endpoint is model-specific (`/models/{model}:…`) and its wire
    // body is the SDK reframing (W4.7d) — handled separately from the fixed-path
    // chat-completions / responses / anthropic builders.
    if kind == ProviderKind::Google {
        let url = google::google_chat_url(&manifest.base_url, &input.model, input.stream);
        let body = google::build_google_wire_body(input, &mut attachment_results)
            .map_err(|e| BuildError::Deferred(format!("GOOGLE wire framing: {e}")))?;
        return Ok(BuiltRequest {
            method: "POST".to_string(),
            url,
            headers,
            body,
            attachment_results,
        });
    }

    let url = format!("{}{}", manifest.base_url, manifest.endpoints.chat);
    let body = match kind {
        ProviderKind::Anthropic => anthropic::build_body(input, &mut attachment_results),
        ProviderKind::OpenAi => responses_api::build_openai_body(input, &mut attachment_results),
        ProviderKind::Grok => responses_api::build_grok_body(input, &mut attachment_results),
        ProviderKind::DeepSeek => {
            chat_completions::build_deepseek_body(input, &mut attachment_results)
        }
        ProviderKind::ZAi => chat_completions::build_zai_body(input, &mut attachment_results),
        ProviderKind::OpenRouter => {
            chat_completions::build_openrouter_body(input, &mut attachment_results)?
        }
        ProviderKind::Ollama => chat_completions::build_ollama_body(input, &mut attachment_results),
        ProviderKind::OpenAiCompatible => {
            chat_completions::build_openai_compatible_body(input, &mut attachment_results)
        }
        ProviderKind::Google => unreachable!("handled above"),
    };

    Ok(BuiltRequest {
        method: "POST".to_string(),
        url,
        headers,
        body,
        attachment_results,
    })
}

// ============================================================================
// Shared attachment helpers (P4.21)
// ============================================================================

/// A JS-truthy string field off a `FileAttachment` bag: `Some` iff present,
/// a string, and non-empty (v4's `!attachment.data` treats `""` as missing).
pub(crate) fn att_str<'a>(a: &'a Value, key: &str) -> Option<&'a str> {
    a.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// The attachment's id (v4 always sets one; an absent id degrades to `""`
/// rather than inventing).
pub(crate) fn att_id(a: &Value) -> String {
    a.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Record one failed attachment (v4 `failed.push({ id, error })`).
pub(crate) fn att_fail(results: &mut StreamAttachmentResults, a: &Value, error: impl Into<String>) {
    results.failed.push(StreamAttachmentFailure {
        id: att_id(a),
        error: error.into(),
    });
}

/// v4 `collectAttachmentFailures` (DeepSeek / Ollama / the OpenAI-compatible
/// base): the provider sends NO attachment — every one on every message fails
/// with the provider's fixed message, and the body strips them silently.
pub(crate) fn collect_drop_failures(
    messages: &[StreamMessage],
    error: &str,
    results: &mut StreamAttachmentResults,
) {
    for m in messages {
        for a in m.attachments() {
            att_fail(results, a, error);
        }
    }
}

/// The provider's declared auth headers (v4 sets `defaultHeaders` on the SDK
/// client / the raw-fetch `headers`). The api-key VALUE is the transport's to
/// inject; this carries the header NAME + any fixed extras (e.g. anthropic's
/// `anthropic-version`).
fn auth_headers(manifest: &crate::provider_manifest::Manifest) -> Vec<(String, String)> {
    let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
    headers.extend(super::provider_auth::declared_auth_extras(manifest));
    headers
}

// ============================================================================
// Shared body-building helpers
// ============================================================================

/// An ordered JSON object builder (insertion order = wire key order).
#[derive(Clone, Default)]
pub(crate) struct Body {
    map: serde_json::Map<String, Value>,
}

impl Body {
    pub(crate) fn new() -> Body {
        Body {
            map: serde_json::Map::new(),
        }
    }
    /// Insert a key (always). A repeated key keeps its FIRST position (JS object
    /// key-reassignment semantics) — match by removing then re-inserting only when
    /// new; here we let `Map::insert` update in place (IndexMap keeps position).
    pub(crate) fn set(&mut self, key: &str, value: Value) -> &mut Body {
        self.map.insert(key.to_string(), value);
        self
    }
    /// Insert a key only if `value` is `Some` (a JS `if present` field).
    pub(crate) fn set_opt(&mut self, key: &str, value: Option<Value>) -> &mut Body {
        if let Some(v) = value {
            self.map.insert(key.to_string(), v);
        }
        self
    }
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key)
    }
    pub(crate) fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
    /// Remove a key PRESERVING order (JS `delete`) — `serde_json::Map::remove`
    /// under preserve_order is swap_remove (reorders), so use `shift_remove`.
    pub(crate) fn remove(&mut self, key: &str) {
        self.map.shift_remove(key);
    }
    pub(crate) fn into_value(self) -> Value {
        Value::Object(self.map)
    }
}

/// A JS number rendered to match `JSON.stringify` (integer-valued floats bare).
pub(crate) fn num(n: f64) -> Value {
    crate::db::js_number_to_json(n)
}
