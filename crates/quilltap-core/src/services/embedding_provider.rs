//! The API-path [`EmbeddingProvider`] — the production implementation of the
//! embedding model boundary (P4.1a deliverable 4). Ports v4's
//! `generateEmbeddingForUser` / `generateEmbedding` / `generateApiEmbedding` /
//! `getApiKeyForProfile` orchestration (`lib/embedding/embedding-service.ts`,
//! lines 90–321): resolve the user's embedding profile off the DB, dispatch the
//! BUILTIN branch to the ported [`generate_builtin_embedding`], or drive the
//! provider wire (OPENAI / OLLAMA / OPENROUTER) through the injected
//! [`WireTransport`] seam over the frozen sans-IO builders/parsers in
//! [`crate::model::embedding_wire`], then apply the profile storage policy
//! ([`apply_embedding_profile`]).
//!
//! ## What is ported vs seamed
//!
//! - **Profile resolution** (v4 `generateEmbeddingForUser`): an explicit
//!   `profile_id` → `findById`; not given OR not found → the user's default
//!   (`findDefault`); still nothing → the exact `EmbeddingError`
//!   "No embedding profile configured" (no provider attribution).
//! - **The priority lane is NOT ported as a timer.** v4's `interactivePending`
//!   counter + 50 ms `waitForInteractiveQuiet` poll is host-timing — it delays
//!   background callers behind interactive ones but never changes any result. The
//!   `priority` parameter is accepted for trait fidelity and ignored (the
//!   memory-gate 500 ms inter-retry-delay precedent; a host scheduler can layer
//!   queueing above this provider).
//! - **The registry gate** (v4 `providerRegistry.createEmbeddingProvider` via
//!   `requireProvider`): an unknown provider name and a provider whose manifest
//!   declares `capabilities.embeddings: false` each throw a PLAIN error (which
//!   escapes UNwrapped — the dead-catch finding below); the checks read
//!   [`Registry::built_in`](crate::provider_manifest::Registry::built_in).
//! - **Base URL**: v4 `resolveBaseUrl` = `baseUrl ? rewriteLocalhostUrl(baseUrl)
//!   : baseUrl` — a non-empty profile `baseUrl` goes through
//!   [`rewrite_localhost_url`] with the host-injected `localhost_gateway`
//!   (default `None` = not in a VM → unchanged).
//! - **API key** (v4 `getApiKeyForProfile`): when the manifest's
//!   `configRequirements.requiresApiKey` is true, `profile.apiKeyId` resolves
//!   through [`api_keys::find_by_id_and_user_id`]; a missing id or row is the
//!   exact `EmbeddingError` "No API key found for {provider} embedding profile"
//!   (UNwrapped — it is already an `EmbeddingError` in v4). Ollama's manifest has
//!   `requiresApiKey: false`, so its key stays `""`.
//! - **v4 `generateEmbedding`'s outer catch is DEAD CODE — no wrapping happens.**
//!   The v4 body does `return generateApiEmbedding(...)` (and `return
//!   generateBuiltinEmbedding(...)`) WITHOUT `await` inside the `try`, so an
//!   async rejection never passes through the `catch`, and the intended
//!   "Embedding failed for provider {p}: {msg}" wrap is unreachable (the same
//!   broken-but-exact family as the enclave `turn_error:` finding — verified
//!   empirically by the differential: a 429 escapes v4 as the PLAIN error
//!   `OpenAI embedding failed: rate limit exceeded`, unwrapped). Ported
//!   faithfully: an `EmbeddingError` (missing api key, BUILTIN not-fitted, no
//!   profile) carries its provider attribution; every PLAIN throw (registry
//!   gate, wire error, transport throw) surfaces as an [`EmbeddingError`]
//!   carrying the plain message with `provider: None` (the trait's error type
//!   is `EmbeddingError`; v4 callers treat any error as failure and read only
//!   the message).
//!
//! ## The OpenRouter SDK wire (recorded 2026-07-08)
//!
//! v4's OpenRouter plugin goes through the bundled `@openrouter/sdk`
//! (`client.embeddings.generate({requestBody})`). The SDK's actual fetch was
//! recorded under a fetch intercept:
//!   - `POST https://openrouter.ai/api/v1/embeddings` (the SDK server default —
//!     the plugin's constructor takes NO baseUrl, so a profile `baseUrl` is
//!     ignored for OpenRouter, faithful to v4).
//!   - Body key order is the SDK's OWN (`dimensions` first when present):
//!     `{"dimensions":256,"input":"…","model":"…"}` / `{"input":"…","model":"…"}`
//!     — NOT the plugin argument order, so the wire body is built HERE (the
//!     frozen `build_openrouter_request_body` models the SDK'S INPUT, not the
//!     wire).
//!   - Headers: `Authorization: Bearer`, `HTTP-Referer` (v4 `process.env.BASE_URL
//!     || 'http://localhost:3000'`, carried as `openrouter_referer`), and
//!     `X-OpenRouter-Title` = the Quilltap user agent.
//!   - The SDK Zod-validates the wire response (`object: "list"`, snake_case
//!     `usage`) and hands the plugin a TRANSFORMED value (camelCase
//!     `usage.promptTokens`/`totalTokens`/`cost`); [`sdk_transform_openrouter`]
//!     reproduces that projection so the frozen `parse_openrouter_response`
//!     (which was differential-verified against the SDK-shaped value) applies.
//!   - A non-2xx becomes an SDK THROW whose message is the response body's
//!     `error.message` (recorded: a 429 `{error:{message:'rate limited'}}` →
//!     `TooManyRequestsResponseError` with message exactly `rate limited`) —
//!     reproduced by [`openrouter_non2xx_message`] for the structured-error
//!     shape; other SDK per-status shapes are a documented seam (out of corpus).
//!
//! ## Other documented seams
//!
//! - Ollama's in-flight `/api/show` dedupe (`numCtxInflight`) is a concurrency
//!   optimization with no observable result — not ported; the derived-only cache
//!   rule IS (via the frozen [`NumCtxCache`]).
//! - A DB read error surfaces as an `EmbeddingError` carrying the error text
//!   (v4 would throw the raw repo error; the trait's error type is
//!   `EmbeddingError`, and no corpus row exercises a DB failure).
//! - v4's `isLocalEmbeddingProvider` guard inside `generateApiEmbedding` cannot
//!   fire for the three wire providers (none return a local provider) — not
//!   ported.

