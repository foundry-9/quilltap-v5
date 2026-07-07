//! The five image-generation wire dialects (W4.7f) — the sans-IO request builders
//! and response decoders for the OpenAI / Google (Imagen + Gemini) / Grok /
//! OpenRouter / Z-AI image providers (v4 `plugins/dist/qtap-plugin-*/
//! image-provider.ts`).
//!
//! Each provider gets [`build_image_request`] (the method / url / body the plugin
//! or its SDK sends) + [`parse_image_response`] (the success body → [`ImageGenResponse`]
//! plus every rejection path normalized to the **exact error string** v4 surfaces
//! — the strings the already-ported
//! [`is_image_moderation_error`](crate::services::dangerous_content::provider_routing::is_image_moderation_error)
//! keyword shim matches). The build/parse are verified independently of transport
//! (`image_dialects_equivalence`); the real [`ImageProvider`] impl
//! ([`RealImageProvider`]) composes them over the injected
//! [`WireTransport`](crate::model::wire::WireTransport) seam.
//!
//! ## Two transport styles (faithful to v4)
//!
//!   - **SDK providers** (OPENAI / GROK / Z_AI) build via the OpenAI SDK, which
//!     POSTs `JSON.stringify(requestParams)` to `{baseURL}/images/generations` and
//!     **throws** on a non-2xx (the moderation signal is the SDK's thrown message
//!     — a recorded fixture, never a synthesized 200). So their `parse_*` only
//!     handles the success body; a transport `Err` surfaces verbatim.
//!   - **raw-fetch providers** (GOOGLE / OPENROUTER) inspect the status
//!     themselves, so their `parse_*` takes the full [`WireResponse`] and
//!     constructs the HTTP-error / manufactured-moderation strings.
//!
//! ## Documented seams (faithful, corpus-bounded)
//!
//!   - Google Gemini's `imageConfig.imageSize` extended param is not in
//!     [`ImageGenParams`] (v4 reads it off an extension the v5 handler never sets),
//!     so `imageConfig` here carries only `aspectRatio`.
//!   - The refusal-keyword GAPs are v4 behavior carried verbatim: Gemini's
//!     `textResponse || 'No images returned…'`, OpenRouter's `Model declined…`, and
//!     z-ai's total lack of moderation handling never match
//!     `is_image_moderation_error`. Never widen the keyword set to "fix" a gap.

use serde_json::{Map, Value};

use crate::jsstr;
use crate::model::image::{GeneratedImageData, ImageGenError, ImageGenParams, ImageGenResponse};
use crate::model::request_builder::BuiltRequest;
use crate::model::wire::WireResponse;

// ===========================================================================
// Small helpers
// ===========================================================================

/// An ordered JSON object (insertion order = wire key order under preserve_order).
fn obj() -> Map<String, Value> {
    Map::new()
}

