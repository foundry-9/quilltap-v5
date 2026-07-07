//! The embedding-provider WIRE — v4's plugin embedding providers
//! (`plugins/dist/qtap-plugin-{openai,ollama,openrouter}/embedding-provider.ts`).
//! Sans-IO, the W4.7c request-builder precedent: each provider's request is a
//! [`BuiltRequest`] VALUE (method / url / headers / body — no HTTP) and each
//! response parse is a pure function over the raw body. Live transport composes
//! this in W4.7d.
//!
//! Providers differ in shape:
//!   - **OpenAI** — `POST {base}/embeddings` `{model, input, dimensions?}`; parse
//!     `data.data[0].embedding` + snake_case usage; error → `error.error?.message
//!     || statusText`.
//!   - **Ollama** (no API key) — the up-front empty/whitespace throw; the modern
//!     `POST /api/embed` `{model, input, truncate:true, options:{num_ctx}}` with
//!     a 404 fallback to the legacy `POST /api/embeddings` `{model, prompt}`; the
//!     `num_ctx` derived from `/api/show`; the finite-vector guard.
//!   - **OpenRouter** — the SDK `embeddings.generate` request body; the response
//!     `embedding` is a float array OR a **base64-encoded Float32** blob (decoded
//!     little-endian via the ported [`crate::embedding_blob`] reads); usage incl.
//!     `cost`.
//!
//! The vectors are kept as `f64` (v4's `number[]`) — the `f32` narrowing happens
//! only at the storage boundary (`embedding_blob`), and `applyEmbeddingProfile`
//! (truncate + L2) already lives in [`crate::embedding_vector`].
//!
//! Host-timing / host-side, ported as nothing (documented seams): the 3 s / socket
//! timeouts, the interactive-priority throttle (`interactivePending` + 50 ms poll),
//! and API-key acquisition. The `/api/show`-derived `num_ctx` cache is per-instance
//! ([`NumCtxCache`], v4's module-global Map — cache only `derived: true`).

use base64::Engine;
use serde_json::{json, Value};

use crate::embedding_blob::blob_to_float32;
use crate::jsstr::js_trim;
use crate::model::request_builder::BuiltRequest;

/// v4 `NUM_CTX_CEILING`.
pub const NUM_CTX_CEILING: i64 = 16384;
/// v4 `NUM_CTX_FALLBACK`.
pub const NUM_CTX_FALLBACK: i64 = 8192;

/// A parsed embedding (the provider result before `applyEmbeddingProfile`).
/// Vectors are `f64` to match v4's `number[]` exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct WireEmbedding {
    pub embedding: Vec<f64>,
    pub model: String,
    pub dimensions: usize,
    pub usage: Option<WireUsage>,
}

/// Token/cost usage (superset of the three providers; fields omitted-when-`None`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WireUsage {
    pub prompt_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
    pub cost: Option<f64>,
}

// ============================================================================
// OpenAI
// ============================================================================

/// v4 OpenAI `generateEmbedding` request: `POST {base}/embeddings`, JSON
/// `{model, input}` (+ `dimensions` only when set). `base_url` defaults to
/// `https://api.openai.com/v1` when empty.
pub fn build_openai_request(
    base_url: &str,
    model: &str,
    input: &str,
    dimensions: Option<i64>,
    api_key: &str,
) -> BuiltRequest {
    let base = if base_url.is_empty() {
        "https://api.openai.com/v1"
    } else {
        base_url
    };
    let mut body = serde_json::Map::new();
    body.insert("model".into(), json!(model));
    body.insert("input".into(), json!(input));
    if let Some(d) = dimensions {
        body.insert("dimensions".into(), json!(d));
    }
    BuiltRequest {
        method: "POST".into(),
        url: format!("{base}/embeddings"),
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Authorization".into(), format!("Bearer {api_key}")),
        ],
        body: Value::Object(body),
    }
}

/// v4 OpenAI parse: `data.data[0].embedding`; snake_case usage.
pub fn parse_openai_response(body: &Value) -> Result<WireEmbedding, String> {
    let embedding = number_array(
        body.get("data")
            .and_then(|d| d.get(0))
            .and_then(|d| d.get("embedding")),
    )
    .ok_or_else(|| "Cannot read property 'embedding' of undefined".to_string())?;
    let dimensions = embedding.len();
    Ok(WireEmbedding {
        embedding,
        model: String::new(), // v4 uses the request `model` (threaded by the caller)
        dimensions,
        usage: parse_usage_snake(body.get("usage")),
    })
}