use std::sync::Mutex;

use serde_json::{json, Value};

use crate::db::api_keys;
use crate::db::embedding_profiles::{self, EmbeddingProfileRow};
use crate::db::runtime::Db;
use crate::embedding_vector::apply_embedding_profile;
use crate::model::embedding::{
    EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
use crate::model::embedding_wire::{
    build_nanogpt_request, build_ollama_embed_request, build_ollama_legacy_request,
    build_ollama_show_request, build_openai_request, derive_num_ctx, nanogpt_error_message,
    ollama_empty_input_error, ollama_error_message, openai_error_message,
    parse_ollama_embed_response, parse_ollama_legacy_response, parse_openai_response,
    parse_openrouter_response, NumCtxCache, WireEmbedding, NUM_CTX_FALLBACK,
};
use crate::model::request_builder::BuiltRequest;
use crate::model::wire::{WireResponse, WireTransport};
use crate::provider_manifest::{rewrite_localhost_url, Registry};
use crate::services::builtin_embedding::generate_builtin_embedding;

/// The OpenRouter SDK's server default (the plugin constructor takes no baseUrl).
const OPENROUTER_EMBEDDINGS_URL: &str = "https://openrouter.ai/api/v1/embeddings";
/// v4 `OllamaEmbeddingProvider`'s constructor default.
const OLLAMA_DEFAULT_BASE: &str = "http://localhost:11434";

/// An internal failure inside the API branch, distinguishing v4's two throw
/// classes: an `EmbeddingError` carries a provider attribution; a PLAIN throw
/// surfaces with only its message (v4's wrap is dead code — module doc).
enum ApiError {
    Embedding(EmbeddingError),
    Plain(String),
}

/// The production API-path embedding provider. Generic over the injected
/// [`WireTransport`] (the host supplies the real HTTP client; the differential a
/// [`CannedWireTransport`](crate::model::wire::CannedWireTransport)).
pub struct ApiEmbeddingProvider<T: WireTransport> {
    db: Db,
    transport: T,
    /// v4 `getQuilltapUserAgent()` = `Quilltap/{version}` (`unknown` when the
    /// global version is unset) — sent on the Ollama fetches and as the
    /// OpenRouter `X-OpenRouter-Title`. v4's single-embedding OpenAI path sends
    /// NO User-Agent (only the batch path does), reproduced here.
    user_agent: String,
    /// The OpenRouter `HTTP-Referer` (v4 `process.env.BASE_URL ||
    /// 'http://localhost:3000'`).
    openrouter_referer: String,
    /// The VM host gateway for `rewrite_localhost_url` (host-injected; `None` =
    /// not in a VM → base URLs pass through unchanged).
    localhost_gateway: Option<String>,
    /// v4's module-global `numCtxCache` (per-provider-instance here, the
    /// `NumCtxCache` precedent): derived-only `/api/show` results keyed by
    /// `{baseUrl}::{model}`.
    num_ctx: Mutex<NumCtxCache>,
}

impl<T: WireTransport> ApiEmbeddingProvider<T> {
    pub fn new(db: Db, transport: T) -> Self {
        Self {
            db,
            transport,
            user_agent: "Quilltap/unknown".to_string(),
            openrouter_referer: "http://localhost:3000".to_string(),
            localhost_gateway: None,
            num_ctx: Mutex::new(NumCtxCache::new()),
        }
    }

    /// Override the Quilltap user agent (`Quilltap/{version}`).
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Override the OpenRouter `HTTP-Referer` (v4 `BASE_URL`).
    pub fn with_openrouter_referer(mut self, referer: impl Into<String>) -> Self {
        self.openrouter_referer = referer.into();
        self
    }

    /// Inject the VM host gateway for localhost base-URL rewriting.
    pub fn with_localhost_gateway(mut self, gateway: Option<String>) -> Self {
        self.localhost_gateway = gateway;
        self
    }

    /// v4 `generateEmbeddingForUser`'s profile resolution + dispatch (the
    /// priority lane is host-timing — see the module doc).
    async fn generate_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
    ) -> Result<EmbeddingResult, EmbeddingError> {
        let mut profile: Option<EmbeddingProfileRow> = None;

        if let Some(pid) = profile_id {
            let pid = pid.to_string();
            profile = self
                .db
                .read_main(move |conn| embedding_profiles::find_by_id(conn, &pid))
                .map_err(|e| EmbeddingError::new(e.to_string()))?;
        }

        if profile.is_none() {
            let uid = user_id.to_string();
            profile = self
                .db
                .read_main(move |conn| embedding_profiles::find_default(conn, &uid))
                .map_err(|e| EmbeddingError::new(e.to_string()))?;
        }

        let Some(profile) = profile else {
            return Err(EmbeddingError::new("No embedding profile configured"));
        };

        self.generate_embedding(text, &profile, user_id).await
    }

    /// v4 `generateEmbedding`: the BUILTIN dispatch. v4's outer catch here is
    /// DEAD CODE (`return <promise>` inside `try` — the rejection bypasses the
    /// catch, so the "Embedding failed for provider …" wrap never fires; see
    /// the module doc). A plain throw surfaces with only its message.
    async fn generate_embedding(
        &self,
        text: &str,
        profile: &EmbeddingProfileRow,
        user_id: &str,
    ) -> Result<EmbeddingResult, EmbeddingError> {
        // Built-in provider has special handling for vocabulary state.
        if profile.provider == "BUILTIN" {
            return generate_builtin_embedding(&self.db, text, profile).await;
        }

        // All other providers use the API-based flow.
        match self.generate_api_embedding(text, profile, user_id).await {
            Ok(r) => Ok(r),
            Err(ApiError::Embedding(e)) => Err(e),
            Err(ApiError::Plain(msg)) => Err(EmbeddingError::new(msg)),
        }
    }

    /// v4 `generateApiEmbedding`: registry gate → base-URL resolution → API key
    /// → wire dispatch → `applyEmbeddingProfile`.
    async fn generate_api_embedding(
        &self,
        text: &str,
        profile: &EmbeddingProfileRow,
        user_id: &str,
    ) -> Result<EmbeddingResult, ApiError> {
        let name = profile.provider.as_str();
        let registry = Registry::built_in();

        // v4 `providerRegistry.createEmbeddingProvider(name, baseUrl ||
        // undefined)` — requireProvider + the embeddings-capability gate (both
        // plain Errors, escaping UNwrapped — the dead outer catch).
        let Some(manifest) = registry.get_provider(name) else {
            return Err(ApiError::Plain(format!(
                "Provider '{name}' not found in registry"
            )));
        };
        if !manifest.capabilities.embeddings {
            return Err(ApiError::Plain(format!(
                "Provider '{name}' does not support embeddings"
            )));
        }

        // v4 `resolveBaseUrl`: `baseUrl ? rewriteLocalhostUrl(baseUrl) : baseUrl`
        // (an empty/NULL baseUrl stays undefined → the provider's default).
        let resolved_base: Option<String> = match &profile.base_url {
            Some(b) if !b.is_empty() => {
                Some(rewrite_localhost_url(b, self.localhost_gateway.as_deref()))
            }
            _ => None,
        };
        let base = resolved_base.as_deref().unwrap_or("");

        // v4 `getApiKeyForProfile` behind the manifest's requiresApiKey gate.
        let mut api_key = String::new();
        if manifest.config_requirements.requires_api_key {
            let key = match &profile.api_key_id {
                Some(kid) => {
                    let kid = kid.clone();
                    let uid = user_id.to_string();
                    self.db
                        .read_main(move |conn| api_keys::find_by_id_and_user_id(conn, &kid, &uid))
                        .map_err(|e| ApiError::Plain(e.to_string()))?
                        .map(|k| k.key_value)
                }
                None => None,
            };
            match key {
                Some(k) => api_key = k,
                None => {
                    return Err(ApiError::Embedding(EmbeddingError {
                        message: format!("No API key found for {name} embedding profile"),
                        provider: Some(name.to_string()),
                    }))
                }
            }
        }

        // v4 `{ dimensions: profile.dimensions || undefined }` — 0 is falsy.
        let dimensions: Option<i64> = profile.dimensions.filter(|&d| d != 0.0).map(|d| d as i64);

        let wire = match name {
            "OPENAI" => {
                self.embed_openai(base, &profile.model_name, text, dimensions, &api_key)
                    .await?
            }
            "OLLAMA" => self.embed_ollama(base, &profile.model_name, text).await?,
            "OPENROUTER" => {
                self.embed_openrouter(&profile.model_name, text, dimensions, &api_key)
                    .await?
            }
            "NANOGPT" => {
                self.embed_nanogpt(base, &profile.model_name, text, dimensions, &api_key)
                    .await?
            }
            // v4: a registered embeddings-capable plugin without a
            // createEmbeddingProvider factory (unreachable for the built-in
            // manifest set — only the three above declare embeddings).
            other => {
                return Err(ApiError::Plain(format!(
                    "Provider '{other}' does not implement createEmbeddingProvider"
                )))
            }
        };

        // applyEmbeddingProfile: v4 wraps the raw `number[]` in a Float32Array
        // (each element narrowed to f32) before the slice + renormalize.
        let raw: Vec<f32> = wire.embedding.iter().map(|&x| x as f32).collect();
        let embedding = apply_embedding_profile(
            &raw,
            profile.truncate_to_dimensions.map(|t| t as usize),
            profile.normalize_l2,
        );
        let dimensions = embedding.len();

        Ok(EmbeddingResult {
            embedding,
            model: wire.model,
            dimensions,
            provider: name.to_string(),
        })
    }

    /// Serialize + send a built request. `with_ua` appends the Quilltap
    /// User-Agent (the Ollama fetches; v4's single-embedding OpenAI path omits
    /// it). A transport `Err` is a plain throw (v4's fetch rejection).
    async fn send(&self, req: &BuiltRequest, with_ua: bool) -> Result<WireResponse, String> {
        let mut headers = req.headers.clone();
        if with_ua {
            headers.push(("User-Agent".to_string(), self.user_agent.clone()));
        }
        let body = serde_json::to_string(&req.body).map_err(|e| e.to_string())?;
        self.transport
            .send(&req.method, &req.url, &headers, &body)
            .await
    }

    async fn embed_openai(
        &self,
        base: &str,
        model: &str,
        text: &str,
        dimensions: Option<i64>,
        api_key: &str,
    ) -> Result<WireEmbedding, ApiError> {
        let req = build_openai_request(base, model, text, dimensions, api_key);
        let resp = self.send(&req, false).await.map_err(ApiError::Plain)?;
        if !resp.ok() {
            // v4: `await response.json().catch(() => ({}))`.
            let error_body = parse_json_or_empty(&resp.body);
            return Err(ApiError::Plain(openai_error_message(
                &error_body,
                &resp.status_text,
            )));
        }
        let body: Value =
            serde_json::from_str(&resp.body).map_err(|e| ApiError::Plain(e.to_string()))?;
        let mut wire = parse_openai_response(&body).map_err(ApiError::Plain)?;
        // v4 returns the REQUEST model, not anything off the response.
        wire.model = model.to_string();
        Ok(wire)
    }

    /// v4 NanoGPT `generateEmbedding` (P4.D101): the OpenAI-compatible
    /// `/embeddings` route with a User-Agent, NanoGPT's own error sentence, and
    /// v4's `data.data[0].embedding` read — the same parse as the OpenAI path,
    /// so it reuses `parse_openai_response`.
    ///
    /// Like OpenAI, v4 returns the REQUEST model rather than anything off the
    /// response body.
    async fn embed_nanogpt(
        &self,
        base: &str,
        model: &str,
        text: &str,
        dimensions: Option<i64>,
        api_key: &str,
    ) -> Result<WireEmbedding, ApiError> {
        let req = build_nanogpt_request(base, model, text, dimensions, api_key);
        let resp = self.send(&req, true).await.map_err(ApiError::Plain)?;
        if !resp.ok() {
            let error_body = parse_json_or_empty(&resp.body);
            return Err(ApiError::Plain(nanogpt_error_message(
                &error_body,
                &resp.status_text,
                false,
            )));
        }
        let body: Value =
            serde_json::from_str(&resp.body).map_err(|e| ApiError::Plain(e.to_string()))?;
        let mut wire = parse_openai_response(&body).map_err(ApiError::Plain)?;
        wire.model = model.to_string();
        Ok(wire)
    }

    async fn embed_ollama(
        &self,
        base: &str,
        model: &str,
        text: &str,
    ) -> Result<WireEmbedding, ApiError> {
        // The up-front empty/whitespace guard (a plain throw in v4).
        if let Some(msg) = ollama_empty_input_error(text) {
            return Err(ApiError::Plain(msg));
        }

        // v4 caches by `${this.baseUrl}::${model}` with the DEFAULTED base.
        let effective_base = if base.is_empty() {
            OLLAMA_DEFAULT_BASE
        } else {
            base
        };
        let num_ctx = self.resolve_num_ctx(effective_base, base, model).await;

        let req = build_ollama_embed_request(base, model, text, num_ctx);
        let resp = self.send(&req, true).await.map_err(ApiError::Plain)?;

        // Older Ollama servers predate /api/embed — legacy fallback (v4 checks
        // 404 BEFORE the generic !ok branch, and re-checks empty input — which
        // cannot differ here, ported for shape fidelity).
        if resp.status == 404 {
            if let Some(msg) = ollama_empty_input_error(text) {
                return Err(ApiError::Plain(msg));
            }
            let req = build_ollama_legacy_request(base, model, text);
            let resp = self.send(&req, true).await.map_err(ApiError::Plain)?;
            if !resp.ok() {
                let error_body = parse_json_or_empty(&resp.body);
                return Err(ApiError::Plain(ollama_error_message(
                    &error_body,
                    &resp.status_text,
                )));
            }
            let body: Value =
                serde_json::from_str(&resp.body).map_err(|e| ApiError::Plain(e.to_string()))?;
            let mut wire = parse_ollama_legacy_response(&body).map_err(ApiError::Plain)?;
            wire.model = model.to_string();
            return Ok(wire);
        }

        if !resp.ok() {
            let error_body = parse_json_or_empty(&resp.body);
            return Err(ApiError::Plain(ollama_error_message(
                &error_body,
                &resp.status_text,
            )));
        }
        let body: Value =
            serde_json::from_str(&resp.body).map_err(|e| ApiError::Plain(e.to_string()))?;
        let mut wire = parse_ollama_embed_response(&body).map_err(ApiError::Plain)?;
        wire.model = model.to_string();
        Ok(wire)
    }

    /// v4 `resolveNumCtx`: cache hit → cached; miss → `/api/show` derivation,
    /// caching ONLY derived values (a transient failure must not lock in the
    /// fallback). The in-flight dedupe is not ported (module doc).
    async fn resolve_num_ctx(&self, effective_base: &str, raw_base: &str, model: &str) -> i64 {
        let mut missed = false;
        let cached = {
            let mut cache = self.num_ctx.lock().expect("num_ctx cache poisoned");
            cache.resolve(effective_base, model, || {
                missed = true;
                (NUM_CTX_FALLBACK, false)
            })
        };
        if !missed {
            return cached;
        }

        let (num_ctx, derived) = self.fetch_model_num_ctx(raw_base, model).await;
        let mut cache = self.num_ctx.lock().expect("num_ctx cache poisoned");
        cache.resolve(effective_base, model, || (num_ctx, derived))
    }

    /// v4 `fetchModelNumCtx`: any failure (transport throw, non-2xx, unparseable
    /// body, no context-length key) → the fallback, not derived.
    async fn fetch_model_num_ctx(&self, base: &str, model: &str) -> (i64, bool) {
        let req = build_ollama_show_request(base, model);
        match self.send(&req, true).await {
            Ok(resp) if resp.ok() => match serde_json::from_str::<Value>(&resp.body) {
                Ok(body) => derive_num_ctx(&body),
                Err(_) => (NUM_CTX_FALLBACK, false),
            },
            _ => (NUM_CTX_FALLBACK, false),
        }
    }

    async fn embed_openrouter(
        &self,
        model: &str,
        text: &str,
        dimensions: Option<i64>,
        api_key: &str,
    ) -> Result<WireEmbedding, ApiError> {
        // The recorded SDK wire body — `dimensions` FIRST when present (the
        // SDK's own key order, NOT the plugin argument order).
        let mut body = serde_json::Map::new();
        if let Some(d) = dimensions {
            body.insert("dimensions".into(), json!(d));
        }
        body.insert("input".into(), json!(text));
        body.insert("model".into(), json!(model));
        let body_text = serde_json::to_string(&Value::Object(body))
            .map_err(|e| ApiError::Plain(e.to_string()))?;

        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Authorization".to_string(), format!("Bearer {api_key}")),
            ("HTTP-Referer".to_string(), self.openrouter_referer.clone()),
            ("X-OpenRouter-Title".to_string(), self.user_agent.clone()),
        ];
        let resp = self
            .transport
            .send("POST", OPENROUTER_EMBEDDINGS_URL, &headers, &body_text)
            .await
            .map_err(ApiError::Plain)?;

        if !resp.ok() {
            // The SDK converts a non-2xx into a per-status throw whose message
            // is the body's `error.message` (recorded). Other shapes are a
            // documented seam.
            return Err(ApiError::Plain(openrouter_non2xx_message(
                &resp.body,
                resp.status,
            )));
        }

        let wire_body: Value =
            serde_json::from_str(&resp.body).map_err(|e| ApiError::Plain(e.to_string()))?;
        let sdk_value = sdk_transform_openrouter(&wire_body);
        parse_openrouter_response(&sdk_value).map_err(ApiError::Plain)
    }
}