/// JS `s` when truthy (a non-empty string), else `None` (`||` skips falsy).
fn truthy_str(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// JS `x.foo` as a `String` when present-and-a-string (drops `null`/absent).
fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// The provider's default image model (v4 `params.model ?? '<default>'`).
fn model_or_default<'a>(params: &'a ImageGenParams, default: &'a str) -> &'a str {
    if params.model.is_empty() {
        default
    } else {
        params.model.as_str()
    }
}

// ===========================================================================
// build_image_request
// ===========================================================================

/// Build the wire request for `provider` (the canonical UPPERCASE id). Headers
/// carry the non-secret fixed set; the auth header (Bearer / `x-goog-api-key`) is
/// appended by [`RealImageProvider`] once it holds the key. The differential diffs
/// method / url / body only (the api key + user-agent version are the transport's).
pub fn build_image_request(
    provider: &str,
    params: &ImageGenParams,
) -> Result<BuiltRequest, ImageGenError> {
    let (url, body) = match provider {
        "OPENAI" => build_openai(params),
        "GROK" => build_grok(params),
        "Z_AI" => build_zai(params),
        "GOOGLE" => build_google(params),
        "OPENROUTER" => build_openrouter(params),
        other => {
            return Err(ImageGenError::new(format!(
                "Unknown image provider: {other}"
            )))
        }
    };
    let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
    if provider == "OPENROUTER" {
        headers.push((
            "HTTP-Referer".to_string(),
            std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
        ));
        headers.push(("X-Title".to_string(), "Quilltap".to_string()));
    }
    Ok(BuiltRequest {
        method: "POST".to_string(),
        url,
        headers,
        body,
    })
}

fn is_gpt_image_model(model: &str) -> bool {
    model.starts_with("gpt-image-")
}

/// v4 OpenAI `validateAndNormalizeSize`.
fn validate_and_normalize_size(size: Option<&str>, model: &str) -> String {
    let Some(size) = size.filter(|s| !s.is_empty()) else {
        return "1024x1024".to_string();
    };
    if is_gpt_image_model(model) {
        let ok = ["1024x1024", "1024x1536", "1536x1024", "auto"];
        return if ok.contains(&size) {
            size
        } else {
            "1024x1024"
        }
        .to_string();
    }
    if model == "dall-e-3" {
        let ok = ["1024x1024", "1024x1792", "1792x1024"];
        return if ok.contains(&size) {
            size
        } else {
            "1024x1024"
        }
        .to_string();
    }
    let ok = ["256x256", "512x512", "1024x1024"];
    if ok.contains(&size) {
        size
    } else {
        "1024x1024"
    }
    .to_string()
}

fn build_openai(params: &ImageGenParams) -> (String, Value) {
    // requestParams.model = params.model; size validation uses `params.model ?? 'dall-e-3'`.
    let model = params.model.as_str();
    let model_name = model_or_default(params, "dall-e-3");
    let is_gpt = is_gpt_image_model(model);
    let mut body = obj();
    body.insert("model".into(), Value::String(model.to_string()));
    body.insert("prompt".into(), Value::String(params.prompt.clone()));
    body.insert("n".into(), Value::from(params.n.unwrap_or(1)));
    if !is_gpt {
        body.insert("response_format".into(), Value::String("b64_json".into()));
    }
    body.insert(
        "size".into(),
        Value::String(validate_and_normalize_size(
            params.size.as_deref(),
            model_name,
        )),
    );
    if !is_gpt {
        body.insert(
            "quality".into(),
            Value::String(params.quality.clone().unwrap_or_else(|| "standard".into())),
        );
        body.insert(
            "style".into(),
            Value::String(params.style.clone().unwrap_or_else(|| "vivid".into())),
        );
    }
    (
        "https://api.openai.com/v1/images/generations".to_string(),
        Value::Object(body),
    )
}

fn build_grok(params: &ImageGenParams) -> (String, Value) {
    let model = model_or_default(params, "grok-imagine-image");
    let mut body = obj();
    body.insert("model".into(), Value::String(model.to_string()));
    body.insert("prompt".into(), Value::String(params.prompt.clone()));
    body.insert("n".into(), Value::from(params.n.unwrap_or(1)));
    body.insert("response_format".into(), Value::String("b64_json".into()));
    if let Some(ar) = params.aspect_ratio.as_deref().filter(|s| !s.is_empty()) {
        body.insert("aspect_ratio".into(), Value::String(ar.to_string()));
    }
    if model.starts_with("grok-imagine-") && model.ends_with("-pro") {
        body.insert("resolution".into(), Value::String("2k".into()));
    }
    (
        "https://api.x.ai/v1/images/generations".to_string(),
        Value::Object(body),
    )
}

fn build_zai(params: &ImageGenParams) -> (String, Value) {
    let model = model_or_default(params, "cogview-4-250304");
    let mut body = obj();
    body.insert("model".into(), Value::String(model.to_string()));
    body.insert("prompt".into(), Value::String(params.prompt.clone()));
    body.insert("n".into(), Value::from(params.n.unwrap_or(1)));
    // size is ALWAYS set: params.size || (glm-image ? 1280x1280 : 1024x1024).
    let size = params
        .size
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if model == "glm-image" {
                "1280x1280".to_string()
            } else {
                "1024x1024".to_string()
            }
        });
    body.insert("size".into(), Value::String(size));
    if let Some(q) = params.quality.as_deref().filter(|s| !s.is_empty()) {
        body.insert("quality".into(), Value::String(q.to_string()));
    }
    (
        "https://api.z.ai/api/paas/v4/images/generations".to_string(),
        Value::Object(body),
    )
}