/// v4 OpenAI error message: `error.error?.message || statusText` (the error body
/// JSON is passed in; `status_text` is the HTTP status text).
pub fn openai_error_message(error_body: &Value, status_text: &str) -> String {
    let msg = error_body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str);
    format!(
        "OpenAI embedding failed: {}",
        match msg {
            Some(m) if !m.is_empty() => m,
            _ => status_text,
        }
    )
}

// ============================================================================
// Ollama
// ============================================================================

/// v4 Ollama's up-front guard: empty / whitespace-only input rejects with a
/// deterministic error before any request is built.
pub fn ollama_empty_input_error(input: &str) -> Option<String> {
    if input.is_empty() || js_trim(input).is_empty() {
        Some("Cannot embed empty input".to_string())
    } else {
        None
    }
}

/// v4 modern `POST /api/embed` request. `base_url` defaults to
/// `http://localhost:11434`.
pub fn build_ollama_embed_request(
    base_url: &str,
    model: &str,
    input: &str,
    num_ctx: i64,
) -> BuiltRequest {
    let base = ollama_base(base_url);
    BuiltRequest {
        method: "POST".into(),
        url: format!("{base}/api/embed"),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: json!({
            "model": model,
            "input": input,
            "truncate": true,
            "options": { "num_ctx": num_ctx },
        }),
    }
}

/// v4 legacy `POST /api/embeddings` request (the 404 fallback).
pub fn build_ollama_legacy_request(base_url: &str, model: &str, input: &str) -> BuiltRequest {
    let base = ollama_base(base_url);
    BuiltRequest {
        method: "POST".into(),
        url: format!("{base}/api/embeddings"),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: json!({ "model": model, "prompt": input }),
    }
}

/// v4 `/api/show` request (for num_ctx derivation).
pub fn build_ollama_show_request(base_url: &str, model: &str) -> BuiltRequest {
    let base = ollama_base(base_url);
    BuiltRequest {
        method: "POST".into(),
        url: format!("{base}/api/show"),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: json!({ "model": model }),
    }
}

fn ollama_base(base_url: &str) -> &str {
    if base_url.is_empty() {
        "http://localhost:11434"
    } else {
        base_url
    }
}

/// v4 modern parse: `data.embeddings[0]` then the finite guard.
pub fn parse_ollama_embed_response(body: &Value) -> Result<WireEmbedding, String> {
    let embedding = body
        .get("embeddings")
        .and_then(Value::as_array)
        .and_then(|a| a.first());
    finite_embedding(embedding).map(|embedding| {
        let dimensions = embedding.len();
        WireEmbedding {
            embedding,
            model: String::new(),
            dimensions,
            usage: None,
        }
    })
}

/// v4 legacy parse: `data.embedding` then the finite guard.
pub fn parse_ollama_legacy_response(body: &Value) -> Result<WireEmbedding, String> {
    finite_embedding(body.get("embedding")).map(|embedding| {
        let dimensions = embedding.len();
        WireEmbedding {
            embedding,
            model: String::new(),
            dimensions,
            usage: None,
        }
    })
}

/// v4 Ollama error message: `error.error || statusText`.
pub fn ollama_error_message(error_body: &Value, status_text: &str) -> String {
    let msg = error_body.get("error").and_then(Value::as_str);
    format!(
        "Ollama embedding failed: {}",
        match msg {
            Some(m) if !m.is_empty() => m,
            _ => status_text,
        }
    )
}

/// v4 `assertFiniteEmbedding`: not an array or empty → "No embedding returned
/// from Ollama"; a non-number / non-finite element → "Ollama returned a
/// non-finite (NaN/Inf) embedding".
pub fn finite_embedding(embedding: Option<&Value>) -> Result<Vec<f64>, String> {
    let arr = match embedding.and_then(Value::as_array) {
        Some(a) if !a.is_empty() => a,
        _ => return Err("No embedding returned from Ollama".to_string()),
    };
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        match v.as_f64() {
            Some(n) if n.is_finite() => out.push(n),
            // A non-number OR NaN/Inf. (serde_json can't hold NaN/Inf, so a
            // non-finite arrives as a non-number token, still caught here.)
            _ => return Err("Ollama returned a non-finite (NaN/Inf) embedding".to_string()),
        }
    }
    Ok(out)
}