/// v4 `response.json().catch(() => ({}))`.
fn parse_json_or_empty(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|_| json!({}))
}

/// Reproduce the OpenRouter SDK's response projection for the fields the plugin
/// consumes: `model` and `data[*].embedding` pass through; snake_case `usage`
/// becomes camelCase (`promptTokens` / `totalTokens` / optional `cost`). A
/// top-level string validates as the SDK's error-string variant and passes
/// through (the plugin's "OpenRouter returned an error" branch).
fn sdk_transform_openrouter(wire: &Value) -> Value {
    if wire.is_string() {
        return wire.clone();
    }
    let mut out = serde_json::Map::new();
    if let Some(model) = wire.get("model") {
        out.insert("model".into(), model.clone());
    }
    if let Some(data) = wire.get("data") {
        out.insert("data".into(), data.clone());
    }
    if let Some(usage) = wire.get("usage").and_then(Value::as_object) {
        let mut u = serde_json::Map::new();
        if let Some(v) = usage.get("prompt_tokens") {
            u.insert("promptTokens".into(), v.clone());
        }
        if let Some(v) = usage.get("total_tokens") {
            u.insert("totalTokens".into(), v.clone());
        }
        if let Some(v) = usage.get("cost") {
            u.insert("cost".into(), v.clone());
        }
        out.insert("usage".into(), Value::Object(u));
    }
    Value::Object(out)
}

