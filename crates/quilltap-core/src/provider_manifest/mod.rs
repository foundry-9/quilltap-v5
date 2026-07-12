//! Provider manifest + registry core (wave 4 / W4.7a).
//!
//! v4 reaches every LLM provider through an npm plugin registered into a
//! singleton `provider-registry`. That dynamic-code-loading mechanism does not
//! survive the port (there is no Node, no `import()`, and shipping third-party
//! JS into the Rust core is the trust boundary we don't want). It is replaced by
//! the two-layer design of `docs/developer/porting/provider-manifest.md`:
//!
//! 1. A **declarative JSON manifest per provider** — everything that is *data*
//!    (display metadata, capabilities, attachment MIME lists, message-format
//!    support, cheap-model config, chars-per-token, default context window,
//!    fallback model lists, static pricing) plus two enum **discriminators**
//!    ([`StreamDecoder`], [`RequestTransform`]) that select — never define —
//!    compiled behavior. The manifests are GENERATED from v4's registered plugin
//!    metadata by `harness/oracle/providers/gen-provider-manifests.mjs`
//!    (transcription, not re-derivation — the tool-catalog precedent), embedded
//!    via [`include_str!`] and parsed once behind a [`LazyLock`].
//! 2. (later, W4.7b/c) a fixed compiled set of Rust stream decoders + request
//!    transforms the manifest selects by [`StreamDecoder`] / [`RequestTransform`].
//!
//! This module owns layer 1: the manifest schema (serde structs — deserialization
//! IS the schema validation, fail-loud with a typed [`ManifestError`] naming the
//! field, `schemaVersion` explicitly gated), the nine built-in manifests, the
//! [`Registry`] accessors reproducing v4's `provider-registry` convenience getters
//! (`get_provider` exact-case Map lookup — v4 does NOT resolve `legacyNames` in
//! lookup, they are display metadata only; the capability getters with their v4
//! defaults `charsPerToken` 3.5 / `defaultContextWindow` 8192 / `toolFormat`
//! "openai"), and [`rewrite_localhost_url`] (pure — the host-side gateway
//! resolution is injected).
//!
//! **Sans-IO.** No filesystem, no network, no dynamic loading. Third-party
//! manifest loading (fs, signing) is a design open item — the load/validate path
//! ([`Manifest::from_json`]) is built but ONLY the built-ins are wired. Manifest
//! pricing is the STATIC fallback tier, NOT the live fetcher (W4.7e).

mod rewrite;

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

pub use rewrite::rewrite_localhost_url;

// ============================================================================
// Enum discriminators (closed sets — W4.7b/c implement against these; renaming
// a variant is forbidden, adding one is fine)
// ============================================================================

/// The stream decoder a provider selects. Closed set — the five wire dialects of
/// `docs/developer/porting/provider-manifest.md` "The five stream decoders".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum StreamDecoder {
    /// OpenAI Chat Completions SSE (openai-compatible, deepseek, z-ai, openrouter).
    #[serde(rename = "chat-completions-sse")]
    ChatCompletionsSse,
    /// OpenAI Responses API SSE (openai, grok).
    #[serde(rename = "responses-api-sse")]
    ResponsesApiSse,
    /// Anthropic Messages SSE (anthropic).
    #[serde(rename = "anthropic-sse")]
    AnthropicSse,
    /// genai `generateContentStream` parts (google).
    #[serde(rename = "google-parts")]
    GoogleParts,
    /// Ollama newline-delimited JSON (not SSE).
    #[serde(rename = "ollama-ndjson")]
    OllamaNdjson,
}

/// The request-transform hook a provider selects. Closed set — the conditional,
/// stateful request-side logic of the design doc "the request-transform hooks".
/// `None` is the default for plain OpenAI-compatible endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum RequestTransform {
    /// No request transform (plain OpenAI-compatible envelope).
    #[serde(rename = "none")]
    None,
    /// Anthropic: mid-history cache breakpoint + tool-result batching + the
    /// adaptive-thinking / sampling-param rejection rules.
    #[serde(rename = "anthropic")]
    Anthropic,
    /// OpenAI: `previous_response_id` chaining + fallback-to-full-input.
    #[serde(rename = "openai")]
    Openai,
    /// Google: recursive JSON-Schema sanitizer + `thoughtSignature` round-trip.
    #[serde(rename = "google")]
    Google,
    /// DeepSeek: echo prior-turn `reasoning_content` on a tool-call turn.
    #[serde(rename = "deepseek")]
    Deepseek,
}