/// v4 Google `GEMINI_IMAGE_MODELS`.
const GEMINI_IMAGE_MODELS: [&str; 2] = ["gemini-2.5-flash-image", "gemini-3-pro-image-preview"];

/// v4 `isGeminiImageModel`.
fn is_gemini_image_model(model: &str) -> bool {
    GEMINI_IMAGE_MODELS
        .iter()
        .any(|m| model == *m || model.starts_with(&format!("{m}-")) || model.contains(m))
}

/// v4 `IMAGEN_MODEL_MAP` (user-friendly → API id).
fn imagen_api_model_id(model: &str) -> String {
    match model {
        "imagen-4" => "imagen-4.0-generate-001".to_string(),
        "imagen-4-fast" => "imagen-4.0-fast-generate-001".to_string(),
        other => other.to_string(),
    }
}

fn build_google(params: &ImageGenParams) -> (String, Value) {
    let model = model_or_default(params, "imagen-4");
    let base = "https://generativelanguage.googleapis.com/v1beta";
    if is_gemini_image_model(model) {
        // generateWithGemini
        let mut parts = obj();
        parts.insert("text".into(), Value::String(params.prompt.clone()));
        let mut gen_config = obj();
        gen_config.insert(
            "responseModalities".into(),
            Value::Array(vec![
                Value::String("TEXT".into()),
                Value::String("IMAGE".into()),
            ]),
        );
        let mut image_config = obj();
        if let Some(ar) = params.aspect_ratio.as_deref().filter(|s| !s.is_empty()) {
            image_config.insert("aspectRatio".into(), Value::String(ar.to_string()));
        }
        // imageSize is an extension the v5 handler never sets — documented seam.
        if !image_config.is_empty() {
            gen_config.insert("imageConfig".into(), Value::Object(image_config));
        }
        let mut body = obj();
        body.insert(
            "contents".into(),
            Value::Array(vec![Value::Object({
                let mut c = obj();
                c.insert("parts".into(), Value::Array(vec![Value::Object(parts)]));
                c
            })]),
        );
        body.insert("generationConfig".into(), Value::Object(gen_config));
        (
            format!("{base}/models/{model}:generateContent"),
            Value::Object(body),
        )
    } else {
        // generateWithImagen
        let api_model_id = imagen_api_model_id(model);
        let mut instance = obj();
        instance.insert("prompt".into(), Value::String(params.prompt.clone()));
        let mut parameters = obj();
        parameters.insert("sampleCount".into(), Value::from(params.n.unwrap_or(1)));
        if let Some(ar) = params.aspect_ratio.as_deref().filter(|s| !s.is_empty()) {
            parameters.insert("aspectRatio".into(), Value::String(ar.to_string()));
        }
        if let Some(seed) = params.seed {
            parameters.insert("seed".into(), Value::from(seed));
        }
        let mut body = obj();
        body.insert(
            "instances".into(),
            Value::Array(vec![Value::Object(instance)]),
        );
        body.insert("parameters".into(), Value::Object(parameters));
        (
            format!("{base}/models/{api_model_id}:predict"),
            Value::Object(body),
        )
    }
}

/// v4 OpenRouter `FALLBACK_IMAGE_MODELS[0]`.
const OPENROUTER_DEFAULT_MODEL: &str = "google/gemini-2.5-flash-preview-native-image";