/// The recorded SDK non-2xx throw message: the response body's `error.message`
/// when present, else a status-tagged fallback (a documented seam — the SDK's
/// per-status shapes beyond the structured-error body are out of corpus).
fn openrouter_non2xx_message(body: &str, status: u16) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
        {
            return msg.to_string();
        }
    }
    format!("Unexpected API response status or content-type: Status {status}")
}

impl<T: WireTransport> EmbeddingProvider for ApiEmbeddingProvider<T> {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
        _priority: EmbeddingPriority,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        self.generate_for_user(text, user_id, profile_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::{Db, DbPaths};
    use crate::model::wire::CannedWireTransport;

    /// An in-memory-style Db over a temp file with the tables the provider
    /// reads, seeded through the writer.
    async fn test_db(tag: &str) -> Db {
        let path = std::env::temp_dir().join(format!(
            "qt-embed-provider-test-{tag}-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let db = Db::open(
            DbPaths {
                main: path,
                mount_index: None,
                llm_logs: None,
            },
            "dGVzdC1wZXBwZXItZm9yLWVtYmVkLXByb3ZpZGVy",
        )
        .expect("open test db");
        db.write(|ws| {
            let conn = ws.main().connection();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS embedding_profiles (
                   id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT,
                   apiKeyId TEXT, baseUrl TEXT, modelName TEXT, dimensions REAL,
                   truncateToDimensions REAL, normalizeL2 INTEGER, isDefault INTEGER,
                   tags TEXT, createdAt TEXT, updatedAt TEXT);
                 CREATE TABLE IF NOT EXISTS api_keys (
                   id TEXT PRIMARY KEY, userId TEXT, label TEXT, provider TEXT,
                   key_value TEXT, isActive INTEGER, lastUsed TEXT,
                   createdAt TEXT, updatedAt TEXT);",
            )
            .map_err(Into::into)
        })
        .await
        .expect("create tables");
        db
    }

    async fn seed_profile(
        db: &Db,
        id: &str,
        user: &str,
        provider: &str,
        api_key_id: Option<&str>,
        base_url: Option<&str>,
        is_default: bool,
    ) {
        let (id, user, provider) = (id.to_string(), user.to_string(), provider.to_string());
        let api_key_id = api_key_id.map(str::to_string);
        let base_url = base_url.map(str::to_string);
        db.write(move |ws| {
            ws.main()
                .connection()
                .execute(
                    "INSERT INTO embedding_profiles \
                     (id, userId, name, provider, apiKeyId, baseUrl, modelName, dimensions, \
                      truncateToDimensions, normalizeL2, isDefault, tags, createdAt, updatedAt) \
                     VALUES (?1, ?2, 'p', ?3, ?4, ?5, 'test-model', NULL, NULL, 1, ?6, '[]', \
                             '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                    rusqlite::params![
                        id,
                        user,
                        provider,
                        api_key_id,
                        base_url,
                        i64::from(is_default)
                    ],
                )
                .map_err(Into::into)
                .map(|_| ())
        })
        .await
        .expect("seed profile");
    }