/// The tool-format type a provider uses (`ToolFormatType` in v4). Defaults to
/// [`ToolFormat::Openai`] when the manifest omits it (v4 `getToolFormat` default).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolFormat {
    Openai,
    Anthropic,
    Google,
}

/// A message role that can carry a `name` field (`system` never does).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

// ============================================================================
// Manifest schema (serde structs = the validation surface)
// ============================================================================

/// Provider auth shape. Carried as data for W4.7c/d (the transport layer);
/// `kind` is an open string here (the transport interprets it) so a future auth
/// shape needs no core change.
#[derive(Clone, Debug, Deserialize)]
pub struct AuthSpec {
    pub kind: String,
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default)]
    pub param: Option<String>,
    #[serde(default)]
    pub extra: Option<HashMap<String, String>>,
}

/// Request endpoints (chat + models). Data for W4.7c/d.
#[derive(Clone, Debug, Deserialize)]
pub struct Endpoints {
    pub chat: String,
    pub models: String,
}

/// UI color classes (Tailwind) — `ProviderMetadata.colors`.
#[derive(Clone, Debug, Deserialize)]
pub struct Colors {
    pub bg: String,
    pub text: String,
    pub icon: String,
}

/// Provider capability flags (`ProviderCapabilities`). `tool_use` defaults to
/// false (v4 `capabilities.toolUse?`).
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Capabilities {
    pub chat: bool,
    #[serde(rename = "imageGeneration")]
    pub image_generation: bool,
    pub embeddings: bool,
    #[serde(rename = "webSearch")]
    pub web_search: bool,
    #[serde(default, rename = "toolUse")]
    pub tool_use: bool,
}

/// Configuration requirements (`ProviderConfigRequirements`).
#[derive(Clone, Debug, Deserialize)]
pub struct ConfigRequirements {
    #[serde(rename = "requiresApiKey")]
    pub requires_api_key: bool,
    #[serde(rename = "requiresBaseUrl")]
    pub requires_base_url: bool,
    #[serde(default, rename = "apiKeyLabel")]
    pub api_key_label: Option<String>,
    #[serde(default, rename = "baseUrlLabel")]
    pub base_url_label: Option<String>,
    #[serde(default, rename = "baseUrlPlaceholder")]
    pub base_url_placeholder: Option<String>,
    #[serde(default, rename = "baseUrlDefault")]
    pub base_url_default: Option<String>,
}

/// Message-format support (`MessageFormatSupport`).
#[derive(Clone, Debug, Deserialize)]
pub struct MessageFormat {
    #[serde(rename = "supportsNameField")]
    pub supports_name_field: bool,
    #[serde(rename = "supportedRoles")]
    pub supported_roles: Vec<MessageRole>,
    #[serde(default, rename = "maxNameLength")]
    pub max_name_length: Option<i64>,
}

/// Cheap-model configuration (`CheapModelConfig`).
#[derive(Clone, Debug, Deserialize)]
pub struct CheapModels {
    #[serde(rename = "defaultModel")]
    pub default_model: String,
    #[serde(rename = "recommendedModels")]
    pub recommended_models: Vec<String>,
}

/// Attachment support (`AttachmentSupport`).
#[derive(Clone, Debug, Deserialize)]
pub struct Attachment {
    #[serde(rename = "supportsAttachments")]
    pub supports_attachments: bool,
    #[serde(rename = "supportedMimeTypes")]
    pub supported_mime_types: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default, rename = "maxFileSize")]
    pub max_file_size: Option<i64>,
    #[serde(default, rename = "maxBase64Size")]
    pub max_base64_size: Option<i64>,
    #[serde(default, rename = "maxFiles")]
    pub max_files: Option<i64>,
}

/// A static pricing row (per 1M tokens). The STATIC fallback tier only — the
/// live pricing fetcher is W4.7e.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Pricing {
    pub input: f64,
    pub output: f64,
}

