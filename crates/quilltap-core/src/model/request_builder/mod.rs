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

pub use google::sanitize_schema_for_google;

// ============================================================================
// Input value (the LLMParams subset the chat path passes)
// ============================================================================

/// One assistant tool call carried on a message (v4 `LLMMessage.toolCalls[]`).
#[derive(Clone, Debug, Default)]
pub struct ToolCallMsg {
    pub id: String,
    /// v4 `tc.type` (usually `"function"`).
    pub type_: String,
    pub name: String,
    /// The raw arguments JSON STRING (v4 `tc.function.arguments`).
    pub arguments: String,
}

/// A message in the provider-agnostic shape (v4 `LLMMessage`). `content` is the
/// text; the tool / reasoning / thought-signature fields drive the per-provider
/// message formatting. Attachments are out of scope here (the chat path's
/// formatted messages are text; multimodal is the file subsystem's concern).
#[derive(Clone, Debug, Default)]
pub struct RequestMessage {
    /// `"system"` | `"user"` | `"assistant"` | `"tool"`.
    pub role: String,
    pub content: String,
    /// Present on a `tool`-role result (v4 `msg.toolCallId`).
    pub tool_call_id: Option<String>,
    /// Present on an assistant turn that invoked tools (v4 `msg.toolCalls`).
    pub tool_calls: Vec<ToolCallMsg>,
    /// The assistant's own reasoning text (v4 `msg.reasoningContent`) — echoed by
    /// the deepseek / z-ai transforms on a tool-call turn.
    pub reasoning_content: Option<String>,
    /// The Gemini thought signature (v4 `msg.thoughtSignature`) — the google
    /// round-trip.
    pub thought_signature: Option<String>,
    /// A caller-supplied ephemeral cache-control hint (v4 `msg.cacheControl`).
    pub cache_control: Option<Value>,
    /// v4 `msg.name` — a tool message's function name (google correlation).
    pub name: Option<String>,
}

impl RequestMessage {
    /// A plain text message.
    pub fn text(role: &str, content: &str) -> RequestMessage {
        RequestMessage {
            role: role.to_string(),
            content: content.to_string(),
            ..Default::default()
        }
    }
}

/// The provider-agnostic request input (v4 `LLMParams` subset). Optional fields
/// map to v4's `?? default` / `if present` handling per provider.
#[derive(Clone, Debug, Default)]
pub struct RequestInput {
    pub model: String,
    pub messages: Vec<RequestMessage>,
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
    /// Whether the stream path is being built (`stream: true`) vs the
    /// non-streaming send (`stream: false`). Defaults to the streaming path.
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
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::UnknownProvider(p) => write!(f, "unknown provider: {p}"),
            BuildError::Deferred(p) => write!(f, "request builder deferred: {p}"),
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
    let manifest = registry
        .get_provider(provider)
        .ok_or_else(|| BuildError::UnknownProvider(provider.to_string()))?;
    let headers = auth_headers(manifest);

    // Google's genai endpoint is model-specific (`/models/{model}:…`) and its wire
    // body is the SDK reframing (W4.7d) — handled separately from the fixed-path
    // chat-completions / responses / anthropic builders.
    if provider == "GOOGLE" {
        let url = google::google_chat_url(&manifest.base_url, &input.model, input.stream);
        let body = google::build_google_wire_body(input)
            .map_err(|e| BuildError::Deferred(format!("GOOGLE wire framing: {e}")))?;
        return Ok(BuiltRequest {
            method: "POST".to_string(),
            url,
            headers,
            body,
        });
    }

    let url = format!("{}{}", manifest.base_url, manifest.endpoints.chat);
    let body = match provider {
        "ANTHROPIC" => anthropic::build_body(input),
        "OPENAI" => responses_api::build_openai_body(input),
        "GROK" => responses_api::build_grok_body(input),
        "DEEPSEEK" => chat_completions::build_deepseek_body(input),
        "Z_AI" => chat_completions::build_zai_body(input),
        "OPENROUTER" => chat_completions::build_openrouter_body(input),
        "OLLAMA" => chat_completions::build_ollama_body(input),
        "OPENAI_COMPATIBLE" => chat_completions::build_openai_compatible_body(input),
        other => return Err(BuildError::UnknownProvider(other.to_string())),
    };

    Ok(BuiltRequest {
        method: "POST".to_string(),
        url,
        headers,
        body,
    })
}

/// The provider's declared auth headers (v4 sets `defaultHeaders` on the SDK
/// client / the raw-fetch `headers`). The api-key VALUE is the transport's to
/// inject; this carries the header NAME + any fixed extras (e.g. anthropic's
/// `anthropic-version`).
fn auth_headers(manifest: &crate::provider_manifest::Manifest) -> Vec<(String, String)> {
    let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
    if let Some(extra) = &manifest.auth.extra {
        for (k, v) in extra {
            headers.push((k.clone(), v.clone()));
        }
    }
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