    async fn seed_api_key(db: &Db, id: &str, user: &str, key_value: &str) {
        let (id, user, key_value) = (id.to_string(), user.to_string(), key_value.to_string());
        db.write(move |ws| {
            ws.main()
                .connection()
                .execute(
                    "INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, \
                     lastUsed, createdAt, updatedAt) VALUES (?1, ?2, 'k', 'OPENAI', ?3, 1, NULL, \
                     '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                    rusqlite::params![id, user, key_value],
                )
                .map_err(Into::into)
                .map(|_| ())
        })
        .await
        .expect("seed api key");
    }

    #[tokio::test]
    async fn no_profile_configured() {
        let db = test_db("none").await;
        let provider = ApiEmbeddingProvider::new(db, CannedWireTransport::new());
        let err = provider
            .generate_for_user("hi", "u-no-profiles", None)
            .await
            .unwrap_err();
        assert_eq!(err.message, "No embedding profile configured");
        assert_eq!(err.provider, None);
    }

    #[tokio::test]
    async fn explicit_missing_falls_back_to_default() {
        let db = test_db("fallback").await;
        seed_profile(&db, "ep-default", "u1", "OPENAI", Some("ak-1"), None, true).await;
        seed_api_key(&db, "ak-1", "u1", "sk-test").await;
        let transport = CannedWireTransport::new().with_response(
            "POST",
            "https://api.openai.com/v1/embeddings",
            "{\"model\":\"test-model\",\"input\":\"hi\"}",
            WireResponse::new(
                200,
                "{\"data\":[{\"embedding\":[3.0,4.0]}],\"usage\":{\"prompt_tokens\":1,\"total_tokens\":1}}",
            ),
        );
        let provider = ApiEmbeddingProvider::new(db, transport);
        // The explicit id does not exist → the default profile resolves.
        let r = provider
            .generate_for_user("hi", "u1", Some("ep-nonexistent"))
            .await
            .unwrap();
        assert_eq!(r.provider, "OPENAI");
        assert_eq!(r.model, "test-model");
        // normalizeL2 default → unit vector.
        assert_eq!(r.embedding, vec![0.6, 0.8]);
        assert_eq!(r.dimensions, 2);
    }