fn build_openrouter(params: &ImageGenParams) -> (String, Value) {
    let model = model_or_default(params, OPENROUTER_DEFAULT_MODEL);
    // negativePrompt + style are APPENDED TO THE PROMPT STRING.
    let mut prompt = params.prompt.clone();
    if let Some(np) = params.negative_prompt.as_deref().filter(|s| !s.is_empty()) {
        prompt.push_str(&format!("\n\nAvoid the following in the image: {np}"));
    }
    if let Some(style) = params.style.as_deref().filter(|s| !s.is_empty()) {
        prompt.push_str(&format!("\n\nUse a {style} artistic style."));
    }
    let mut message = obj();
    message.insert("role".into(), Value::String("user".into()));
    message.insert("content".into(), Value::String(prompt));
    let mut body = obj();
    body.insert("model".into(), Value::String(model.to_string()));
    body.insert(
        "messages".into(),
        Value::Array(vec![Value::Object(message)]),
    );
    body.insert(
        "modalities".into(),
        Value::Array(vec![
            Value::String("image".into()),
            Value::String("text".into()),
        ]),
    );
    let mut image_config = obj();
    if let Some(ar) = params.aspect_ratio.as_deref().filter(|s| !s.is_empty()) {
        image_config.insert("aspect_ratio".into(), Value::String(ar.to_string()));
    }
    if params.quality.as_deref() == Some("hd") {
        image_config.insert("image_size".into(), Value::String("4K".into()));
    }
    if !image_config.is_empty() {
        body.insert("image_config".into(), Value::Object(image_config));
    }
    (
        "https://openrouter.ai/api/v1/chat/completions".to_string(),
        Value::Object(body),
    )
}

// ===========================================================================
// parse_image_response
// ===========================================================================

/// Parse a provider's response into an [`ImageGenResponse`], or the exact error
/// string v4 throws. `model` selects the Google dialect (Imagen vs Gemini).
///
/// For SDK providers (OPENAI / GROK / Z_AI) this is only ever called on a
/// successful body (a transport throw is surfaced by [`RealImageProvider`]).
pub fn parse_image_response(
    provider: &str,
    model: &str,
    resp: &WireResponse,
) -> Result<ImageGenResponse, ImageGenError> {
    match provider {
        "OPENAI" => parse_openai_like(resp, "OpenAI", "image/png"),
        "GROK" => parse_openai_like(resp, "Grok", "image/jpeg"),
        "Z_AI" => parse_zai(resp),
        "GOOGLE" => parse_google(model, resp),
        "OPENROUTER" => parse_openrouter(resp),
        other => Err(ImageGenError::new(format!(
            "Unknown image provider: {other}"
        ))),
    }
}

/// OpenAI / Grok: `data: img.b64_json || img.url || ''`, mimeType HARDCODED.
fn parse_openai_like(
    resp: &WireResponse,
    name: &str,
    mime: &str,
) -> Result<ImageGenResponse, ImageGenError> {
    let v: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    let Some(arr) = v.get("data").and_then(Value::as_array) else {
        return Err(ImageGenError::new(format!(
            "Invalid response from {name} Images API"
        )));
    };
    let images = arr
        .iter()
        .map(|img| {
            let data = truthy_str(img.get("b64_json"))
                .or_else(|| truthy_str(img.get("url")))
                .unwrap_or("")
                .to_string();
            GeneratedImageData {
                data: Some(data),
                url: None,
                mime_type: Some(mime.to_string()),
                revised_prompt: str_of(img, "revised_prompt"),
            }
        })
        .collect();
    Ok(ImageGenResponse { images })
}

/// Z-AI: keeps BOTH `b64_json` AND `url` (the only happy-path `url` provider).
fn parse_zai(resp: &WireResponse) -> Result<ImageGenResponse, ImageGenError> {
    let v: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    let Some(arr) = v.get("data").and_then(Value::as_array) else {
        return Err(ImageGenError::new("Invalid response from Z.AI Images API"));
    };
    let images = arr
        .iter()
        .map(|img| GeneratedImageData {
            data: str_of(img, "b64_json"),
            url: str_of(img, "url"),
            mime_type: Some("image/png".to_string()),
            revised_prompt: str_of(img, "revised_prompt"),
        })
        .collect();
    Ok(ImageGenResponse { images })
}

fn parse_google(model: &str, resp: &WireResponse) -> Result<ImageGenResponse, ImageGenError> {
    let is_gemini = is_gemini_image_model(model);
    if !resp.ok() {
        let err_json: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
        let message = err_json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        let fallback = if is_gemini {
            format!("Gemini API error: {}", resp.status)
        } else {
            format!("Google Imagen API error: {}", resp.status)
        };
        return Err(ImageGenError::new(
            message.map(str::to_string).unwrap_or(fallback),
        ));
    }
    let data: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    if is_gemini {
        parse_gemini(&data)
    } else {
        parse_imagen(&data)
    }
}