/// A single provider manifest — the declarative data layer. Field order mirrors
/// the generator's emission order (documentation only; serde deserialization is
/// order-independent). Deserialization IS the validation: a missing required
/// field, a bad enum discriminator, or (via [`Manifest::from_json`]) a wrong
/// `schemaVersion` each fails loud with a typed [`ManifestError`].
#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    /// Canonical provider id (v4 `metadata.providerName`, UPPERCASE, e.g.
    /// `"ANTHROPIC"`, `"Z_AI"`). The registry keys by this exactly.
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub description: String,
    pub abbreviation: String,
    pub colors: Colors,
    /// Legacy aliases (display metadata only — v4 does NOT resolve these in
    /// `getProvider`; kept for the image-profiles UI surface).
    #[serde(default, rename = "legacyNames")]
    pub legacy_names: Vec<String>,
    pub auth: AuthSpec,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    pub endpoints: Endpoints,
    #[serde(rename = "streamDecoder")]
    pub stream_decoder: StreamDecoder,
    #[serde(rename = "requestTransform")]
    pub request_transform: RequestTransform,
    #[serde(rename = "toolFormat")]
    pub tool_format: ToolFormat,
    pub capabilities: Capabilities,
    #[serde(rename = "configRequirements")]
    pub config_requirements: ConfigRequirements,
    #[serde(rename = "messageFormat")]
    pub message_format: MessageFormat,
    /// `null` when the provider declares no cheap-model config.
    #[serde(default, rename = "cheapModels")]
    pub cheap_models: Option<CheapModels>,
    pub attachment: Attachment,
    #[serde(rename = "charsPerToken")]
    pub chars_per_token: f64,
    #[serde(rename = "defaultContextWindow")]
    pub default_context_window: i64,
    /// The provider's declared model ids (`getModelInfo().map(m => m.id)`).
    #[serde(default, rename = "fallbackModels")]
    pub fallback_models: Vec<String>,
    /// The provider's declared image-generation model ids — v4's
    /// `getImageGenerationModels().map(m => m.id)` (else the image provider's
    /// `supportedModels`). Backs the `imageProfileList` `list-providers`
    /// `defaultModels` (P4.6p); empty on providers without image generation.
    #[serde(default, rename = "imageGenerationModels")]
    pub image_generation_models: Vec<String>,
    /// Static fallback pricing by model id. Empty on every built-in today (the
    /// live fetcher is W4.7e); a manifest MAY carry rows.
    #[serde(default)]
    pub pricing: HashMap<String, Pricing>,
}

/// A typed load/validate error naming the failure (fail-loud, never half-load).
#[derive(Debug)]
pub enum ManifestError {
    /// `schemaVersion` is not the supported version (carries the found value).
    UnsupportedSchemaVersion(u32),
    /// serde deserialization failed — a missing required field, a bad enum
    /// discriminator, or a type mismatch. The message names the offending field.
    Invalid(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::UnsupportedSchemaVersion(v) => {
                write!(
                    f,
                    "unsupported manifest schemaVersion: {v} (expected {SUPPORTED_SCHEMA_VERSION})"
                )
            }
            ManifestError::Invalid(msg) => write!(f, "invalid provider manifest: {msg}"),
        }
    }
}

impl std::error::Error for ManifestError {}

/// The only manifest schema version this build understands.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

impl Manifest {
    /// Load + validate a manifest from JSON. Deserialization is the schema check
    /// (missing field / bad enum → [`ManifestError::Invalid`] naming the field);
    /// `schemaVersion` is then gated explicitly ([`ManifestError::UnsupportedSchemaVersion`]).
    /// This is the third-party load path — built-ins ride it too, but no fs /
    /// network / signing happens in the core (a design open item).
    pub fn from_json(json: &str) -> Result<Manifest, ManifestError> {
        let manifest: Manifest =
            serde_json::from_str(json).map_err(|e| ManifestError::Invalid(e.to_string()))?;
        if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchemaVersion(
                manifest.schema_version,
            ));
        }
        Ok(manifest)
    }

    /// Model pricing for a model id (v4 `getModelPricing`): the manifest's static
    /// pricing row, or `None`.
    pub fn model_pricing(&self, model_id: &str) -> Option<Pricing> {
        self.pricing.get(model_id).copied()
    }
}