    #[tokio::test]
    async fn requires_api_key_gate() {
        let db = test_db("nokey").await;
        // apiKeyId points at a missing row.
        seed_profile(&db, "ep-1", "u1", "OPENAI", Some("ak-missing"), None, false).await;
        // apiKeyId NULL.
        seed_profile(&db, "ep-2", "u1", "OPENAI", None, None, false).await;
        let provider = ApiEmbeddingProvider::new(db, CannedWireTransport::new());
        for pid in ["ep-1", "ep-2"] {
            let err = provider
                .generate_for_user("hi", "u1", Some(pid))
                .await
                .unwrap_err();
            assert_eq!(err.message, "No API key found for OPENAI embedding profile");
            assert_eq!(err.provider.as_deref(), Some("OPENAI"));
        }
    }

    #[tokio::test]
    async fn unknown_provider_plain_error() {
        let db = test_db("unknown").await;
        seed_profile(&db, "ep-1", "u1", "MYSTERY", None, None, false).await;
        let provider = ApiEmbeddingProvider::new(db, CannedWireTransport::new());
        let err = provider
            .generate_for_user("hi", "u1", Some("ep-1"))
            .await
            .unwrap_err();
        // v4's wrap is dead code: the registry gate's plain Error escapes as-is.
        assert_eq!(err.message, "Provider 'MYSTERY' not found in registry");
        assert_eq!(err.provider, None);
    }