fn parse_gemini(data: &Value) -> Result<ImageGenResponse, ImageGenError> {
    let mut images = Vec::new();
    let mut text_response = String::new();
    if let Some(parts) = data
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
    {
        for part in parts {
            if let Some(inline) = part.get("inlineData") {
                images.push(GeneratedImageData {
                    data: str_of(inline, "data"),
                    url: None,
                    mime_type: Some(
                        str_of(inline, "mimeType").unwrap_or_else(|| "image/png".to_string()),
                    ),
                    revised_prompt: None,
                });
            } else if let Some(text) = truthy_str(part.get("text")) {
                text_response = text.to_string();
            }
        }
    }
    if images.is_empty() {
        // The documented keyword GAP: `textResponse || 'No images…'` matches no keyword.
        return Err(ImageGenError::new(if text_response.is_empty() {
            "No images returned from Gemini API".to_string()
        } else {
            text_response
        }));
    }
    Ok(ImageGenResponse { images })
}

fn parse_imagen(data: &Value) -> Result<ImageGenResponse, ImageGenError> {
    let empty = Vec::new();
    let predictions = data
        .get("predictions")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let usable: Vec<&Value> = predictions
        .iter()
        .filter(|p| truthy_str(p.get("bytesBase64Encoded")).is_some())
        .collect();
    if usable.is_empty() {
        // The ONLY manufactured moderation error — keyword-matching `content policy`.
        let reason = predictions
            .iter()
            .find_map(|p| p.get("raiFilteredReason").and_then(Value::as_str))
            .map(str::to_string)
            .or_else(|| str_of(data, "raiFilteredReason"))
            .or_else(|| str_of(data, "filteredReason"));
        let suffix = match reason {
            Some(r) if !r.is_empty() => format!(": {r}"),
            _ => String::new(),
        };
        return Err(ImageGenError::new(format!(
            "Google Imagen rejected prompt by content policy{suffix}"
        )));
    }
    let images = usable
        .iter()
        .map(|pred| GeneratedImageData {
            data: str_of(pred, "bytesBase64Encoded"),
            url: None,
            mime_type: Some(str_of(pred, "mimeType").unwrap_or_else(|| "image/png".to_string())),
            revised_prompt: None,
        })
        .collect();
    Ok(ImageGenResponse { images })
}

/// v4 OpenRouter `data:(image/[^;]+);base64,(.+)` data-URI matcher (JS `.`
/// excludes line terminators; base64 payloads carry none, so the `regex` crate's
/// default `.` [also newline-excluding] matches identically).
fn extract_openrouter_image(url: &str, images: &mut Vec<GeneratedImageData>) {
    use std::sync::LazyLock;
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^data:(image/[^;]+);base64,(.+)$").unwrap());
    if let Some(caps) = RE.captures(url) {
        images.push(GeneratedImageData {
            data: Some(caps[2].to_string()),
            url: None,
            mime_type: Some(caps[1].to_string()),
            revised_prompt: None,
        });
    } else {
        images.push(GeneratedImageData {
            data: None,
            url: Some(url.to_string()),
            mime_type: Some("image/png".to_string()),
            revised_prompt: None,
        });
    }
}