// ============================================================================
// The built-in manifests (embedded + parsed once)
// ============================================================================

/// The nine built-in manifest JSON blobs, in v4 registration order. Embedded via
/// [`include_str!`] from the generated files under `manifests/` (regen with the
/// generator — see this module's header / the generator's header).
const BUILT_IN_MANIFEST_JSON: &[&str] = &[
    include_str!("manifests/anthropic.json"),
    include_str!("manifests/openai.json"),
    include_str!("manifests/google.json"),
    include_str!("manifests/grok.json"),
    include_str!("manifests/deepseek.json"),
    include_str!("manifests/z_ai.json"),
    include_str!("manifests/openrouter.json"),
    include_str!("manifests/ollama.json"),
    include_str!("manifests/openai_compatible.json"),
];

/// The parsed built-in registry, in registration order. Parsed once. A malformed
/// built-in manifest is a build-time-committed bug → panic with the field, never
/// a half-load (the fail-loud rule).
static BUILT_IN_REGISTRY: LazyLock<Registry> = LazyLock::new(|| {
    let providers: Vec<Manifest> = BUILT_IN_MANIFEST_JSON
        .iter()
        .map(|json| Manifest::from_json(json).expect("built-in provider manifest is valid"))
        .collect();
    Registry { providers }
});

// ============================================================================
// The registry (v4 provider-registry convenience getters)
// ============================================================================

/// A provider registry: an ordered set of [`Manifest`]s keyed by canonical id.
/// Reproduces v4's `provider-registry` convenience getters. v4's registry is
/// HMR-stateful (globalThis); none of that ports — this is static data +
/// accessors.
#[derive(Clone, Debug)]
pub struct Registry {
    providers: Vec<Manifest>,
}

impl Registry {
    /// The nine built-in providers.
    pub fn built_in() -> &'static Registry {
        &BUILT_IN_REGISTRY
    }

    /// A registry over an arbitrary manifest set (third-party path — no fs here).
    pub fn from_manifests(providers: Vec<Manifest>) -> Registry {
        Registry { providers }
    }

    /// `getProvider(name)` — EXACT-CASE lookup by canonical id. v4's registry is a
    /// `Map.get(providerName)`; it does NOT resolve `legacyNames` (those are
    /// display metadata). Returns `None` for an unknown / legacy id.
    pub fn get_provider(&self, name: &str) -> Option<&Manifest> {
        self.providers.iter().find(|p| p.id == name)
    }

    /// `getAllProviders()` — every provider, in registration order.
    pub fn all_providers(&self) -> &[Manifest] {
        &self.providers
    }

    /// `hasProvider(name)`.
    pub fn has_provider(&self, name: &str) -> bool {
        self.get_provider(name).is_some()
    }

    /// `getProviderNames()`, in registration order.
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.iter().map(|p| p.id.as_str()).collect()
    }

    /// `supportsCapability(name, capability)` — false for an unknown provider.
    pub fn supports_capability(&self, name: &str, capability: Capability) -> bool {
        self.get_provider(name)
            .map(|p| p.capabilities.has(capability))
            .unwrap_or(false)
    }

    /// `getAttachmentSupport(name)` — `None` for an unknown provider.
    pub fn attachment_support(&self, name: &str) -> Option<&Attachment> {
        self.get_provider(name).map(|p| &p.attachment)
    }

    /// `getMessageFormat(name)` — the provider's declared message format, or the
    /// empty default `{ supportsNameField: false, supportedRoles: [] }` for an
    /// unknown provider (v4's `?? { supportsNameField: false, supportedRoles: [] }`).
    pub fn message_format(&self, name: &str) -> MessageFormat {
        self.get_provider(name)
            .map(|p| p.message_format.clone())
            .unwrap_or(MessageFormat {
                supports_name_field: false,
                supported_roles: vec![],
                max_name_length: None,
            })
    }

    /// `getCharsPerToken(name)` — the provider's value, default **3.5**.
    pub fn chars_per_token(&self, name: &str) -> f64 {
        self.get_provider(name)
            .map(|p| p.chars_per_token)
            .unwrap_or(3.5)
    }

    /// `getToolFormat(name)` — the provider's value, default [`ToolFormat::Openai`].
    pub fn tool_format(&self, name: &str) -> ToolFormat {
        self.get_provider(name)
            .map(|p| p.tool_format)
            .unwrap_or(ToolFormat::Openai)
    }

    /// `getCheapModelConfig(name)` — `None` for an unknown provider or one that
    /// declares no cheap-model config.
    pub fn cheap_model_config(&self, name: &str) -> Option<&CheapModels> {
        self.get_provider(name)
            .and_then(|p| p.cheap_models.as_ref())
    }

    /// `getDefaultContextWindow(name)` — the provider's value, default **8192**.
    pub fn default_context_window(&self, name: &str) -> i64 {
        self.get_provider(name)
            .map(|p| p.default_context_window)
            .unwrap_or(8192)
    }

    /// `getModelPricing(providerName, modelId)` — the static pricing row for a
    /// model, or `None` (unknown provider, unknown model, or no static row —
    /// which is every built-in today; the live fetcher is W4.7e).
    pub fn model_pricing(&self, provider_name: &str, model_id: &str) -> Option<Pricing> {
        self.get_provider(provider_name)
            .and_then(|p| p.model_pricing(model_id))
    }
}

