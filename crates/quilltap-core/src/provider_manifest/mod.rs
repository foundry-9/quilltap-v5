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
//! field, `schemaVersion` explicitly gated), the ten built-in manifests, the
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

use serde::{Deserialize, Serialize};
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
    /// v4 `acceptsApiKey` (bug 81) — whether a key *may* be attached at all, as
    /// against `requiresApiKey`'s "must it be?". **Optional by design:** omitted
    /// means "the same answer as `requiresApiKey`", which is correct for every
    /// provider that is wholly hosted or wholly local, so no manifest that
    /// predates the field changes behavior. OpenAI-Compatible is the one that
    /// spans both — an unauthenticated llama.cpp on localhost and a hosted
    /// endpoint behind a bearer token — and declares `false`/`true`.
    ///
    /// Read it through [`ConfigRequirements::accepts_api_key`], never by hand:
    /// the fallback rule has exactly one home.
    #[serde(
        default,
        rename = "acceptsApiKey",
        skip_serializing_if = "Option::is_none"
    )]
    pub accepts_api_key: Option<bool>,
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

impl ConfigRequirements {
    /// v4 `providerAcceptsApiKey` (`lib/llm/api-key-support.ts`) — whether a
    /// provider *may* hold an API key at all.
    ///
    /// The fallback is [`Self::requires_api_key`] rather than a bare `true`: a
    /// provider that requires a key necessarily accepts one, and an Ollama
    /// endpoint has nowhere to put a bearer token and should not be offered the
    /// field. Truth table (v4's): `{req:true}` → accepts; `{req:false}` →
    /// refuses; `{req:false, acc:true}` → accepts (OpenAI-Compatible).
    pub fn accepts_api_key(&self) -> bool {
        self.accepts_api_key.unwrap_or(self.requires_api_key)
    }
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

/// How the host can tell whether a connection profile on this provider will
/// run a reasoning ("thinking") turn (v4 `ThinkingTurnRule`,
/// `@quilltap/plugin-types` 2.5.8, `97d2fcb5`).
///
/// Thinking changes what a request may look like. Two providers are already on
/// record refusing an assistant `[Name]` prefill *only* while thinking: Ollama
/// never opens the reasoning block behind a prefilled turn, and DeepSeek 400s
/// on continuing a thinking turn whose `reasoning_content` it never saw. The
/// host needs a per-profile answer to seed the right multi-character turn
/// anchor, and only the plugin knows which option key it reads.
///
/// Deliberately declarative rather than a predicate function: the same answer
/// is needed in the connection-profile editor, which runs in the browser and
/// cannot call into a server-side plugin. A rule serialises; a closure does
/// not.
///
/// The rule answers only the *explicit* half — "has this profile switched
/// thinking on or off?". When the profile says nothing, the host falls back to
/// the selected model's `thinksByDefault` flag. The value lists are carried as
/// opaque JSON scalars (v4's type is `(string | number | boolean)[]`) so the
/// wire re-serializes them byte-for-byte; the evaluator in
/// [`crate::services::thinking_turn`] compares them with JS `===` semantics.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ThinkingTurnRule {
    /// The `parameters` key on the connection profile that switches thinking
    /// on or off. Matches a field key from the provider's options schema.
    #[serde(rename = "optionKey")]
    pub option_key: String,
    /// Values of that key meaning thinking is ON.
    #[serde(
        default,
        rename = "enabledValues",
        skip_serializing_if = "Option::is_none"
    )]
    pub enabled_values: Option<Vec<serde_json::Value>>,
    /// Values of that key meaning thinking is OFF.
    #[serde(
        default,
        rename = "disabledValues",
        skip_serializing_if = "Option::is_none"
    )]
    pub disabled_values: Option<Vec<serde_json::Value>>,
}

/// The two model facts the thinking-turn question cares about, as the wire
/// carries them (v4 `ThinkingModelFacts` — the `ModelInfo` subset that
/// `evaluateThinkingTurn` and the models-fetch echo read).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct ThinkingModelFacts {
    /// Whether this model is capable of a reasoning ("thinking") turn at all.
    /// Distinct from `thinksByDefault`: a model may be capable of thinking yet
    /// only do so when the profile asks for it.
    #[serde(
        default,
        rename = "supportsThinking",
        skip_serializing_if = "Option::is_none"
    )]
    pub supports_thinking: Option<bool>,
    /// Whether this model runs a thinking turn **without being asked** — i.e.
    /// with no thinking option set on the connection profile. The host uses
    /// this as the fallback answer when the profile sets no thinking option of
    /// its own.
    #[serde(
        default,
        rename = "thinksByDefault",
        skip_serializing_if = "Option::is_none"
    )]
    pub thinks_by_default: Option<bool>,
}