/// v4 `fetchModelNumCtx`'s pure core: derive `num_ctx` from an `/api/show`
/// response body. Scans `model_info` for a key ending `.context_length` (or the
/// exact `context_length`) whose value is a positive number; `min(ctx, CEILING)`
/// with `derived: true`. No such key → `(FALLBACK, false)`.
pub fn derive_num_ctx(show_body: &Value) -> (i64, bool) {
    let model_info = match show_body.get("model_info").and_then(Value::as_object) {
        Some(m) => m,
        None => return (NUM_CTX_FALLBACK, false),
    };
    for (k, v) in model_info {
        if (k.ends_with(".context_length") || k == "context_length")
            && v.as_f64().is_some_and(|n| n > 0.0)
        {
            // v4 requires `typeof v === 'number' && v > 0`; the value is stored
            // as an integer context length.
            let ctx = v
                .as_i64()
                .unwrap_or_else(|| v.as_f64().unwrap_or(0.0) as i64);
            return (ctx.min(NUM_CTX_CEILING), true);
        }
    }
    (NUM_CTX_FALLBACK, false)
}

/// v4's `numCtxCache` (module-global, per-instance here): resolve `num_ctx` for a
/// `{base_url}::{model}` key, deriving from an `/api/show` body via `derive` on a
/// miss and caching ONLY derived values (a transient `/api/show` failure must not
/// lock in the fallback).
#[derive(Default)]
pub struct NumCtxCache {
    cache: std::collections::HashMap<String, i64>,
}

impl NumCtxCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up (or derive-and-store) the `num_ctx`. `derive` returns
    /// `(num_ctx, derived)`; only `derived == true` results are cached.
    pub fn resolve(
        &mut self,
        base_url: &str,
        model: &str,
        derive: impl FnOnce() -> (i64, bool),
    ) -> i64 {
        let key = format!("{base_url}::{model}");
        if let Some(&v) = self.cache.get(&key) {
            return v;
        }
        let (num_ctx, derived) = derive();
        if derived {
            self.cache.insert(key, num_ctx);
        }
        num_ctx
    }
}

// ============================================================================
// OpenRouter
// ============================================================================

/// v4 OpenRouter SDK request body (`embeddings.generate({requestBody})`): the
/// `{input, model, dimensions}` object. `dimensions` is ALWAYS present as a key
/// (v4 sets `dimensions: options?.dimensions`, so it is `undefined` → the SDK
/// drops it, or a number). We include it only when `Some` (matching the SDK's
/// drop-undefined wire behavior).
pub fn build_openrouter_request_body(model: &str, input: &str, dimensions: Option<i64>) -> Value {
    let mut body = serde_json::Map::new();
    body.insert("input".into(), json!(input));
    body.insert("model".into(), json!(model));
    if let Some(d) = dimensions {
        body.insert("dimensions".into(), json!(d));
    }
    Value::Object(body)
}

/// v4 OpenRouter parse: a string response is an error
/// (`OpenRouter returned an error: {response}`); else `data[0].embedding`
/// missing → "No embedding returned from OpenRouter"; a base64 STRING embedding
/// decodes as little-endian Float32; an array passes through. Usage carries
/// `cost`.
pub fn parse_openrouter_response(response: &Value) -> Result<WireEmbedding, String> {
    if let Some(s) = response.as_str() {
        return Err(format!("OpenRouter returned an error: {s}"));
    }

    let embedding_data = response
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|d| d.get("embedding"));

    let embedding = match embedding_data {
        Some(Value::String(b64)) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| format!("OpenRouter base64 decode failed: {e}"))?;
            // v4: new Float32Array(buffer) -> Array.from -> number[] (f32 widened).
            blob_to_float32(&bytes)
                .into_iter()
                .map(|f| f as f64)
                .collect::<Vec<f64>>()
        }
        Some(Value::Array(_)) => number_array(embedding_data)
            .ok_or_else(|| "No embedding returned from OpenRouter".to_string())?,
        // Missing / null / non-string-non-array -> falsy -> the throw.
        _ => return Err("No embedding returned from OpenRouter".to_string()),
    };

    let dimensions = embedding.len();
    Ok(WireEmbedding {
        embedding,
        model: response
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_default(),
        dimensions,
        usage: parse_usage_camel(response.get("usage")),
    })
}

// ============================================================================
// Shared helpers
// ============================================================================

fn number_array(v: Option<&Value>) -> Option<Vec<f64>> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
}

/// snake_case usage (`prompt_tokens` / `total_tokens`); `None` when absent.
fn parse_usage_snake(usage: Option<&Value>) -> Option<WireUsage> {
    let u = usage?;
    Some(WireUsage {
        prompt_tokens: u.get("prompt_tokens").and_then(Value::as_f64),
        total_tokens: u.get("total_tokens").and_then(Value::as_f64),
        cost: None,
    })
}