fn parse_openrouter(resp: &WireResponse) -> Result<ImageGenResponse, ImageGenError> {
    if !resp.ok() {
        return Err(ImageGenError::new(format!(
            "OpenRouter API error: {} - {}",
            resp.status, resp.body
        )));
    }
    let data: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    let mut images: Vec<GeneratedImageData> = Vec::new();
    let mut text_content = String::new();
    if let Some(choices) = data.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let Some(message) = choice.get("message") else {
                continue;
            };
            if let Some(imgs) = message.get("images").and_then(Value::as_array) {
                for img in imgs {
                    let url = img
                        .get("image_url")
                        .and_then(|iu| iu.get("url"))
                        .and_then(Value::as_str)
                        .or_else(|| img.get("url").and_then(Value::as_str));
                    if let Some(url) = url {
                        extract_openrouter_image(url, &mut images);
                    }
                }
            }
            if let Some(refusal) = truthy_str(message.get("refusal")) {
                text_content = refusal.to_string();
            }
            if let Some(content) = truthy_str(message.get("content")) {
                text_content = content.to_string();
            }
            if let Some(parts) = message.get("content").and_then(Value::as_array) {
                for part in parts {
                    let part_type = part.get("type").and_then(Value::as_str);
                    if part_type == Some("image_url") {
                        if let Some(url) = part
                            .get("image_url")
                            .and_then(|iu| iu.get("url"))
                            .and_then(Value::as_str)
                        {
                            extract_openrouter_image(url, &mut images);
                        }
                    } else if part_type == Some("text") {
                        if let Some(text) = truthy_str(part.get("text")) {
                            text_content = text.to_string();
                        }
                    }
                    let inline = part.get("inline_data").or_else(|| part.get("inlineData"));
                    if let Some(inline) = inline {
                        if let Some(d) = truthy_str(inline.get("data")) {
                            images.push(GeneratedImageData {
                                data: Some(d.to_string()),
                                url: None,
                                mime_type: Some(
                                    str_of(inline, "mimeType")
                                        .or_else(|| str_of(inline, "mime_type"))
                                        .unwrap_or_else(|| "image/png".to_string()),
                                ),
                                revised_prompt: None,
                            });
                        }
                    }
                }
            }
        }
    }
    if images.is_empty() {
        // Documented keyword GAP: `Model declined…` matches no keyword.
        if !text_content.is_empty() {
            let summary = if jsstr::utf16_len(&text_content) > 200 {
                format!("{}...", jsstr::utf16_truncate(&text_content, 200))
            } else {
                text_content
            };
            return Err(ImageGenError::new(format!(
                "Model declined to generate an image: {summary}"
            )));
        }
        return Err(ImageGenError::new("No images returned from OpenRouter API"));
    }
    Ok(ImageGenResponse { images })
}

// ===========================================================================
// The real ImageProvider (composes build + transport + parse)
// ===========================================================================

use crate::model::image::ImageProvider;
use crate::model::wire::WireTransport;

/// The real [`ImageProvider`] — builds the wire request, sends it over the
/// injected [`WireTransport`], and parses the response into an [`ImageGenResponse`]
/// (closing the `generate_image` provider seam). A transport `Err` (an SDK
/// converting a non-2xx to a throw, or a network error) surfaces verbatim as the
/// [`ImageGenError`] the Concierge reroute inspects; a raw-fetch provider's HTTP
/// status is handled inside [`parse_image_response`].
pub struct RealImageProvider<T: WireTransport> {
    transport: T,
}

impl<T: WireTransport> RealImageProvider<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// The per-provider auth header (v4's SDK `Authorization` / the raw-fetch
    /// header). Not differential-checked, but carried for the real transport.
    fn auth_header(provider: &str, api_key: &str) -> (String, String) {
        match provider {
            "GOOGLE" => ("x-goog-api-key".to_string(), api_key.to_string()),
            _ => ("Authorization".to_string(), format!("Bearer {api_key}")),
        }
    }

    /// v4's `if (!apiKey) throw new Error('<X> provider requires an API key')`
    /// (grok / z-ai / openrouter only).
    fn require_api_key(provider: &str, api_key: &str) -> Result<(), ImageGenError> {
        if !api_key.is_empty() {
            return Ok(());
        }
        match provider {
            "GROK" => Err(ImageGenError::new("Grok provider requires an API key")),
            "Z_AI" => Err(ImageGenError::new("Z.AI provider requires an API key")),
            "OPENROUTER" => Err(ImageGenError::new(
                "OpenRouter provider requires an API key",
            )),
            _ => Ok(()),
        }
    }
}