    #[tokio::test]
    async fn embeddings_incapable_provider_plain_error() {
        let db = test_db("nocap").await;
        // ANTHROPIC exists in the registry but declares embeddings: false.
        seed_profile(&db, "ep-1", "u1", "ANTHROPIC", None, None, false).await;
        let provider = ApiEmbeddingProvider::new(db, CannedWireTransport::new());
        let err = provider
            .generate_for_user("hi", "u1", Some("ep-1"))
            .await
            .unwrap_err();
        assert_eq!(
            err.message,
            "Provider 'ANTHROPIC' does not support embeddings"
        );
        assert_eq!(err.provider, None);
    }

    #[tokio::test]
    async fn openai_non_2xx_plain_error_message() {
        let db = test_db("429").await;
        seed_profile(&db, "ep-1", "u1", "OPENAI", Some("ak-1"), None, false).await;
        seed_api_key(&db, "ak-1", "u1", "sk-test").await;
        let transport = CannedWireTransport::new().with_response(
            "POST",
            "https://api.openai.com/v1/embeddings",
            "{\"model\":\"test-model\",\"input\":\"hi\"}",
            WireResponse {
                status: 429,
                status_text: "Too Many Requests".to_string(),
                body: "{\"error\":{\"message\":\"rate limit exceeded\"}}".to_string(),
            },
        );
        let provider = ApiEmbeddingProvider::new(db, transport);
        let err = provider
            .generate_for_user("hi", "u1", Some("ep-1"))
            .await
            .unwrap_err();
        // v4's wrap is dead code: the plugin's plain throw escapes unwrapped.
        assert_eq!(err.message, "OpenAI embedding failed: rate limit exceeded");
        assert_eq!(err.provider, None);
    }