/// A capability flag selector for [`Registry::supports_capability`], mirroring
/// v4's `keyof DEFAULT_CAPABILITIES` (`chat` / `imageGeneration` / `embeddings` /
/// `webSearch` / `toolUse`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Chat,
    ImageGeneration,
    Embeddings,
    WebSearch,
    ToolUse,
}

impl Capabilities {
    fn has(&self, capability: Capability) -> bool {
        match capability {
            Capability::Chat => self.chat,
            Capability::ImageGeneration => self.image_generation,
            Capability::Embeddings => self.embeddings,
            Capability::WebSearch => self.web_search,
            Capability::ToolUse => self.tool_use,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_parse_and_count() {
        let reg = Registry::built_in();
        assert_eq!(reg.all_providers().len(), 9);
        assert_eq!(
            reg.provider_names(),
            vec![
                "ANTHROPIC",
                "OPENAI",
                "GOOGLE",
                "GROK",
                "DEEPSEEK",
                "Z_AI",
                "OPENROUTER",
                "OLLAMA",
                "OPENAI_COMPATIBLE",
            ]
        );
    }

    #[test]
    fn get_provider_is_exact_case_and_ignores_legacy() {
        let reg = Registry::built_in();
        assert!(reg.get_provider("ANTHROPIC").is_some());
        // exact case only — lowercase misses (v4 keys by providerName)
        assert!(reg.get_provider("anthropic").is_none());
        // legacyNames are NOT resolved by getProvider
        assert!(reg.get_provider("GOOGLE_IMAGEN").is_none());
        assert_eq!(
            reg.get_provider("GOOGLE").unwrap().legacy_names,
            vec!["GOOGLE_IMAGEN".to_string()]
        );
    }

    #[test]
    fn capability_defaults_for_unknown_provider() {
        let reg = Registry::built_in();
        assert!(!reg.supports_capability("NOPE", Capability::Chat));
        assert_eq!(reg.chars_per_token("NOPE"), 3.5);
        assert_eq!(reg.default_context_window("NOPE"), 8192);
        assert_eq!(reg.tool_format("NOPE"), ToolFormat::Openai);
        assert!(reg.cheap_model_config("NOPE").is_none());
        let mf = reg.message_format("NOPE");
        assert!(!mf.supports_name_field);
        assert!(mf.supported_roles.is_empty());
    }

    #[test]
    fn decoder_transform_enums_match_design() {
        let reg = Registry::built_in();
        assert_eq!(
            reg.get_provider("ANTHROPIC").unwrap().stream_decoder,
            StreamDecoder::AnthropicSse
        );
        assert_eq!(
            reg.get_provider("ANTHROPIC").unwrap().request_transform,
            RequestTransform::Anthropic
        );
        assert_eq!(
            reg.get_provider("OLLAMA").unwrap().stream_decoder,
            StreamDecoder::OllamaNdjson
        );
        assert_eq!(
            reg.get_provider("DEEPSEEK").unwrap().request_transform,
            RequestTransform::Deepseek
        );
        assert_eq!(
            reg.get_provider("Z_AI").unwrap().request_transform,
            RequestTransform::None
        );
    }

    #[test]
    fn model_pricing_empty_on_builtins() {
        let reg = Registry::built_in();
        assert!(reg.model_pricing("ANTHROPIC", "claude-opus-4-6").is_none());
        assert!(reg.model_pricing("NOPE", "x").is_none());
    }

    // ---- fail-loud validation --------------------------------------------

    #[test]
    fn from_json_rejects_wrong_schema_version() {
        let json = r#"{
            "schemaVersion": 2, "id": "X", "displayName": "X", "description": "",
            "abbreviation": "X", "colors": {"bg":"","text":"","icon":""},
            "auth": {"kind":"bearer"}, "baseUrl": "", "endpoints": {"chat":"/c","models":"/m"},
            "streamDecoder": "chat-completions-sse", "requestTransform": "none",
            "toolFormat": "openai",
            "capabilities": {"chat":true,"imageGeneration":false,"embeddings":false,"webSearch":false},
            "configRequirements": {"requiresApiKey":true,"requiresBaseUrl":false},
            "messageFormat": {"supportsNameField":false,"supportedRoles":[]},
            "attachment": {"supportsAttachments":false,"supportedMimeTypes":[],"description":""},
            "charsPerToken": 3.5, "defaultContextWindow": 8192
        }"#;
        match Manifest::from_json(json) {
            Err(ManifestError::UnsupportedSchemaVersion(2)) => {}
            other => panic!("expected UnsupportedSchemaVersion(2), got {other:?}"),
        }
    }

    #[test]
    fn from_json_rejects_missing_field() {
        // no `id`
        let json = r#"{
            "schemaVersion": 1, "displayName": "X", "description": "",
            "abbreviation": "X", "colors": {"bg":"","text":"","icon":""},
            "auth": {"kind":"bearer"}, "baseUrl": "", "endpoints": {"chat":"/c","models":"/m"},
            "streamDecoder": "chat-completions-sse", "requestTransform": "none",
            "toolFormat": "openai",
            "capabilities": {"chat":true,"imageGeneration":false,"embeddings":false,"webSearch":false},
            "configRequirements": {"requiresApiKey":true,"requiresBaseUrl":false},
            "messageFormat": {"supportsNameField":false,"supportedRoles":[]},
            "attachment": {"supportsAttachments":false,"supportedMimeTypes":[],"description":""},
            "charsPerToken": 3.5, "defaultContextWindow": 8192
        }"#;
        match Manifest::from_json(json) {
            Err(ManifestError::Invalid(msg)) => {
                assert!(msg.contains("id"), "msg names field: {msg}")
            }
            other => panic!("expected Invalid naming `id`, got {other:?}"),
        }
    }