impl<T: WireTransport> ImageProvider for RealImageProvider<T> {
    async fn generate_image(
        &self,
        provider: &str,
        api_key: &str,
        params: &ImageGenParams,
    ) -> Result<ImageGenResponse, ImageGenError> {
        Self::require_api_key(provider, api_key)?;
        let mut request = build_image_request(provider, params)?;
        request.headers.push(Self::auth_header(provider, api_key));
        // The Google dialect is selected by model on the parse side too.
        let model = params.model.clone();
        let body = request.body_string();
        match self
            .transport
            .send(&request.method, &request.url, &request.headers, &body)
            .await
        {
            Ok(resp) => parse_image_response(provider, &model, &resp),
            // The SDK/transport throw (moderation, network) surfaces verbatim.
            Err(message) => Err(ImageGenError::new(message)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::image::ImageGenParams;

    fn params(model: &str) -> ImageGenParams {
        ImageGenParams {
            prompt: "a cat".into(),
            negative_prompt: None,
            model: model.into(),
            n: Some(1),
            size: None,
            aspect_ratio: None,
            quality: None,
            style: None,
            seed: None,
            guidance_scale: None,
            steps: None,
        }
    }

    #[test]
    fn openai_request_body_order_and_size_default() {
        let (url, body) = build_openai(&params("dall-e-3"));
        assert_eq!(url, "https://api.openai.com/v1/images/generations");
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"model":"dall-e-3","prompt":"a cat","n":1,"response_format":"b64_json","size":"1024x1024","quality":"standard","style":"vivid"}"#
        );
    }

    #[test]
    fn gpt_image_omits_response_format_quality_style() {
        let (_, body) = build_openai(&params("gpt-image-1"));
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"model":"gpt-image-1","prompt":"a cat","n":1,"size":"1024x1024"}"#
        );
    }

    #[test]
    fn imagen_model_map_and_empty_200_moderation() {
        let mut p = params("imagen-4");
        p.aspect_ratio = Some("3:4".into());
        let (url, _) = build_google(&p);
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/imagen-4.0-generate-001:predict"
        );
        // empty predictions with a raiFilteredReason → manufactured moderation error.
        let resp = WireResponse::new(200, r#"{"predictions":[{"raiFilteredReason":"policy X"}]}"#);
        let err = parse_image_response("GOOGLE", "imagen-4", &resp).unwrap_err();
        assert_eq!(
            err.message,
            "Google Imagen rejected prompt by content policy: policy X"
        );
        assert!(
            crate::services::dangerous_content::provider_routing::is_image_moderation_error(
                &err.message
            )
        );
    }

    #[test]
    fn openrouter_data_uri_and_declined_gap() {
        let resp = WireResponse::new(
            200,
            r#"{"choices":[{"message":{"images":[{"image_url":{"url":"data:image/png;base64,QUJD"}}]}}]}"#,
        );
        let ok = parse_image_response("OPENROUTER", OPENROUTER_DEFAULT_MODEL, &resp).unwrap();
        assert_eq!(ok.images[0].data.as_deref(), Some("QUJD"));
        assert_eq!(ok.images[0].mime_type.as_deref(), Some("image/png"));

        let declined = WireResponse::new(
            200,
            r#"{"choices":[{"message":{"refusal":"nope, policy"}}]}"#,
        );
        let err =
            parse_image_response("OPENROUTER", OPENROUTER_DEFAULT_MODEL, &declined).unwrap_err();
        assert_eq!(
            err.message,
            "Model declined to generate an image: nope, policy"
        );
        // The GAP: this must NOT be classified as a moderation error.
        assert!(
            !crate::services::dangerous_content::provider_routing::is_image_moderation_error(
                &err.message
            )
        );
    }

    #[test]
    fn zai_keeps_url_and_b64() {
        let resp = WireResponse::new(
            200,
            r#"{"data":[{"url":"https://z.ai/img.png"},{"b64_json":"QUJD"}]}"#,
        );
        let ok = parse_image_response("Z_AI", "glm-image", &resp).unwrap();
        assert_eq!(ok.images[0].data, None);
        assert_eq!(ok.images[0].url.as_deref(), Some("https://z.ai/img.png"));
        assert_eq!(ok.images[1].data.as_deref(), Some("QUJD"));
        assert_eq!(ok.images[1].url, None);
    }
}