/// camelCase usage (`promptTokens` / `totalTokens` / `cost`).
fn parse_usage_camel(usage: Option<&Value>) -> Option<WireUsage> {
    let u = usage?;
    Some(WireUsage {
        prompt_tokens: u.get("promptTokens").and_then(Value::as_f64),
        total_tokens: u.get("totalTokens").and_then(Value::as_f64),
        cost: u.get("cost").and_then(Value::as_f64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_request_dimensions_optional() {
        let no_dim = build_openai_request("", "text-embedding-3-small", "hi", None, "sk-x");
        assert_eq!(no_dim.url, "https://api.openai.com/v1/embeddings");
        assert!(no_dim.body.get("dimensions").is_none());
        let with_dim = build_openai_request("https://x/v1", "m", "hi", Some(256), "sk-x");
        assert_eq!(with_dim.url, "https://x/v1/embeddings");
        assert_eq!(with_dim.body.get("dimensions"), Some(&json!(256)));
    }

    #[test]
    fn ollama_empty_input_rejects() {
        assert_eq!(
            ollama_empty_input_error("").as_deref(),
            Some("Cannot embed empty input")
        );
        assert_eq!(
            ollama_empty_input_error("   ").as_deref(),
            Some("Cannot embed empty input")
        );
        assert!(ollama_empty_input_error("hi").is_none());
    }

    #[test]
    fn num_ctx_matrix() {
        // Found, under ceiling.
        assert_eq!(
            derive_num_ctx(&json!({ "model_info": { "qwen3.context_length": 4096 } })),
            (4096, true)
        );
        // Found, over ceiling -> capped.
        assert_eq!(
            derive_num_ctx(&json!({ "model_info": { "llama.context_length": 131072 } })),
            (16384, true)
        );
        // Exact key.
        assert_eq!(
            derive_num_ctx(&json!({ "model_info": { "context_length": 2048 } })),
            (2048, true)
        );
        // Missing -> fallback, not derived.
        assert_eq!(derive_num_ctx(&json!({ "model_info": {} })), (8192, false));
        assert_eq!(derive_num_ctx(&json!({})), (8192, false));
    }

    #[test]
    fn num_ctx_cache_only_derived() {
        let mut cache = NumCtxCache::new();
        // First: a non-derived fallback is NOT cached.
        assert_eq!(cache.resolve("b", "m", || (8192, false)), 8192);
        // Second: derives again (not cached), now a derived value IS cached.
        assert_eq!(cache.resolve("b", "m", || (4096, true)), 4096);
        // Third: the cache-hit returns the derived value without deriving.
        assert_eq!(
            cache.resolve("b", "m", || panic!("should not derive")),
            4096
        );
    }

    #[test]
    fn finite_guard() {
        assert_eq!(
            finite_embedding(Some(&json!([1.0, 2.0, 3.0]))).unwrap(),
            vec![1.0, 2.0, 3.0]
        );
        assert_eq!(
            finite_embedding(Some(&json!([]))).unwrap_err(),
            "No embedding returned from Ollama"
        );
        assert_eq!(
            finite_embedding(None).unwrap_err(),
            "No embedding returned from Ollama"
        );
        assert_eq!(
            finite_embedding(Some(&json!([1.0, "x", 3.0]))).unwrap_err(),
            "Ollama returned a non-finite (NaN/Inf) embedding"
        );
    }

    #[test]
    fn openrouter_base64_and_array() {
        // Array passthrough.
        let arr = json!({ "model": "m", "data": [{ "embedding": [0.5, -0.25] }] });
        let r = parse_openrouter_response(&arr).unwrap();
        assert_eq!(r.embedding, vec![0.5, -0.25]);

        // base64 of two LE f32s: 1.0 and 2.0.
        let bytes: Vec<u8> = [1.0f32, 2.0f32]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let enc = json!({ "model": "m", "data": [{ "embedding": b64 }] });
        let r = parse_openrouter_response(&enc).unwrap();
        assert_eq!(r.embedding, vec![1.0, 2.0]);

        // String top-level -> error.
        assert_eq!(
            parse_openrouter_response(&json!("rate limited")).unwrap_err(),
            "OpenRouter returned an error: rate limited"
        );
        // Missing embedding.
        assert_eq!(
            parse_openrouter_response(&json!({ "data": [{}] })).unwrap_err(),
            "No embedding returned from OpenRouter"
        );
    }
}