    #[test]
    fn from_json_rejects_bad_enum() {
        // streamDecoder is not a known variant
        let json = r#"{
            "schemaVersion": 1, "id": "X", "displayName": "X", "description": "",
            "abbreviation": "X", "colors": {"bg":"","text":"","icon":""},
            "auth": {"kind":"bearer"}, "baseUrl": "", "endpoints": {"chat":"/c","models":"/m"},
            "streamDecoder": "not-a-real-decoder", "requestTransform": "none",
            "toolFormat": "openai",
            "capabilities": {"chat":true,"imageGeneration":false,"embeddings":false,"webSearch":false},
            "configRequirements": {"requiresApiKey":true,"requiresBaseUrl":false},
            "messageFormat": {"supportsNameField":false,"supportedRoles":[]},
            "attachment": {"supportsAttachments":false,"supportedMimeTypes":[],"description":""},
            "charsPerToken": 3.5, "defaultContextWindow": 8192
        }"#;
        match Manifest::from_json(json) {
            Err(ManifestError::Invalid(msg)) => {
                assert!(
                    msg.contains("streamDecoder") || msg.contains("not-a-real-decoder"),
                    "msg: {msg}"
                )
            }
            other => panic!("expected Invalid for bad enum, got {other:?}"),
        }
    }
}