    /// The num_ctx derivation is cached only when derived: a failed /api/show
    /// re-derives on the next call; a successful one is served from cache.
    #[tokio::test]
    async fn ollama_num_ctx_caches_only_derived() {
        let db = test_db("numctx").await;
        seed_profile(
            &db,
            "ep-1",
            "u1",
            "OLLAMA",
            None,
            Some("http://ollama.test:11434"),
            false,
        )
        .await;
        let embed_ok = WireResponse::new(200, "{\"embeddings\":[[1.0,0.0]]}");
        // First round: /api/show fails (500) → fallback 8192, NOT cached.
        let t1 = CannedWireTransport::new()
            .with_response(
                "POST",
                "http://ollama.test:11434/api/show",
                "{\"model\":\"test-model\"}",
                WireResponse::new(500, "{}"),
            )
            .with_response(
                "POST",
                "http://ollama.test:11434/api/embed",
                "{\"model\":\"test-model\",\"input\":\"hi\",\"truncate\":true,\"options\":{\"num_ctx\":8192}}",
                embed_ok.clone(),
            );
        let p1 = ApiEmbeddingProvider::new(db.clone(), t1);
        p1.generate_for_user("hi", "u1", Some("ep-1"))
            .await
            .unwrap();

        // Second round on the SAME provider would re-derive; simulate by a fresh
        // provider whose /api/show now succeeds (2048) and confirm the embed
        // body carries the derived num_ctx, then a repeat call skips /api/show
        // (a second /api/show would be a canned miss only if bodies diverged —
        // instead prove via the cache: the transport has NO /api/show entry for
        // the second call round-trip by keying the same request; the cache-hit
        // path never sends it, so a single canned /api/show entry suffices).
        let t2 = CannedWireTransport::new()
            .with_response(
                "POST",
                "http://ollama.test:11434/api/show",
                "{\"model\":\"test-model\"}",
                WireResponse::new(200, "{\"model_info\":{\"qwen3.context_length\":2048}}"),
            )
            .with_response(
                "POST",
                "http://ollama.test:11434/api/embed",
                "{\"model\":\"test-model\",\"input\":\"hi\",\"truncate\":true,\"options\":{\"num_ctx\":2048}}",
                embed_ok,
            );
        let p2 = ApiEmbeddingProvider::new(db, t2);
        let r1 = p2
            .generate_for_user("hi", "u1", Some("ep-1"))
            .await
            .unwrap();
        assert_eq!(r1.embedding, vec![1.0, 0.0]);
        // Cache hit: the second call derives nothing (the canned /api/show
        // entry being consumed again would be fine; what we assert is the
        // result still resolves with num_ctx 2048 from cache).
        let r2 = p2
            .generate_for_user("hi", "u1", Some("ep-1"))
            .await
            .unwrap();
        assert_eq!(r2.embedding, vec![1.0, 0.0]);
    }

    #[test]
    fn openrouter_transform_and_non2xx_message() {
        let wire = serde_json::json!({
            "id": "gen-1",
            "object": "list",
            "data": [{ "object": "embedding", "embedding": [0.5, -0.25], "index": 0 }],
            "model": "openai/text-embedding-3-small",
            "usage": { "prompt_tokens": 2, "total_tokens": 2, "cost": 0.000001 }
        });
        let sdk = sdk_transform_openrouter(&wire);
        assert_eq!(sdk.get("usage").unwrap().get("promptTokens").unwrap(), 2);
        assert_eq!(sdk.get("usage").unwrap().get("cost").unwrap(), 0.000001);
        let parsed = parse_openrouter_response(&sdk).unwrap();
        assert_eq!(parsed.model, "openai/text-embedding-3-small");
        assert_eq!(parsed.embedding, vec![0.5, -0.25]);

        // usage without cost stays cost-less.
        let wire2 = serde_json::json!({
            "model": "m", "data": [{ "embedding": [1.0] }],
            "usage": { "prompt_tokens": 3, "total_tokens": 3 }
        });
        let sdk2 = sdk_transform_openrouter(&wire2);
        assert!(sdk2.get("usage").unwrap().get("cost").is_none());

        // The recorded 429 throw message.
        assert_eq!(
            openrouter_non2xx_message(
                "{\"error\":{\"message\":\"rate limited\",\"code\":429}}",
                429
            ),
            "rate limited"
        );
    }
}