/// One model-catalogue row's thinking facts (the `models` manifest field —
/// v4's `getModelInfo()` rows narrowed to what the wire and the evaluator
/// read).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelThinkingEntry {
    pub id: String,
    #[serde(flatten)]
    pub facts: ThinkingModelFacts,
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
    /// The plugin's connection-profile options schema (v4
    /// `plugin.getProviderOptionsSchema?.()`), served verbatim on the providers
    /// listing — `null` exactly when the plugin declares none (google only, at
    /// the `93ed8abf` pin).
    ///
    /// Deliberately an opaque `serde_json::Value` rather than a typed tree: the shape is
    /// v4's `packages/plugin-types/.../provider-options.ts`, the generator
    /// EXTRACTS it from the built plugin object, and the listing hands it
    /// straight to the renderer. A typed mirror would have to reproduce v4's key
    /// ORDER to stay byte-identical and would rot the moment v4 adds a field —
    /// where an opaque carry (with `preserve_order` keeping insertion order)
    /// cannot. P4.D84 narrows its own client-side type from the contract text.
    #[serde(default, rename = "optionsSchema")]
    pub options_schema: Option<serde_json::Value>,
    /// The plugin's declared thinking-turn rule (v4 `plugin.thinkingTurnRule`,
    /// bug 85) — which `parameters` key switches reasoning on or off, and
    /// which values mean which. `None` where the plugin declares none (every
    /// built-in but deepseek and ollama at `12fe3e6f`); the providers listing
    /// serves `?? null`. Positioned after `optionsSchema` to mirror both the
    /// generator's emission order and v4's wire.
    #[serde(default, rename = "thinkingTurnRule")]
    pub thinking_turn_rule: Option<ThinkingTurnRule>,
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
    /// The thinking facts from the plugin's model catalogue (v4 bug 85 —
    /// `getModelInfo()` rows carrying `supportsThinking` / `thinksByDefault`).
    /// The generator emits only fact-bearing entries and omits the key when
    /// none exist (deepseek's two V4 models are the only rows at `12fe3e6f`):
    /// a fact-less entry is observably identical to no entry on every consumer
    /// — the evaluator tests `thinksByDefault == Some(true)` and the
    /// models-fetch echo drops absent keys exactly as v4's
    /// `staticInfo?.…` spread does.
    #[serde(default)]
    pub models: Vec<ModelThinkingEntry>,
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

/// The ten built-in manifest JSON blobs, in v4 registration order. Embedded via
/// [`include_str!`] from the generated files under `manifests/` (regen with the
/// generator — see this module's header / the generator's header).
///
/// NanoGPT (P4.D101) is APPENDED, not slotted alphabetically: the registration
/// order is compared positionally by both `provider_registry_equivalence`
/// (`names`) and `providers_listing_equivalence` (a zip over the two lists), so
/// appending keeps all nine pre-existing rows byte-identical on both sides. The
/// oracle cases' `PLUGIN_DIRS` lists carry the same append.
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
    include_str!("manifests/nanogpt.json"),
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
    /// The ten built-in providers.
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

    /// The provider's declared thinking-turn rule (v4 `plugin.thinkingTurnRule`,
    /// bug 85), or `None` for an unknown provider or one declaring no rule.
    pub fn thinking_turn_rule(&self, name: &str) -> Option<&ThinkingTurnRule> {
        self.get_provider(name)
            .and_then(|p| p.thinking_turn_rule.as_ref())
    }

    /// The thinking facts for a model, by EXACT id match against the provider's
    /// catalogue (v4 `plugin.getModelInfo?.().find(m => m.id === modelName)`,
    /// narrowed to the two facts). `None` for an unknown provider or an
    /// uncatalogued id — which v4 also answers for a catalogued model carrying
    /// neither fact, since the manifest omits fact-less entries and both
    /// shapes evaluate identically.
    pub fn model_thinking_facts(&self, name: &str, model_id: &str) -> Option<ThinkingModelFacts> {
        self.get_provider(name)
            .and_then(|p| p.models.iter().find(|m| m.id == model_id))
            .map(|m| m.facts)
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
        assert_eq!(reg.all_providers().len(), 10);
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
                "NANOGPT",
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

    /// v4 `providerAcceptsApiKey`'s truth table
    /// (`__tests__/unit/lib/services/api-key-service.test.ts`'s premise, and the
    /// pure predicate's own contract): `{req:true}` → T; `{req:false}` → F;
    /// `{req:false, acc:true}` → T. The fallback is the OTHER flag, never a
    /// bare `true` — an Ollama endpoint has nowhere to put a bearer token.
    #[test]
    fn accepts_api_key_falls_back_to_requires() {
        fn reqs(requires: bool, accepts: Option<bool>) -> ConfigRequirements {
            ConfigRequirements {
                requires_api_key: requires,
                accepts_api_key: accepts,
                requires_base_url: false,
                api_key_label: None,
                base_url_label: None,
                base_url_placeholder: None,
                base_url_default: None,
            }
        }
        assert!(reqs(true, None).accepts_api_key(), "hosted: must → may");
        assert!(
            !reqs(false, None).accepts_api_key(),
            "local: must not → may not"
        );
        assert!(
            reqs(false, Some(true)).accepts_api_key(),
            "OAC: need not, but may"
        );
        // The declared value wins in the other direction too, though no shipped
        // plugin declares it: a provider could refuse a key it is not required
        // to hold.
        assert!(!reqs(true, Some(false)).accepts_api_key());
    }

    /// The ten committed manifests, as the field actually ships: OpenAI-Compatible
    /// is the ONE that declares `acceptsApiKey`, and it is the ONE whose two
    /// answers differ. Shape, not a hand count — a generator that started
    /// emitting the key everywhere (or dropped it) fails here.
    #[test]
    fn only_openai_compatible_declares_accepts_api_key() {
        let registry = Registry::built_in();
        let declaring: Vec<&str> = registry
            .all_providers()
            .iter()
            .filter(|m| m.config_requirements.accepts_api_key.is_some())
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(declaring, vec!["OPENAI_COMPATIBLE"]);

        let split: Vec<&str> = registry
            .all_providers()
            .iter()
            .filter(|m| {
                m.config_requirements.accepts_api_key() != m.config_requirements.requires_api_key
            })
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(split, vec!["OPENAI_COMPATIBLE"]);
        let oac = registry.get_provider("OPENAI_COMPATIBLE").unwrap();
        assert!(!oac.config_requirements.requires_api_key);
        assert!(oac.config_requirements.accepts_api_key());
    }
}
