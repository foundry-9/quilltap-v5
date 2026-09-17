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

use crate::db::js_number_to_json;
use crate::jsstr;
use crate::model::image::{GeneratedImageData, ImageGenError, ImageGenParams, ImageGenResponse};
use crate::model::nanogpt_loras::{apply_loras, apply_passthrough_parameters};
use crate::model::openai_image_models::{
    arbitrary_size_rules, check_arbitrary_size, find_image_model, is_gpt_image_model,
    mime_type_for_format, openai_image_model_ids, parse_size, ArbitrarySizeCheck, BACKGROUNDS,
    COMPRESSIBLE_FORMATS, MODERATIONS, OUTPUT_FORMATS, TRANSPARENCY_CAPABLE_FORMATS,
};
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
    build_image_request_with_extras(provider, params).map(|(request, _)| request)
}

/// [`build_image_request`] plus NanoGPT's composed-body summary, for the one
/// caller that needs it: [`RealImageProvider`]'s debug + bug-111 failure logs
/// (`648d5c8aa`). `None` for every other provider.
pub fn build_image_request_with_extras(
    provider: &str,
    params: &ImageGenParams,
) -> Result<(BuiltRequest, Option<NanoGptWireExtras>), ImageGenError> {
    let mut nanogpt_extras: Option<NanoGptWireExtras> = None;
    let (url, body) = match provider {
        "OPENAI" => build_openai(params),
        "GROK" => build_grok(params),
        "Z_AI" => build_zai(params),
        "GOOGLE" => build_google(params),
        "OPENROUTER" => build_openrouter(params),
        "NANOGPT" => {
            let (url, body, extras) = build_nanogpt(params);
            nanogpt_extras = Some(extras);
            (url, body)
        }
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
    Ok((
        BuiltRequest {
            method: "POST".to_string(),
            url,
            headers,
            body,
            attachment_results: Default::default(),
        },
        nanogpt_extras,
    ))
}

/// v4 OpenAI `validateAndNormalizeSize` (`d8d2890ee`).
///
/// GPT Image 2 and both GPT Image 2.5 models take any `WIDTHxHEIGHT` meeting
/// the documented edge, aspect and pixel rules, so those are forwarded as
/// given; everything else is checked against its family's standard list. An
/// unusable size falls back to `1024x1024` — the one size every family
/// supports — and says in the log what was wrong, because silently returning a
/// differently-shaped image is the harder failure to diagnose.
///
/// A model the table does not recognise (a live `/v1/models` listing can name
/// one) has its size forwarded untouched rather than guessed at.
fn validate_and_normalize_size(size: Option<&str>, model: &str) -> String {
    // v4 `if (!size)` — a truthy test, so `''` takes the default and the
    // fallback WARN below is never reached for it.
    let Some(size) = size.filter(|s| !s.is_empty()) else {
        return "1024x1024".to_string();
    };
    let Some(caps) = find_image_model(Some(model)) else {
        return size.to_string();
    };
    if caps.sizes.contains(&size) {
        return size.to_string();
    }
    if caps.arbitrary_sizes {
        match check_arbitrary_size(size) {
            ArbitrarySizeCheck::Ok => {
                // `parseSize(size)!` — the check already proved it parses.
                if let Some(p) = parse_size(size) {
                    if p.width * p.height > arbitrary_size_rules::EXPERIMENTAL_ABOVE_PIXELS {
                        tracing::debug!(
                            context = "OpenAIImageProvider.validateAndNormalizeSize",
                            model = %model,
                            size = %size,
                            "Requesting an experimental OpenAI image resolution"
                        );
                    }
                }
                return size.to_string();
            }
            ArbitrarySizeCheck::Rejected(reason) => {
                tracing::warn!(
                    context = "OpenAIImageProvider.validateAndNormalizeSize",
                    model = %model,
                    size = %size,
                    reason = %reason,
                    "Falling back to 1024x1024: unsupported OpenAI image size"
                );
                return "1024x1024".to_string();
            }
        }
    }
    tracing::warn!(
        context = "OpenAIImageProvider.validateAndNormalizeSize",
        model = %model,
        size = %size,
        supported = %caps.sizes.join(", "),
        "Falling back to 1024x1024: size not supported by this OpenAI model"
    );
    "1024x1024".to_string()
}

/// v4 OpenAI `resolveQuality` (`d8d2890ee`).
///
/// The tiers are family-specific — `xhigh` and `max` are GPT Image 2.5's alone,
/// and `hd` is DALL·E 3's — so a profile carrying a tier the chosen model does
/// not know is dropped rather than forwarded into a 400. GPT Image sends
/// nothing when unset (the API's own `auto` default applies); DALL·E keeps its
/// historical `standard` default.
fn resolve_quality(quality: Option<&str>, model: &str) -> Option<String> {
    let caps = find_image_model(Some(model));
    let is_gpt = is_gpt_image_model(Some(model));
    let default = || {
        if is_gpt {
            None
        } else {
            Some("standard".to_string())
        }
    };
    // v4 `if (!quality)` — truthy, so `''` is "unset".
    let Some(quality) = quality.filter(|q| !q.is_empty()) else {
        return default();
    };
    let Some(caps) = caps else {
        return Some(quality.to_string());
    };
    if caps.qualities.contains(&quality) {
        return Some(quality.to_string());
    }
    tracing::warn!(
        context = "OpenAIImageProvider.resolveQuality",
        model = %model,
        quality = %quality,
        supported = %caps.qualities.join(", "),
        "Ignoring quality tier unsupported by this OpenAI model"
    );
    default()
}

/// Read one of a known set of strings out of the profile's residual parameter
/// bag (v4 `readEnum`). Anything unrecognised is dropped with a warning rather
/// than forwarded — the Images API rejects the whole request over one bad
/// enum, so a typo in a stored profile would otherwise take the image down
/// with it.
///
/// `undefined` / `null` / `''` are all "unset", silently. The warning renders
/// the offending value through JS `String()`, so an object arrives as
/// `[object Object]` and an array as its join — v4's own `String(raw)`.
fn read_enum(bag: Option<&Value>, key: &str, allowed: &[&str]) -> Option<String> {
    read_enum_impl(bag, key, allowed, true)
}

/// [`read_enum`] without the warning — for a SECOND read of a key the request
/// assembly has already read and warned about once. v4 reads each key exactly
/// once, inside `generateImage`, so one bad value is one warning per request;
/// [`openai_output_format`] re-derives the format at parse time and must not
/// double it (caught by the `53294163f` round's §3 review — the build-only
/// capture in the tests below could not see the parse-side repeat).
fn read_enum_quiet(bag: Option<&Value>, key: &str, allowed: &[&str]) -> Option<String> {
    read_enum_impl(bag, key, allowed, false)
}

fn read_enum_impl(bag: Option<&Value>, key: &str, allowed: &[&str], warn: bool) -> Option<String> {
    let raw = bag?.get(key)?;
    if raw.is_null() || raw.as_str() == Some("") {
        return None;
    }
    if let Some(s) = raw.as_str() {
        if allowed.contains(&s) {
            return Some(s.to_string());
        }
    }
    if warn {
        tracing::warn!(
            context = "OpenAIImageProvider.readEnum",
            key = %key,
            value = %crate::pascal::js_value::to_js_string(raw),
            allowed = %allowed.join(", "),
            "Ignoring unsupported OpenAI image parameter value"
        );
    }
    None
}

/// Read an integer in `[min, max]` from the residual bag, dropping anything
/// else (v4 `readIntInRange`). The conversion is JS `Number()` — `'80'` is 80,
/// `true` is 1, `[]` is 0, `{}` is NaN — and the guard is
/// `Number.isFinite && Number.isInteger`, so a fractional value is refused.
fn read_int_in_range(bag: Option<&Value>, key: &str, min: f64, max: f64) -> Option<f64> {
    let raw = bag?.get(key)?;
    if raw.is_null() || raw.as_str() == Some("") {
        return None;
    }
    // v4 `typeof raw === 'number' ? raw : Number(raw)` — the two branches agree
    // for a JSON number, so `to_number` covers both.
    let value = crate::pascal::js_value::to_number(raw);
    if !value.is_finite() || value.fract() != 0.0 || value < min || value > max {
        tracing::warn!(
            context = "OpenAIImageProvider.readIntInRange",
            key = %key,
            value = %crate::pascal::js_value::to_js_string(raw),
            min = min,
            max = max,
            "Ignoring out-of-range OpenAI image parameter"
        );
        return None;
    }
    Some(value)
}

/// The MIME type OPENAI's parsed images carry: v4 computes it in
/// `generateImage` from the `outputFormat` local the BODY used — i.e. after the
/// transparent-background force — so it is a pure function of `params` and the
/// parser can ask for it directly rather than take it down a side channel.
///
/// `None` (a DALL·E model, or a GPT Image call with no `output_format` in the
/// bag) renders as `image/png`, which is what v4's `mimeTypeForFormat(undefined)`
/// returns and what the hardcoded value used to be.
fn openai_output_format(params: &ImageGenParams) -> Option<String> {
    let model_name = model_or_default(params, "dall-e-3");
    if !is_gpt_image_model(Some(model_name)) {
        return None;
    }
    let bag = params.profile_parameters.as_ref();
    // QUIET reads: `build_openai` already read both keys and warned about a
    // bad value once, which is v4's count — the parse-side re-derivation is a
    // pure function of the same bag and must add no log line.
    let output_format = read_enum_quiet(bag, "output_format", &OUTPUT_FORMATS);
    let background = read_enum_quiet(bag, "background", &BACKGROUNDS);
    match (&background, &output_format) {
        (Some(b), Some(f))
            if b == "transparent" && !TRANSPARENCY_CAPABLE_FORMATS.contains(&f.as_str()) =>
        {
            Some("png".to_string())
        }
        _ => output_format,
    }
}

/// v4 OpenAI `generateImage`'s request assembly (`d8d2890ee`).
///
/// Every per-model fact this branches on — which parameters a family accepts,
/// which quality tiers, which sizes — comes from
/// [`crate::model::openai_image_models`], so the wire logic never re-states a
/// model's limits.
///
/// Body KEY ORDER is v4's assignment order: `model, prompt, n,
/// [response_format], size, [quality], [style], [background], [output_format],
/// [output_compression], [moderation]`.
fn build_openai(params: &ImageGenParams) -> (String, Value) {
    // `requestParams.model = params.model` is the RAW value — an absent model
    // never reaches the wire — while `?? 'dall-e-3'` drove every validation
    // below. v5's `model` is a `String`, so "absent" is the empty one.
    let model = params.model.as_str();
    let model_name = model_or_default(params, "dall-e-3");
    let caps = find_image_model(Some(model_name));
    let is_gpt = is_gpt_image_model(Some(model_name));
    let bag = params.profile_parameters.as_ref();

    let requested_n = params.n.unwrap_or(1.0);
    let n = match caps {
        Some(c) => requested_n.max(1.0).min(c.max_n),
        None => requested_n,
    };
    if n != requested_n {
        tracing::warn!(
            context = "OpenAIImageProvider.generateImage",
            model = %model_name,
            requested = requested_n,
            capped = n,
            "Capping image count to the model maximum"
        );
    }

    let mut body = obj();
    if !model.is_empty() {
        body.insert("model".into(), Value::String(model.to_string()));
    }
    body.insert("prompt".into(), Value::String(params.prompt.clone()));
    body.insert("n".into(), js_number_to_json(n));

    // gpt-image models always return base64 and reject response_format;
    // DALL-E models default to a URL, so ask them for b64_json explicitly.
    if !is_gpt {
        body.insert("response_format".into(), Value::String("b64_json".into()));
    }

    let size = validate_and_normalize_size(params.size.as_deref(), model_name);
    body.insert("size".into(), Value::String(size.clone()));

    let quality = resolve_quality(params.quality.as_deref(), model_name);
    if let Some(q) = &quality {
        body.insert("quality".into(), Value::String(q.clone()));
    }

    // style is DALL-E 3's alone — dall-e-2 and the GPT Image families reject
    // it. `params.style ?? 'vivid'` is NULLISH, so an empty string rides.
    let supports_style = match caps {
        Some(c) => c.supports_style,
        None => !is_gpt,
    };
    if supports_style {
        body.insert(
            "style".into(),
            Value::String(params.style.clone().unwrap_or_else(|| "vivid".into())),
        );
    }

    // ---- GPT Image extras -------------------------------------------------
    // background / output_format / output_compression / moderation are the
    // GPT Image families' own parameters; they ride the profile's residual bag
    // under their wire names, so the host never has to enumerate them.
    if is_gpt {
        let mut output_format = read_enum(bag, "output_format", &OUTPUT_FORMATS);
        let background = read_enum(bag, "background", &BACKGROUNDS);
        let moderation = read_enum(bag, "moderation", &MODERATIONS);
        let output_compression = read_int_in_range(bag, "output_compression", 0.0, 100.0);

        if let Some(background) = background {
            // A transparent background needs an alpha-capable format. Honour
            // the more specific intent — the user asked for transparency — and
            // say so, rather than letting the API return a silently flattened
            // JPEG.
            if background == "transparent" {
                if let Some(f) = output_format.clone() {
                    if !TRANSPARENCY_CAPABLE_FORMATS.contains(&f.as_str()) {
                        tracing::warn!(
                            context = "OpenAIImageProvider.generateImage",
                            model = %model_name,
                            requestedFormat = %f,
                            "Forcing PNG output: a transparent background needs png or webp"
                        );
                        output_format = Some("png".to_string());
                    }
                }
            }
            body.insert("background".into(), Value::String(background));
        }

        if let Some(f) = &output_format {
            body.insert("output_format".into(), Value::String(f.clone()));
        }

        if let Some(c) = output_compression {
            // Only webp and jpeg are compressed; png ignores the parameter, and
            // the API rejects it outright on some families.
            match &output_format {
                Some(f) if COMPRESSIBLE_FORMATS.contains(&f.as_str()) => {
                    body.insert("output_compression".into(), js_number_to_json(c));
                }
                _ => {
                    tracing::debug!(
                        context = "OpenAIImageProvider.generateImage",
                        model = %model_name,
                        // v4 `outputFormat ?? 'png (default)'`.
                        outputFormat =
                            %output_format.clone().unwrap_or_else(|| "png (default)".into()),
                        "Dropping output_compression: it applies only to jpeg and webp"
                    );
                }
            }
        }

        if let Some(m) = moderation {
            body.insert("moderation".into(), Value::String(m));
        }
    }

    // v4's `logger.debug('Calling OpenAI Images API', …)` reads the ASSEMBLED
    // body, each optional key rendered `?? '(model default)'`.
    //
    // Hoisted out of the macro: a `tracing` field expression cannot name
    // `Value::as_str` — inside the macro `Value` resolves to the `tracing`
    // trait of that name (the standing note).
    const DFLT: &str = "(model default)";
    let rendered = |key: &str| -> String {
        body.get(key)
            .map(crate::pascal::js_value::to_js_string)
            .unwrap_or_else(|| DFLT.to_string())
    };
    let log_background = rendered("background");
    let log_output_format = rendered("output_format");
    let log_output_compression = rendered("output_compression");
    let log_moderation = rendered("moderation");
    tracing::debug!(
        context = "OpenAIImageProvider.generateImage",
        model = %model_name,
        size = %size,
        quality = %quality.as_deref().unwrap_or(DFLT),
        background = %log_background,
        outputFormat = %log_output_format,
        outputCompression = %log_output_compression,
        moderation = %log_moderation,
        n = n,
        "Calling OpenAI Images API"
    );

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
    body.insert("n".into(), js_number_to_json(params.n.unwrap_or(1.0)));
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
    body.insert("n".into(), js_number_to_json(params.n.unwrap_or(1.0)));
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
///
/// Any `gemini*` model routes to `generateContent` — live-fetched IDs (e.g.
/// `gemini-2.0-flash-preview-image-generation`, which the honest Fetch Models
/// list now surfaces) must not fall through to the Imagen `predict` endpoint,
/// which only serves `imagen-*` models. The prefix arm is FIRST; the original
/// exact / prefixed / substring arms over [`GEMINI_IMAGE_MODELS`] are preserved
/// behind it (they still catch a non-`gemini`-prefixed alias).
fn is_gemini_image_model(model: &str) -> bool {
    model.starts_with("gemini")
        || GEMINI_IMAGE_MODELS
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
        parameters.insert(
            "sampleCount".into(),
            js_number_to_json(params.n.unwrap_or(1.0)),
        );
        if let Some(ar) = params.aspect_ratio.as_deref().filter(|s| !s.is_empty()) {
            parameters.insert("aspectRatio".into(), Value::String(ar.to_string()));
        }
        if let Some(seed) = params.seed {
            parameters.insert("seed".into(), js_number_to_json(seed));
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
/// string v4 throws. `params.model` selects the Google dialect (Imagen vs
/// Gemini); OPENAI additionally reads the requested output format back off
/// `params` (see [`openai_output_format`]).
///
/// Taking the whole [`ImageGenParams`] rather than a bare `model` is the
/// `d8d2890ee` shape: v4's parse happens INSIDE `generateImage`, where the
/// `outputFormat` local the body used is still in scope, so `mimeType` follows
/// the (possibly forced) requested format instead of always claiming png. One
/// source beats a side channel — the format is pure in `params`.
///
/// For SDK providers (OPENAI / GROK / Z_AI) this is only ever called on a
/// successful body (a transport throw is surfaced by [`RealImageProvider`]).
pub fn parse_image_response(
    provider: &str,
    params: &ImageGenParams,
    resp: &WireResponse,
) -> Result<ImageGenResponse, ImageGenError> {
    let model = params.model.as_str();
    match provider {
        "OPENAI" => parse_openai_like(
            resp,
            "OpenAI",
            mime_type_for_format(openai_output_format(params).as_deref()),
        ),
        "GROK" => parse_openai_like(resp, "Grok", "image/jpeg"),
        "Z_AI" => parse_zai(resp),
        "GOOGLE" => parse_google(model, resp),
        "OPENROUTER" => parse_openrouter(resp),
        "NANOGPT" => parse_nanogpt(resp),
        other => Err(ImageGenError::new(format!(
            "Unknown image provider: {other}"
        ))),
    }
}

/// OpenAI / Grok: `data: img.b64_json || img.url || ''`. Grok's `mime` is
/// fixed; OPENAI's follows the requested output format (`d8d2890ee`).
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

/// v4 NanoGPT `generateImage` (P4.D101): the OpenAI-compatible images route.
///
/// Body key order is v4's literal object order — `model`, `prompt`, `n`,
/// `response_format` — then the two conditionals.
///
/// **`response_format: "b64_json"` is PINNED**, carrying v4's why: "NanoGPT
/// defaults to b64_json already; pin it so a future default change upstream
/// cannot silently hand us URLs." Unlike the OpenAI builder there is no
/// `is_gpt_image_model` exemption — NanoGPT always sends it.
///
/// `size` is passed through VERBATIM and only when supplied: v4 casts it
/// without validating or normalizing, so none of the OpenAI path's
/// size-coercion applies. `seed` rides only when the caller set it.
fn build_nanogpt(params: &ImageGenParams) -> (String, Value, NanoGptWireExtras) {
    // `params.model ?? 'hidream'` — hidream is NanoGPT's own server-side
    // default, made explicit.
    let model = model_or_default(params, "hidream");
    let mut body = obj();
    body.insert("model".into(), Value::String(model.to_string()));
    body.insert("prompt".into(), Value::String(params.prompt.clone()));
    body.insert("n".into(), js_number_to_json(params.n.unwrap_or(1.0)));
    body.insert("response_format".into(), Value::String("b64_json".into()));
    if let Some(size) = params.size.as_deref().filter(|s| !s.is_empty()) {
        body.insert("size".into(), Value::String(size.to_string()));
    }
    // v4 `84f33ce94`: NanoGPT's model-specific generation controls ride the
    // request body as FLAT keys alongside the OpenAI-compatible fields — the
    // documented mechanism for `guidance_scale` / `num_inference_steps` /
    // `strength` / `seed`, and the same channel the LoRA fields use. The OpenAI
    // SDK serialises the params object as given, so extra keys travel; this is
    // the existing `seed` cast pattern, widened. Insertion order is v4's.
    if let Some(seed) = params.seed {
        body.insert("seed".into(), js_number_to_json(seed));
    }
    if let Some(gs) = params.guidance_scale {
        body.insert("guidance_scale".into(), js_number_to_json(gs));
    }
    if let Some(steps) = params.steps {
        body.insert("num_inference_steps".into(), js_number_to_json(steps));
    }
    // `if (params.negativePrompt)` — a JS truthy test, so an EMPTY string is
    // "unset" and never reaches the wire.
    if let Some(np) = params.negative_prompt.as_deref().filter(|s| !s.is_empty()) {
        body.insert("negative_prompt".into(), Value::String(np.to_string()));
    }
    let passthrough_keys =
        apply_passthrough_parameters(&mut body, params.profile_parameters.as_ref());
    let applied = apply_loras(
        &mut body,
        model,
        &params.loras,
        params.profile_parameters.as_ref(),
    );
    (
        "https://nano-gpt.com/api/v1/images/generations".to_string(),
        Value::Object(body),
        NanoGptWireExtras {
            passthrough_keys,
            applied,
        },
    )
}

/// What `build_nanogpt` composed onto the body beyond the common fields — v4's
/// `{ passthroughKeys, applied }` pair, which its provider logs by NAME on the
/// debug line and (since `648d5c8aa`, bug 111) on the failure line too.
///
/// **Returned from the builder rather than recomputed at the provider.**
/// Running `apply_loras` a second time would fire its capping / unknown-family
/// warnings twice, and a second computation could silently drift from the one
/// that actually built the body — which is the only thing the log is worth
/// anything for.
#[derive(Debug, Clone, Default)]
pub struct NanoGptWireExtras {
    pub passthrough_keys: Vec<String>,
    pub applied: crate::model::nanogpt_loras::AppliedLoras,
}

/// v4 NanoGPT image parse (P4.D101): the OpenAI-compatible `data[]`, keeping
/// BOTH `b64_json` and `url` — the URL→base64 download happens in
/// [`RealImageProvider`], which is the only layer with a fetch.
fn parse_nanogpt(resp: &WireResponse) -> Result<ImageGenResponse, ImageGenError> {
    let v: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    let Some(arr) = v.get("data").and_then(Value::as_array) else {
        return Err(ImageGenError::new(
            "Invalid response from NanoGPT Images API",
        ));
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
// Keyed model discovery (v4 `ca22ec45` — `getAvailableModels(apiKey?)`)
// ===========================================================================
//
// Every image plugin gained (or hardened) a keyed `getAvailableModels(apiKey?)`.
// The contract is the same five ways and the asymmetries are deliberate:
//
//   - **No key → the curated static list**, with NO request made at all.
//   - **A live failure THROWS** (all five). The route catches, labels the answer
//     `builtin`, and surfaces the message as `fetchError`.
//   - **An empty result THROWS** for openai / google / grok / openrouter, but
//     NOT for z-ai, whose union with two static ids makes empty unreachable.
//
// The discovery is split the same way as the generate dialects: a pure request
// builder, a pure per-page parser, and a pure finalizer; [`RealImageProvider`]
// composes them over the injected [`WireTransport`], looping while a page token
// remains.

/// One parsed page of a provider's model list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelsPage {
    /// The ids from THIS page that passed the provider's image filter, in wire
    /// order (dedup / sort happen in [`finalize_models`]).
    pub ids: Vec<String>,
    /// The token for the next page, when the provider pages (google only).
    pub next_page_token: Option<String>,
}

/// The plugin's `supportedModels` — v4's curated per-provider list, the answer
/// when no key is supplied and the fallback the route labels `builtin`.
///
/// **Not** the manifest's `imageGenerationModels`: those two lists genuinely
/// differ (google's ordering, openrouter's entries), and the route reads the
/// PLUGIN's. Transcribed from the five `image-provider.ts` files and verified
/// against v4's live instances by the `kind:'models'` corpus rows, every one of
/// which carries the recorded `supportedModels`.
/// v4 `models.ts`'s `STATIC_IMAGE_MODEL_IDS = STATIC_IMAGE_MODELS.map(m => m.id)`:
/// the six documented flagships FOLLOWED by one entry per LoRA family, in the
/// dialect table's order. Built once; the table is compiled data.
/// `[...OPENAI_IMAGE_MODEL_IDS]` (`d8d2890ee`) — the capability table's own ids
/// in its own order, the NanoGPT idiom applied to OpenAI's table.
fn openai_static_image_model_ids() -> &'static [&'static str] {
    static IDS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    IDS.get_or_init(openai_image_model_ids).as_slice()
}

fn nanogpt_static_image_model_ids() -> &'static [&'static str] {
    static IDS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        crate::model::nanogpt_loras::NANOGPT_FLAGSHIP_IMAGE_MODEL_IDS
            .iter()
            .copied()
            .chain(
                crate::model::nanogpt_loras::nanogpt_lora_families()
                    .into_iter()
                    .map(|f| f.prefix),
            )
            .collect()
    })
    .as_slice()
}

pub fn supported_image_models(provider: &str) -> Result<&'static [&'static str], ImageGenError> {
    Ok(match provider {
        // P4.D101 — NanoGPT's `STATIC_IMAGE_MODEL_IDS` (from `models.ts`, which
        // `image-provider.ts` imports as its `supportedModels`).
        // `84f33ce94`: `STATIC_IMAGE_MODEL_IDS` is now
        // `STATIC_IMAGE_MODELS.map(m => m.id)` — the six documented flagships
        // FOLLOWED by one entry per LoRA family, generated from the dialect
        // table (`models.ts` imports `NANOGPT_LORA_FAMILIES` rather than
        // repeating the list, so a family added there appears here for free).
        // v5 derives it the same way rather than repeating the sixteen ids by
        // hand — a hand-copied list drifted from the table once already (the
        // unit-4 manifest regen), and the unification review caught the copy.
        "NANOGPT" => nanogpt_static_image_model_ids(),
        // `[...OPENAI_IMAGE_MODEL_IDS]` (`d8d2890ee`) — derived from the
        // capability table, never restated, so a family added there appears
        // here for free. NOTE the table's order: `gpt-image-1-mini` precedes
        // `gpt-image-1`, the reverse of every pre-`d8d2890ee` v5 list.
        "OPENAI" => openai_static_image_model_ids(),
        // `[...IMAGEN_MODELS, ...GEMINI_IMAGE_MODELS]` — imagen FIRST.
        "GOOGLE" => &[
            "imagen-4",
            "imagen-4-fast",
            "gemini-2.5-flash-image",
            "gemini-3-pro-image-preview",
        ],
        "GROK" => &[
            "grok-imagine-image",
            "grok-imagine-image-pro",
            "grok-2-image",
        ],
        "Z_AI" => &ZAI_STATIC_IMAGE_MODELS,
        // `FALLBACK_IMAGE_MODELS`.
        "OPENROUTER" => &[
            "google/gemini-2.5-flash-preview-native-image",
            "google/gemini-3-pro-image-preview",
            "openai/gpt-5-image",
            "openai/gpt-5-image-mini",
        ],
        other => {
            return Err(ImageGenError::new(format!(
                "Unknown image provider: {other}"
            )))
        }
    })
}

/// v4 z-ai `SUPPORTED_MODELS` — also the union floor in [`finalize_models`].
const ZAI_STATIC_IMAGE_MODELS: [&str; 2] = ["cogview-4-250304", "glm-image"];

/// Build the model-list request for `provider`. `page_token` is `Some` only on
/// google's second and later pages. Headers are COMPLETE here (including auth):
/// discovery's whole point is that the key reaches the provider, so the header
/// set is part of the contract the corpus pins. The transport still adds its own
/// `User-Agent` (a version string, deliberately not diffed).
pub fn build_models_request(
    provider: &str,
    api_key: &str,
    page_token: Option<&str>,
) -> Result<BuiltRequest, ImageGenError> {
    let bearer = || ("Authorization".to_string(), format!("Bearer {api_key}"));
    let (url, headers) = match provider {
        // OpenAI SDK `client.models.list()` → GET {baseURL}/models.
        "OPENAI" => (
            "https://api.openai.com/v1/models".to_string(),
            vec![bearer()],
        ),
        // The same SDK against Z.AI's OpenAI-compatible base URL.
        "Z_AI" => (
            "https://api.z.ai/api/paas/v4/models".to_string(),
            vec![bearer()],
        ),
        // NanoGPT's dedicated image listing, with per-model capability flags
        // (the listing also carries edit-only and upscale-only entries).
        "NANOGPT" => (
            "https://nano-gpt.com/api/v1/image-models?detailed=true".to_string(),
            vec![bearer()],
        ),
        // xAI's dedicated image-only endpoint (no name filtering needed).
        "GROK" => (
            "https://api.x.ai/v1/image-generation-models".to_string(),
            vec![bearer()],
        ),
        // A raw fetch with `pageSize=1000` and the page token appended in that
        // order (v4 sets `pageSize` first, then `pageToken`, on one `URL`).
        "GOOGLE" => {
            let mut url =
                "https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000".to_string();
            if let Some(token) = page_token {
                url.push_str("&pageToken=");
                url.push_str(&urlencoded_component(token));
            }
            (
                url,
                vec![("x-goog-api-key".to_string(), api_key.to_string())],
            )
        }
        // `@openrouter/sdk` `models.list()` → GET /api/v1/models, with the SDK's
        // `httpReferer` header (v4 passes `process.env.BASE_URL ||
        // 'http://localhost:3000'`, the same read `build_image_request` makes).
        "OPENROUTER" => (
            "https://openrouter.ai/api/v1/models".to_string(),
            vec![
                bearer(),
                (
                    "HTTP-Referer".to_string(),
                    std::env::var("BASE_URL")
                        .unwrap_or_else(|_| "http://localhost:3000".to_string()),
                ),
            ],
        ),
        other => {
            return Err(ImageGenError::new(format!(
                "Unknown image provider: {other}"
            )))
        }
    };
    Ok(BuiltRequest {
        method: "GET".to_string(),
        url,
        headers,
        body: Value::Null,
        attachment_results: Default::default(),
    })
}

/// The OpenAI SDK's `APIError` message, which v4 surfaces verbatim as
/// `fetchError`. v5's host wire hands back the status and body where v4's SDK
/// threw, so the message is reconstructed here.
///
/// The SDK's rule is three-way, not one-way (`APIError.makeMessage`):
///
/// | body | SDK message |
/// |---|---|
/// | `{"error":{"message":"Invalid model"}}` | `400 Invalid model` |
/// | `{"error":"…rejected by content moderation."}` | `400 "…rejected by content moderation."` |
/// | `service unavailable` (not JSON) | `500 service unavailable` |
///
/// A **string** `error` — no `.message` to read — is `JSON.stringify`d, quotes
/// included; only a body with no `error` at all falls back to the raw text.
/// Dogfood finding #104: the middle row is exactly what Grok's Images API
/// returns on a moderation refusal, and reading the whole raw body there (the
/// previous fallback) is not what an operator sees in v4.
///
/// All four rows measured against the REAL SDK at 2026-08-25 (a stub server per
/// case, `client.images.generate` driven through it), not transcribed.
fn openai_sdk_error(resp: &WireResponse) -> ImageGenError {
    let parsed: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    let message = match parsed.get("error") {
        Some(err) => match err.get("message") {
            // `typeof error.message === 'string' ? error.message : JSON.stringify(error.message)`
            Some(Value::String(m)) if !m.is_empty() => m.clone(),
            Some(m) => m.to_string(),
            // `error ? JSON.stringify(error) : message`
            None => err.to_string(),
        },
        None => resp.body.clone(),
    };
    ImageGenError::new(format!("{} {message}", resp.status))
}

/// Parse ONE page of `provider`'s model list into the ids that passed its image
/// filter, plus the next page token when it pages.
pub fn parse_models_page(provider: &str, resp: &WireResponse) -> Result<ModelsPage, ImageGenError> {
    match provider {
        "OPENAI" => {
            if !resp.ok() {
                return Err(openai_sdk_error(resp));
            }
            // `/^(dall-e|gpt-image)/.test(id)` — an anchored alternation.
            Ok(ModelsPage {
                ids: openai_like_ids(&resp.body, |id| {
                    id.starts_with("dall-e") || id.starts_with("gpt-image")
                }),
                next_page_token: None,
            })
        }
        // P4.D101 — a RAW fetch, so a non-ok status is v4's own sentence rather
        // than an SDK error. The filter is the capability FLAG, not the id:
        // `capabilities?.image_generation === true`, strictly true.
        "NANOGPT" => {
            if !resp.ok() {
                return Err(ImageGenError::new(format!(
                    "NanoGPT image-model listing failed: HTTP {}",
                    resp.status
                )));
            }
            let v: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
            let ids = v
                .get("data")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter(|m| {
                            m.get("capabilities")
                                .and_then(|c| c.get("image_generation"))
                                == Some(&Value::Bool(true))
                        })
                        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            Ok(ModelsPage {
                ids,
                next_page_token: None,
            })
        }
        "Z_AI" => {
            if !resp.ok() {
                return Err(openai_sdk_error(resp));
            }
            // `IMAGE_GEN_MODEL_PATTERN = /^(cogview|glm-image)/i` — the EXACT
            // negation of the chat list's filter (`STATIC_CHAT_MODEL_IDS`), so
            // the chat and image catalogues are exact complements and an id can
            // never appear in both pickers. Case-insensitive.
            Ok(ModelsPage {
                ids: openai_like_ids(&resp.body, |id| {
                    let lower = id.to_lowercase();
                    lower.starts_with("cogview") || lower.starts_with("glm-image")
                }),
                next_page_token: None,
            })
        }
        "GROK" => {
            if !resp.ok() {
                return Err(ImageGenError::new(format!(
                    "xAI image-generation-models list failed: HTTP {}",
                    resp.status
                )));
            }
            let v: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
            // `payload.models ?? payload.data ?? []` — NULLISH coalescing, so an
            // explicitly EMPTY `models` array wins and does not fall through to
            // `data` (that path is what makes the empty-list throw reachable).
            let entries = v
                .get("models")
                .filter(|x| !x.is_null())
                .or_else(|| v.get("data").filter(|x| !x.is_null()))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut ids = Vec::new();
            for entry in &entries {
                // `if (entry.id) ids.add(entry.id)` — falsy (absent / empty) skips.
                if let Some(id) = truthy_str(entry.get("id")) {
                    ids.push(id.to_string());
                }
                // Every non-empty alias is a selectable id in its own right.
                for alias in entry
                    .get("aliases")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                {
                    if let Some(a) = truthy_str(Some(alias)) {
                        ids.push(a.to_string());
                    }
                }
            }
            Ok(ModelsPage {
                ids,
                next_page_token: None,
            })
        }
        "GOOGLE" => {
            if !resp.ok() {
                return Err(ImageGenError::new(format!(
                    "Google models list failed: HTTP {}",
                    resp.status
                )));
            }
            let v: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
            let mut ids = Vec::new();
            for model in v
                .get("models")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                // `(model.name ?? '').replace(/^models\//, '')` — a non-global
                // anchored replace, so only the leading prefix goes.
                let name = model.get("name").and_then(Value::as_str).unwrap_or("");
                let id = name.strip_prefix("models/").unwrap_or(name);
                if id.is_empty() {
                    continue;
                }
                let methods: Vec<&str> = model
                    .get("supportedGenerationMethods")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                // Imagen exposes `predict`; image-output Gemini variants carry
                // "image" in the id AND expose `generateContent`. veo-* (video),
                // text and embedding models match neither — there is no explicit
                // exclusion list, and none is needed.
                let is_imagen = id.starts_with("imagen-") && methods.contains(&"predict");
                let is_gemini_image = id.starts_with("gemini")
                    && id.contains("image")
                    && methods.contains(&"generateContent");
                if is_imagen || is_gemini_image {
                    ids.push(id.to_string());
                }
            }
            Ok(ModelsPage {
                ids,
                next_page_token: v
                    .get("nextPageToken")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            })
        }
        "OPENROUTER" => {
            if !resp.ok() {
                // The speakeasy-generated SDK's error message shape, measured at
                // the pin: `API error occurred: Status {n}. Body: {raw}`.
                return Err(ImageGenError::new(format!(
                    "API error occurred: Status {}. Body: {}",
                    resp.status, resp.body
                )));
            }
            let v: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
            let mut ids = Vec::new();
            for model in v
                .get("data")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                // v4 reads WIRE key names off an object the SDK's zod has already
                // rewritten — see `openrouter_sdk_project`.
                let model = openrouter_sdk_project(model);
                let id = match model.get("id").and_then(Value::as_str) {
                    Some(id) => id.to_string(),
                    None => continue,
                };
                // Arm 1: `output_modalities || outputModalities` (model level).
                let output_modalities = model
                    .get("output_modalities")
                    .or_else(|| model.get("outputModalities"));
                if let Some(arr) = output_modalities.and_then(Value::as_array) {
                    if arr.iter().any(|m| m.as_str() == Some("image")) {
                        ids.push(id);
                        continue;
                    }
                }
                // Arm 2: `architecture.outputModality` (a string containing "image").
                if let Some(s) = model
                    .get("architecture")
                    .and_then(|a| a.get("outputModality"))
                    .and_then(Value::as_str)
                {
                    if s.contains("image") {
                        ids.push(id);
                        continue;
                    }
                }
                // Arm 3: `supported_generation_methods` includes "image".
                if let Some(arr) = model
                    .get("supported_generation_methods")
                    .and_then(Value::as_array)
                {
                    if arr.iter().any(|m| m.as_str() == Some("image")) {
                        ids.push(id);
                        continue;
                    }
                }
            }
            Ok(ModelsPage {
                ids,
                // The SDK's page loop stops on a short page; v4's catalogue rows
                // arrive well under the 500-row limit and the recorded corpus
                // pins the one-request shape. Paging here would need the SDK's
                // offset arithmetic (`pricing_fetcher::openrouter_next_page_offset`).
                next_page_token: None,
            })
        }
        other => Err(ImageGenError::new(format!(
            "Unknown image provider: {other}"
        ))),
    }
}

/// `response.data.map(m => m.id).filter(pred)` for the two OpenAI-SDK providers.
fn openai_like_ids(body: &str, pred: impl Fn(&str) -> bool) -> Vec<String> {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    v.get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
        .filter(|id| pred(id))
        .map(str::to_string)
        .collect()
}

/// `@openrouter/sdk`'s `Model$inboundSchema` as a projection: a zod `z.object`
/// **STRIPS** every key it does not declare, then `.transform(remap$)` renames
/// the snake_case survivors to camelCase.
///
/// **This is a v4 bug, faithfully reproduced.** `OpenRouterImageProvider.
/// getAvailableModels` reads three WIRE key names off the already-transformed
/// object: model-level `output_modalities` / `outputModalities`,
/// `architecture.outputModality` (SINGULAR), and
/// `supported_generation_methods`. None of the three survives: the first and
/// third are not in the schema at all, and the architecture's genuine
/// `output_modalities` is remapped to `outputModalities` (PLURAL), which v4
/// never reads. So at `d5830439` v4's OpenRouter image discovery answers
/// **nothing** — every keyed call throws "OpenRouter listed no image-output
/// models for this API key" and the route falls back to `builtin`. Measured at
/// the pin with a payload carrying all four signals; the
/// `openrouter/models_live_every_signal` corpus row is the tripwire, and it goes
/// red the moment v4 fixes the read.
///
/// This is the same class as the P4.D33 bank note and dogfood #24 — an
/// SDK-synthesized shape is not the wire shape. `pricing_fetcher::
/// remap_openrouter_sdk_models` reproduces the rename for the pricing path but
/// deliberately PRESERVES unknown keys (nothing downstream reads them there);
/// here the stripping is exactly what decides the answer, so it is reproduced.
fn openrouter_sdk_project(model: &Value) -> Value {
    /// The declared keys of `Model$inboundSchema` (everything else is stripped).
    const MODEL_KEYS: &[(&str, &str)] = &[
        ("alias_target", "aliasTarget"),
        ("architecture", "architecture"),
        ("benchmarks", "benchmarks"),
        ("canonical_slug", "canonicalSlug"),
        ("context_length", "contextLength"),
        ("created", "created"),
        ("default_parameters", "defaultParameters"),
        ("description", "description"),
        ("expiration_date", "expirationDate"),
        ("hugging_face_id", "huggingFaceId"),
        ("id", "id"),
        ("knowledge_cutoff", "knowledgeCutoff"),
        ("links", "links"),
        ("name", "name"),
        ("per_request_limits", "perRequestLimits"),
        ("pricing", "pricing"),
        ("reasoning", "reasoning"),
        ("supported_parameters", "supportedParameters"),
        ("supported_voices", "supportedVoices"),
        ("top_provider", "topProvider"),
    ];
    /// The declared keys of `ModelArchitecture$inboundSchema`.
    const ARCH_KEYS: &[(&str, &str)] = &[
        ("input_modalities", "inputModalities"),
        ("instruct_type", "instructType"),
        ("modality", "modality"),
        ("output_modalities", "outputModalities"),
        ("tokenizer", "tokenizer"),
    ];
    fn project(v: &Value, keys: &[(&str, &str)]) -> Value {
        let Some(map) = v.as_object() else {
            return v.clone();
        };
        let mut out = Map::new();
        for (wire, sdk) in keys {
            if let Some(val) = map.get(*wire) {
                out.insert((*sdk).to_string(), val.clone());
            }
        }
        Value::Object(out)
    }
    let mut projected = project(model, MODEL_KEYS);
    if let Some(arch) = projected.get("architecture").cloned() {
        projected
            .as_object_mut()
            .unwrap()
            .insert("architecture".into(), project(&arch, ARCH_KEYS));
    }
    projected
}

/// JS `Array.prototype.sort()` — a UTF-16 code-unit comparison over the default
/// string coercion. Identical to Rust's byte order for every id in the corpus
/// (all ASCII), but spelled out so a non-BMP id could not diverge silently.
fn js_sort(ids: &mut [String]) {
    ids.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
}

/// Turn the ids collected across every page into the provider's final answer:
/// its dedup / union / sort semantics, and its empty-result disposition.
///
/// The asymmetries are v4's and are contractual — openrouter alone neither
/// sorts nor dedupes, and z-ai alone cannot come back empty.
pub fn finalize_models(provider: &str, mut ids: Vec<String>) -> Result<Vec<String>, ImageGenError> {
    match provider {
        "OPENAI" => {
            if ids.is_empty() {
                return Err(ImageGenError::new(
                    "OpenAI /v1/models listed no image-generation models for this API key",
                ));
            }
            js_sort(&mut ids);
            Ok(ids)
        }
        "GOOGLE" => {
            if ids.is_empty() {
                return Err(ImageGenError::new(
                    "Google models list contained no image-generation models for this API key",
                ));
            }
            js_sort(&mut ids);
            Ok(ids)
        }
        "GROK" => {
            // v4 accumulates into a `Set`, so `ids.size === 0` is the empty test
            // and `Array.from(set).sort()` the answer.
            let mut seen = Vec::new();
            for id in ids {
                if !seen.contains(&id) {
                    seen.push(id);
                }
            }
            if seen.is_empty() {
                return Err(ImageGenError::new(
                    "xAI listed no image-generation models for this API key",
                ));
            }
            js_sort(&mut seen);
            Ok(seen)
        }
        // P4.D101 — like Z.AI: the curated ids are UNIONED in, so the flagship
        // names always appear and the arm needs no empty-throw (the union
        // guarantees six entries). v4's chat listing is the deliberate mirror
        // image — it SUBTRACTS these same ids.
        "NANOGPT" => {
            let mut merged = Vec::new();
            for id in ids.into_iter().chain(
                supported_image_models("NANOGPT")?
                    .iter()
                    .map(|s| (*s).to_string()),
            ) {
                if !merged.contains(&id) {
                    merged.push(id);
                }
            }
            js_sort(&mut merged);
            Ok(merged)
        }
        "Z_AI" => {
            // The endpoint under-reports, so the two documented ids are UNIONED
            // in rather than trusted to appear — which is also why this arm has
            // no empty-throw: the union guarantees at least two entries.
            let mut merged = Vec::new();
            for id in ids
                .into_iter()
                .chain(ZAI_STATIC_IMAGE_MODELS.iter().map(|s| (*s).to_string()))
            {
                if !merged.contains(&id) {
                    merged.push(id);
                }
            }
            js_sort(&mut merged);
            Ok(merged)
        }
        "OPENROUTER" => {
            if ids.is_empty() {
                return Err(ImageGenError::new(
                    "OpenRouter listed no image-output models for this API key",
                ));
            }
            // NO sort and NO dedup: v4 returns the accumulated push order.
            Ok(ids)
        }
        other => Err(ImageGenError::new(format!(
            "Unknown image provider: {other}"
        ))),
    }
}

/// `URLSearchParams` value serialization (v4 builds the google page URL with
/// `url.searchParams.set`, whose `toString()` is
/// application/x-www-form-urlencoded): unreserved characters pass, a space
/// becomes `+`, everything else is percent-encoded from its UTF-8 bytes.
fn urlencoded_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ===========================================================================
// The real ImageProvider (composes build + transport + parse)
// ===========================================================================

use crate::model::image::{ImageModelDiscovery, ImageProvider};
use crate::model::image_bytes::{FetchedImageBytes, ImageBytesFetch};
use crate::model::wire::WireTransport;

/// The real [`ImageProvider`] — builds the wire request, sends it over the
/// injected [`WireTransport`], and parses the response into an [`ImageGenResponse`]
/// (closing the `generate_image` provider seam). A transport `Err` (an SDK
/// converting a non-2xx to a throw, or a network error) surfaces verbatim as the
/// [`ImageGenError`] the Concierge reroute inspects; a raw-fetch provider's HTTP
/// status is handled inside [`parse_image_response`].
pub struct RealImageProvider<T: WireTransport, B: ImageBytesFetch = NoImageBytesFetch> {
    transport: T,
    /// The `ca22ec45` image-download seam — Z.AI (`ca22ec45`) and NanoGPT
    /// (P4.D101) reach it, and only for an entry that carries a `url` and no
    /// `b64_json`.
    bytes: B,
}

impl<T: WireTransport> RealImageProvider<T, NoImageBytesFetch> {
    /// A provider with no download seam. Correct for every composition that
    /// cannot reach the Z.AI / NanoGPT URL path; if one ever does, the download
    /// fails LOUDLY by name rather than yielding an empty image.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            bytes: NoImageBytesFetch,
        }
    }
}

impl<T: WireTransport, B: ImageBytesFetch> RealImageProvider<T, B> {
    /// A provider with a live image-download seam (the host's HTTP client).
    pub fn with_bytes_fetch(transport: T, bytes: B) -> Self {
        Self { transport, bytes }
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
            "NANOGPT" => Err(ImageGenError::new("NanoGPT provider requires an API key")),
            "OPENROUTER" => Err(ImageGenError::new(
                "OpenRouter provider requires an API key",
            )),
            _ => Ok(()),
        }
    }
}

impl<T: WireTransport, B: ImageBytesFetch> ImageProvider for RealImageProvider<T, B> {
    async fn generate_image(
        &self,
        provider: &str,
        api_key: &str,
        params: &ImageGenParams,
    ) -> Result<ImageGenResponse, ImageGenError> {
        Self::require_api_key(provider, api_key)?;
        let (mut request, nanogpt_extras) = build_image_request_with_extras(provider, params)?;
        request.headers.push(Self::auth_header(provider, api_key));
        // v4 `84f33ce94`, at the provider: "Named rather than counted: when the
        // dogfood run checks whether the flat keys survived NanoGPT's legacy
        // route, this line is the record of exactly what was posted. A response
        // identical to the no-LoRA one, with these keys present in the log, is
        // the signature of a silently-dropped key."
        // v4 logs `params.model ?? 'hidream'` — the model it POSTS — on both
        // lines (`image-provider.ts:121`); an absent model reads `hidream`.
        let posted_model = model_or_default(params, "hidream");
        if let Some(extras) = &nanogpt_extras {
            tracing::debug!(
                context = "NanoGPTImageProvider.generateImage",
                model = %posted_model,
                size = ?params.size,
                n = params.n.unwrap_or(1.0),
                lora_dialect = ?extras.applied.dialect.map(|d| d.as_str()),
                lora_keys = ?extras.applied.keys,
                lora_dropped = ?extras.applied.dropped,
                passthrough_keys = ?extras.passthrough_keys,
                "Posting NanoGPT image request"
            );
        }
        // The Google dialect is selected by model on the parse side too.
        let body = request.body_string();
        let sent = match self
            .transport
            .send(&request.method, &request.url, &request.headers, &body)
            .await
        {
            // v4 generates through `new OpenAI(...)` + `client.images.generate`
            // for OPENAI / GROK / Z_AI / NANOGPT, and the SDK THROWS an
            // `APIError` on any non-2xx — carrying the API's own message — long
            // before reaching the `data` check. Their `Invalid response from …
            // Images API` sentence is reserved for a 2xx with a malformed body.
            // v5 fetches the wire itself, so the status gate has to be here.
            // Dogfood finding #104: without it a Grok 400 (`{"error":
            // "Generated image rejected by content moderation."}`) collapsed
            // into the generic sentence and the operator lost the reason.
            //
            // GOOGLE and OPENROUTER are the raw-`fetch` pair in v4 (they check
            // `response.ok` themselves and throw their own sentences), which
            // `parse_image_response` reproduces — so they must NOT come through
            // here. NanoGPT is raw-fetch only for its MODEL LISTING; its
            // generation is the SDK, which is why it belongs on this list.
            // `sdk_and_raw_fetch_providers_keep_their_own_non_2xx_sentences`
            // pins the split in both directions.
            Ok(resp)
                if !resp.ok() && matches!(provider, "OPENAI" | "GROK" | "Z_AI" | "NANOGPT") =>
            {
                Err(openai_sdk_error(&resp))
            }
            Ok(resp) => Ok(resp),
            // The SDK/transport throw (network) surfaces verbatim.
            Err(message) => Err(ImageGenError::new(message)),
        };
        // `648d5c8aa` (bug 111): the failure path needs this more than the
        // success path does. NanoGPT answers a rejected adapter, an unreachable
        // weights repo, a bad resolution and a filtered prompt with the SAME
        // generic 400 — "try a different prompt or image" — so the body that was
        // POSTED is the only thing separating those causes, and it was logged at
        // `debug`, which no packaged instance keeps. Three consecutive failures
        // were indistinguishable in the logs and had to be told apart by reading
        // the profile row out of the database.
        //
        // Key NAMES only: `lora_keys` never carries a value, which is what keeps
        // `hf_api_token` out of the log while still recording that it was sent.
        // The error is rethrown UNCHANGED.
        //
        // v4 wraps only `client.images.generate`, so this covers the transport
        // throw and the non-2xx gate above — NOT the `Invalid response from
        // NanoGPT Images API` sentence, which v4 raises AFTER its try/catch:
        // `parse_image_response` therefore runs below, outside this match (the
        // P4.D138 follow-up review caught the parse inside it — a malformed 2xx
        // logged a "request failed" v4 never logs; `nanogpt_malformed_2xx_
        // does_not_log_the_failure_line` pins the split).
        let resp = match sent {
            Ok(v) => v,
            Err(error) => {
                if let Some(extras) = &nanogpt_extras {
                    tracing::error!(
                        context = "NanoGPTImageProvider.generateImage",
                        model = %posted_model,
                        size = ?params.size,
                        // v4 logs `requestParams.n`, i.e. the RESOLVED count.
                        n = params.n.unwrap_or(1.0),
                        lora_dialect = ?extras.applied.dialect.map(|d| d.as_str()),
                        lora_keys = ?extras.applied.keys,
                        lora_dropped = ?extras.applied.dropped,
                        passthrough_keys = ?extras.passthrough_keys,
                        error = %error.message,
                        "NanoGPT image request failed"
                    );
                }
                return Err(error);
            }
        };
        let parsed = parse_image_response(provider, params, &resp)?;
        // `ca22ec45`: Z.AI returns URLs (valid ~30 days), not base64 — but every
        // Quilltap consumer (chat handler, avatar/background jobs) reads only
        // base64 `data`. Download each image here so the response is usable.
        // `ca22ec45` (Z.AI) and P4.D101 (NanoGPT): both providers can answer
        // with URLs instead of base64, and both documented the download in the
        // plugin. Identical loop, different sentences.
        if provider == "Z_AI" || provider == "NANOGPT" {
            return self.download_url_images(parsed, provider).await;
        }
        Ok(parsed)
    }
}

impl<T: WireTransport, B: ImageBytesFetch> RealImageProvider<T, B> {
    /// v4's per-image loop inside the Z.AI and NanoGPT providers'
    /// `generateImage`: keep the base64 when it is already there; otherwise
    /// download the URL and encode the bytes; reject an entry that carries
    /// neither.
    ///
    /// The mime-type rule is v4's exactly: the default is `image/png`, and the
    /// response's `content-type` overrides it ONLY when it starts with
    /// `image/`, truncated at the first `;`.
    ///
    /// P4.D101 generalized this from P4.D100's `download_zai_images`. The two
    /// plugins carry the SAME loop and differ only in their two error
    /// sentences, so the provider selects the wording and nothing else — the
    /// Z.AI path is byte-for-byte what P4.D100 landed, which its own corpus
    /// rows keep proving.
    async fn download_url_images(
        &self,
        parsed: ImageGenResponse,
        provider: &str,
    ) -> Result<ImageGenResponse, ImageGenError> {
        let (download_failed, carried_neither) = match provider {
            "NANOGPT" => (
                "Failed to download NanoGPT image: HTTP ",
                "NanoGPT image entry carried neither base64 data nor a URL",
            ),
            _ => (
                "Failed to download Z.AI image: HTTP ",
                "Z.AI image entry carried neither base64 data nor a URL",
            ),
        };
        use base64::Engine as _;
        let mut images = Vec::with_capacity(parsed.images.len());
        for img in parsed.images {
            // `let data = img.b64_json; let mimeType = 'image/png';`
            let mut data = img.data.filter(|d| !d.is_empty());
            let mut mime_type = "image/png".to_string();
            if data.is_none() {
                if let Some(url) = img.url.as_deref().filter(|u| !u.is_empty()) {
                    let resp = self.bytes.fetch(url).await.map_err(ImageGenError::new)?;
                    if !resp.ok() {
                        return Err(ImageGenError::new(format!(
                            "{download_failed}{}",
                            resp.status
                        )));
                    }
                    if let Some(ct) = resp.content_type.as_deref() {
                        if ct.starts_with("image/") {
                            // `contentType.split(';')[0]` — no trimming: v4
                            // takes the raw first segment.
                            mime_type = ct.split(';').next().unwrap_or(ct).to_string();
                        }
                    }
                    data = Some(base64::engine::general_purpose::STANDARD.encode(&resp.bytes));
                }
            }
            let Some(data) = data else {
                return Err(ImageGenError::new(carried_neither));
            };
            images.push(GeneratedImageData {
                data: Some(data),
                url: img.url,
                mime_type: Some(mime_type),
                revised_prompt: img.revised_prompt,
            });
        }
        Ok(ImageGenResponse { images })
    }
}

/// The no-download-seam default for [`RealImageProvider::new`]. A composition
/// that never reaches the Z.AI / NanoGPT URL path never calls it; one that does
/// gets a named failure instead of a silently empty image.
pub struct NoImageBytesFetch;

impl ImageBytesFetch for NoImageBytesFetch {
    async fn fetch(&self, _url: &str) -> Result<FetchedImageBytes, String> {
        Err("no image-download seam is wired on this host".to_string())
    }
}

/// The maximum number of model-list pages a single discovery call will request.
/// Only google pages at all, and its `pageSize=1000` means one page covers the
/// whole catalogue several times over; the bound exists so a provider echoing
/// its own `nextPageToken` cannot spin forever.
const MODEL_PAGE_LIMIT: usize = 32;

impl<T: WireTransport, B: ImageBytesFetch> ImageModelDiscovery for RealImageProvider<T, B> {
    /// v4 `getAvailableModels(apiKey?)`, composed: no key → the curated static
    /// list with NO request; otherwise page through the provider's model list,
    /// then apply its dedup / union / sort / empty-throw semantics.
    async fn available_models(
        &self,
        provider: &str,
        api_key: Option<&str>,
    ) -> Result<Vec<String>, ImageGenError> {
        let Some(api_key) = api_key.filter(|k| !k.is_empty()) else {
            // `return [...this.supportedModels]` — v4 makes no call at all.
            return Ok(supported_image_models(provider)?
                .iter()
                .map(|s| (*s).to_string())
                .collect());
        };
        let mut collected: Vec<String> = Vec::new();
        let mut page_token: Option<String> = None;
        for _ in 0..MODEL_PAGE_LIMIT {
            let request = build_models_request(provider, api_key, page_token.as_deref())?;
            // A GET carries no body; `Value::Null` must not become the literal
            // four bytes `null` on the wire.
            let body = if request.body.is_null() {
                String::new()
            } else {
                request.body_string()
            };
            let resp = self
                .transport
                .send(&request.method, &request.url, &request.headers, &body)
                .await
                .map_err(ImageGenError::new)?;
            // v4 fills the detailed-catalog cache inside `fetchDetailedCatalog`
            // — the IO method, not the parser — because the synchronous
            // options-schema hook gets no API key and can only ever READ it.
            // v5 keeps `parse_models_page` sans-IO, so the same write lands
            // here, on the same body, before the parse. Only NanoGPT's listing
            // is `?detailed=true`; no other provider has a catalog to remember.
            if provider == "NANOGPT" && resp.ok() {
                crate::model::nanogpt_catalog::remember_detailed_catalog(&resp.body);
            }
            let page = parse_models_page(provider, &resp)?;
            collected.extend(page.ids);
            match page.next_page_token {
                Some(token) => page_token = Some(token),
                None => return finalize_models(provider, collected),
            }
        }
        Err(ImageGenError::new(format!(
            "{provider} model list did not terminate after {MODEL_PAGE_LIMIT} pages"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::image::ImageGenParams;
    use crate::model::wire::CannedWireTransport;

    fn params(model: &str) -> ImageGenParams {
        ImageGenParams {
            prompt: "a cat".into(),
            model: model.into(),
            n: Some(1.0),
            ..Default::default()
        }
    }

    // ── Dogfood finding #104: an image API's own error must reach the
    // operator. v4 calls OPENAI / GROK / Z_AI through the OpenAI SDK, which
    // THROWS on any non-2xx with the API's message; v5 fetches the wire itself
    // and used to hand a 400 body straight to the parser, which found no
    // `data` key and answered the generic "Invalid response from … Images API".
    // The table below was measured against the REAL SDK (a stub server per
    // case), not transcribed. ──

    /// The four `APIError` messages the real SDK produces, measured
    /// 2026-08-25 by driving `client.images.generate` against a stub server.
    #[test]
    fn sdk_error_reconstruction_matches_the_measured_table() {
        let cases: &[(u16, &str, &str)] = &[
            // A STRING error — Grok's moderation refusal, the live shape that
            // found this. The SDK `JSON.stringify`s it, quotes included.
            (
                400,
                r#"{"error":"Generated image rejected by content moderation."}"#,
                r#"400 "Generated image rejected by content moderation.""#,
            ),
            // An object error with a string message — the plain message.
            (
                400,
                r#"{"error":{"message":"Invalid model","type":"invalid_request_error"}}"#,
                "400 Invalid model",
            ),
            (
                401,
                r#"{"error":{"message":"Incorrect API key provided: sk-xxx"}}"#,
                "401 Incorrect API key provided: sk-xxx",
            ),
            // No `error` key at all (and not even JSON) — the raw body.
            (500, "service unavailable", "500 service unavailable"),
        ];
        for (status, body, want) in cases {
            let got = openai_sdk_error(&WireResponse::new(*status, *body));
            assert_eq!(&got.message, want, "body {body}");
        }
    }

    /// The gate itself: a non-2xx from an SDK provider never reaches the parser.
    #[tokio::test]
    async fn a_non_2xx_from_an_sdk_provider_carries_the_api_message() {
        for provider in ["OPENAI", "GROK", "Z_AI", "NANOGPT"] {
            let p = params(match provider {
                "OPENAI" => "gpt-image-1",
                "GROK" => "grok-imagine-image",
                "NANOGPT" => "chroma",
                _ => "cogview-4-250304",
            });
            let request = build_image_request(provider, &p).unwrap();
            let wire = CannedWireTransport::new().with_response(
                &request.method,
                &request.url,
                &request.body_string(),
                WireResponse::new(
                    400,
                    r#"{"error":"Generated image rejected by content moderation."}"#,
                ),
            );
            let err = RealImageProvider::new(wire)
                .generate_image(provider, "k", &p)
                .await
                .expect_err("a 400 must be an error");
            assert_eq!(
                err.message, r#"400 "Generated image rejected by content moderation.""#,
                "{provider} should surface the API's own message"
            );
            assert!(
                !err.message.contains("Invalid response from"),
                "{provider} must not fall through to the malformed-body sentence: {}",
                err.message
            );
        }
    }

    /// **The consequence pin.** The reconstructed message is not just display
    /// text: the Concierge's post-hoc reroute decides whether to retry on the
    /// uncensored image profile by KEYWORD-MATCHING it
    /// (`is_image_moderation_error`). While a Grok 400 collapsed into
    /// `Invalid response from Grok Images API`, nothing matched, the reroute
    /// never fired, and AUTO_ROUTE image generation was dead for all four
    /// SDK-backed providers — measured live on 2026-08-25, where the same job
    /// went FAILED before this fix and COMPLETED after it, with a second
    /// `IMAGE_GENERATION` row on NANOGPT/chroma reading
    /// `Generated 1 image(s) (Concierge reroute)`.
    ///
    /// So: any future change to `openai_sdk_error`'s wording has to keep the
    /// provider's own words in the message, or it silently switches the reroute
    /// off again. This test is what makes that loud.
    #[test]
    fn a_moderation_400_still_reads_as_a_moderation_error_downstream() {
        use crate::services::dangerous_content::provider_routing::is_image_moderation_error;

        let grok_400 = openai_sdk_error(&WireResponse::new(
            400,
            r#"{"error":"Generated image rejected by content moderation."}"#,
        ));
        assert!(
            is_image_moderation_error(&grok_400.message),
            "the reroute must still recognise this: {}",
            grok_400.message
        );

        // The pre-fix message is the counter-example that explains the bug.
        assert!(
            !is_image_moderation_error("Invalid response from Grok Images API"),
            "the generic sentence never matched — which is why the reroute died"
        );
    }

    /// The SDK/raw-fetch SPLIT, pinned in BOTH directions. Widening the gate
    /// to every provider compiles, reads like a simplification, and silently
    /// replaces GOOGLE's and OPENROUTER's own v4 sentences — a mutation that
    /// stayed green until this test existed.
    #[tokio::test]
    async fn sdk_and_raw_fetch_providers_keep_their_own_non_2xx_sentences() {
        // (provider, model, the sentence a 502 must produce)
        let cases: &[(&str, &str, &str)] = &[
            // The raw-`fetch` pair: v4 checks `response.ok` in the plugin and
            // throws its own wording, which `parse_image_response` reproduces.
            ("GOOGLE", "gemini-2.5-flash-image", "Gemini API error: 502"),
            ("GOOGLE", "imagen-4", "Google Imagen API error: 502"),
            (
                "OPENROUTER",
                "google/gemini-2.5-flash-image",
                "OpenRouter API error: 502 - upstream exploded",
            ),
            // The SDK four: the reconstructed `APIError`.
            ("OPENAI", "gpt-image-1", "502 upstream exploded"),
            ("GROK", "grok-imagine-image", "502 upstream exploded"),
            ("Z_AI", "cogview-4-250304", "502 upstream exploded"),
            ("NANOGPT", "chroma", "502 upstream exploded"),
        ];
        for (provider, model, want) in cases {
            let p = params(model);
            let request = build_image_request(provider, &p).unwrap();
            let wire = CannedWireTransport::new().with_response(
                &request.method,
                &request.url,
                &request.body_string(),
                WireResponse::new(502, "upstream exploded"),
            );
            let err = RealImageProvider::new(wire)
                .generate_image(provider, "k", &p)
                .await
                .expect_err("a 502 must be an error");
            assert_eq!(&err.message, want, "{provider} / {model}");
        }
    }

    /// The other half of v4's contract: that sentence is still what a **2xx**
    /// with a malformed body answers, so the gate cannot swallow it.
    #[tokio::test]
    async fn a_2xx_without_a_data_array_still_says_invalid_response() {
        let p = params("grok-imagine-image");
        let request = build_image_request("GROK", &p).unwrap();
        let wire = CannedWireTransport::new().with_response(
            &request.method,
            &request.url,
            &request.body_string(),
            WireResponse::new(200, r#"{"unexpected":true}"#),
        );
        let err = RealImageProvider::new(wire)
            .generate_image("GROK", "k", &p)
            .await
            .expect_err("a malformed 2xx body must be an error");
        assert_eq!(err.message, "Invalid response from Grok Images API");
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

    // ── `d8d2890ee`: the dropped-parameter log lines. The corpus proves the
    // DROP (the body bytes); these prove the SENTENCE and its bag, and each
    // carries its silence leg — a line that fires on the accepting arm too is
    // noise the operator learns to ignore. ──

    /// Build with a profile bag, the shape every GPT Image extras row uses.
    fn params_with_bag(model: &str, bag: serde_json::Value) -> ImageGenParams {
        let mut p = params(model);
        p.profile_parameters = Some(bag);
        p
    }

    fn lines_for(p: &ImageGenParams) -> Vec<String> {
        crate::test_support::captured(|| {
            build_openai(p);
        })
    }

    fn one(lines: &[String], needle: &str) -> String {
        let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one {needle:?} in {lines:?}"
        );
        hits[0].clone()
    }

    fn none(lines: &[String], needle: &str) {
        assert!(
            !lines.iter().any(|l| l.contains(needle)),
            "unexpected {needle:?} in {lines:?}"
        );
    }

    #[test]
    fn size_fallback_warns_with_the_rule_it_broke() {
        let mut p = params("gpt-image-2.5-sunburst");
        p.size = Some("1000x1000".into());
        let lines = lines_for(&p);
        let line = one(
            &lines,
            "Falling back to 1024x1024: unsupported OpenAI image size",
        );
        assert!(line.starts_with("WARN"), "{line}");
        assert!(
            line.contains("context=OpenAIImageProvider.validateAndNormalizeSize"),
            "{line}"
        );
        assert!(line.contains("model=gpt-image-2.5-sunburst"), "{line}");
        assert!(line.contains("size=1000x1000"), "{line}");
        assert!(
            line.contains("reason=both edges must be divisible by 16"),
            "{line}"
        );

        // The non-arbitrary family takes the OTHER sentence, naming its list.
        let mut q = params("gpt-image-1.5");
        q.size = Some("1536x864".into());
        let qlines = lines_for(&q);
        let qline = one(
            &qlines,
            "Falling back to 1024x1024: size not supported by this OpenAI model",
        );
        assert!(
            qline.contains("supported=auto, 1024x1024, 1536x1024, 1024x1536"),
            "{qline}"
        );

        // Silence: an accepted size, and the truthy-falsy empty string that
        // returns before either sentence.
        let mut ok = params("gpt-image-2.5-sunburst");
        ok.size = Some("1536x864".into());
        none(&lines_for(&ok), "Falling back to 1024x1024");
        let mut blank = params("gpt-image-2.5-sunburst");
        blank.size = Some(String::new());
        none(&lines_for(&blank), "Falling back to 1024x1024");
    }

    /// Only an ARBITRARY size can reach this line: `caps.sizes.includes(size)`
    /// returns first, so the picker's own `3840x2160` is never called
    /// experimental even though it clears the threshold. The measured shape,
    /// not the obvious one.
    #[test]
    fn an_experimental_resolution_is_a_debug_line_not_a_refusal() {
        let mut p = params("gpt-image-2.5-flare");
        // 3840×1280 = 4,915,200 px: past the threshold, edges /16, aspect
        // exactly 3:1, and NOT in the picker list.
        p.size = Some("3840x1280".into());
        let lines = lines_for(&p);
        let line = one(&lines, "Requesting an experimental OpenAI image resolution");
        assert!(line.starts_with("DEBUG"), "{line}");
        assert!(line.contains("size=3840x1280"), "{line}");
        // …and the size still rides.
        let (_, body) = build_openai(&p);
        assert_eq!(body["size"], "3840x1280");

        // Silence at exactly the threshold (`>`, not `>=`): 2560×1440 as an
        // ARBITRARY size on a family whose picker does not list it.
        let mut edge = params("gpt-image-2");
        edge.size = Some("1920x1920".into()); // 3,686,400 — exactly the budget.
        none(
            &lines_for(&edge),
            "Requesting an experimental OpenAI image resolution",
        );
        // …and for a listed size, which returns before the check runs at all.
        let mut listed = params("gpt-image-2.5-flare");
        listed.size = Some("3840x2160".into());
        none(
            &lines_for(&listed),
            "Requesting an experimental OpenAI image resolution",
        );
    }

    #[test]
    fn an_unsupported_quality_tier_warns_and_falls_back() {
        let mut p = params("gpt-image-2");
        p.quality = Some("xhigh".into());
        let lines = lines_for(&p);
        let line = one(
            &lines,
            "Ignoring quality tier unsupported by this OpenAI model",
        );
        assert!(line.starts_with("WARN"), "{line}");
        assert!(
            line.contains("context=OpenAIImageProvider.resolveQuality"),
            "{line}"
        );
        assert!(line.contains("quality=xhigh"), "{line}");
        assert!(line.contains("supported=auto, low, medium, high"), "{line}");

        // Silence on a tier the family does offer, and on an unknown model
        // (whose quality is forwarded rather than judged).
        let mut ok = params("gpt-image-2");
        ok.quality = Some("high".into());
        none(&lines_for(&ok), "Ignoring quality tier");
        let mut unknown = params("gpt-image-3-supernova");
        unknown.quality = Some("ludicrous".into());
        none(&lines_for(&unknown), "Ignoring quality tier");
    }

    #[test]
    fn a_bad_enum_in_the_bag_warns_with_the_js_string_of_the_value() {
        let lines = lines_for(&params_with_bag(
            "gpt-image-2",
            serde_json::json!({ "background": "chartreuse" }),
        ));
        let line = one(&lines, "Ignoring unsupported OpenAI image parameter value");
        assert!(line.starts_with("WARN"), "{line}");
        assert!(
            line.contains("context=OpenAIImageProvider.readEnum"),
            "{line}"
        );
        assert!(line.contains("key=background"), "{line}");
        assert!(line.contains("value=chartreuse"), "{line}");
        assert!(line.contains("allowed=auto, transparent, opaque"), "{line}");

        // `String(raw)` over a non-string: an object is `[object Object]`, an
        // array its join — v4's own coercion, not a Rust `Debug`.
        let objs = lines_for(&params_with_bag(
            "gpt-image-2",
            serde_json::json!({ "background": {}, "output_format": ["webp"] }),
        ));
        assert!(
            one(&objs, "key=background").contains("value=[object Object]"),
            "{objs:?}"
        );
        assert!(
            one(&objs, "key=output_format").contains("value=webp"),
            "{objs:?}"
        );

        // Silence: an accepted value, and the three "unset" spellings.
        none(
            &lines_for(&params_with_bag(
                "gpt-image-2",
                serde_json::json!({ "background": "auto" }),
            )),
            "Ignoring unsupported OpenAI image parameter value",
        );
        none(
            &lines_for(&params_with_bag(
                "gpt-image-2",
                serde_json::json!({ "background": "", "moderation": null }),
            )),
            "Ignoring unsupported OpenAI image parameter value",
        );
    }

    /// A bad bag value warns ONCE per key per request, as v4's single read in
    /// `generateImage` does — the parser's MIME re-derivation reads the same
    /// keys again and must stay silent. Before `read_enum_quiet`, this counted
    /// four (two from the build, two more from the parse) where v4 logs two.
    #[test]
    fn a_bad_bag_value_warns_once_across_build_and_parse() {
        let p = params_with_bag(
            "gpt-image-2",
            serde_json::json!({ "output_format": "tiff", "background": "chartreuse" }),
        );
        let lines = crate::test_support::captured(|| {
            build_openai(&p);
            let resp = WireResponse {
                status: 200,
                status_text: "OK".into(),
                body: r#"{"data":[{"b64_json":"aGVsbG8="}]}"#.into(),
            };
            let parsed = parse_image_response("OPENAI", &p, &resp).expect("parsed");
            // Both bad keys dropped, so the format is the png default.
            assert_eq!(parsed.images[0].mime_type.as_deref(), Some("image/png"));
        });
        let hits = lines
            .iter()
            .filter(|l| l.contains("Ignoring unsupported OpenAI image parameter value"))
            .count();
        assert_eq!(
            hits, 2,
            "one warning per bad key from the build and NONE from the parse: {lines:?}"
        );
    }

    #[test]
    fn an_out_of_range_compression_warns_with_its_bounds() {
        let lines = lines_for(&params_with_bag(
            "gpt-image-2",
            serde_json::json!({ "output_format": "webp", "output_compression": 300 }),
        ));
        let line = one(&lines, "Ignoring out-of-range OpenAI image parameter");
        assert!(line.starts_with("WARN"), "{line}");
        assert!(
            line.contains("context=OpenAIImageProvider.readIntInRange"),
            "{line}"
        );
        assert!(line.contains("key=output_compression"), "{line}");
        assert!(line.contains("value=300"), "{line}");
        assert!(line.contains("min=0"), "{line}");
        assert!(line.contains("max=100"), "{line}");

        // A fractional value fails `Number.isInteger`, not the bounds.
        let frac = lines_for(&params_with_bag(
            "gpt-image-2",
            serde_json::json!({ "output_format": "webp", "output_compression": 50.5 }),
        ));
        assert!(
            one(&frac, "Ignoring out-of-range OpenAI image parameter").contains("value=50.5"),
            "{frac:?}"
        );

        // Silence: `Number('80')` is 80, and 0 is in range (the `!== undefined`
        // gate is what keeps a falsy zero).
        for bag in [
            serde_json::json!({ "output_format": "webp", "output_compression": "80" }),
            serde_json::json!({ "output_format": "webp", "output_compression": 0 }),
            serde_json::json!({ "output_format": "webp", "output_compression": "" }),
        ] {
            none(
                &lines_for(&params_with_bag("gpt-image-2", bag)),
                "Ignoring out-of-range OpenAI image parameter",
            );
        }
    }

    #[test]
    fn capping_the_image_count_warns_once_with_both_numbers() {
        let mut p = params("dall-e-3");
        p.n = Some(4.0);
        let lines = lines_for(&p);
        let line = one(&lines, "Capping image count to the model maximum");
        assert!(line.starts_with("WARN"), "{line}");
        assert!(line.contains("model=dall-e-3"), "{line}");
        assert!(line.contains("requested=4"), "{line}");
        assert!(line.contains("capped=1"), "{line}");

        // `Math.max(requested, 1)` raises a zero, which is also a change.
        let mut zero = params("gpt-image-2");
        zero.n = Some(0.0);
        assert!(
            one(&lines_for(&zero), "Capping image count").contains("capped=1"),
            "the zero arm"
        );

        // Silence: within the cap, and on an unknown model (uncapped).
        let mut ok = params("gpt-image-2.5-sunburst");
        ok.n = Some(4.0);
        none(&lines_for(&ok), "Capping image count");
        let mut unknown = params("gpt-image-3-supernova");
        unknown.n = Some(50.0);
        none(&lines_for(&unknown), "Capping image count");
    }

    #[test]
    fn forcing_png_for_a_transparent_background_warns_and_moves_the_mime() {
        let p = params_with_bag(
            "gpt-image-2.5-flare",
            serde_json::json!({ "background": "transparent", "output_format": "jpeg" }),
        );
        let lines = lines_for(&p);
        let line = one(
            &lines,
            "Forcing PNG output: a transparent background needs png or webp",
        );
        assert!(line.starts_with("WARN"), "{line}");
        assert!(line.contains("requestedFormat=jpeg"), "{line}");
        let (_, body) = build_openai(&p);
        assert_eq!(body["output_format"], "png");
        // The parse follows the FORCED format, which is the whole point.
        assert_eq!(openai_output_format(&p).as_deref(), Some("png"));

        // Silence: webp is already alpha-capable, and a background with no
        // format at all cannot be forced (`outputFormat !== undefined`).
        for bag in [
            serde_json::json!({ "background": "transparent", "output_format": "webp" }),
            serde_json::json!({ "background": "transparent" }),
            serde_json::json!({ "background": "opaque", "output_format": "jpeg" }),
        ] {
            none(
                &lines_for(&params_with_bag("gpt-image-2", bag)),
                "Forcing PNG output",
            );
        }
    }

    #[test]
    fn dropping_compression_for_png_is_a_debug_line_naming_the_format() {
        let lines = lines_for(&params_with_bag(
            "gpt-image-2",
            serde_json::json!({ "output_format": "png", "output_compression": 50 }),
        ));
        let line = one(
            &lines,
            "Dropping output_compression: it applies only to jpeg and webp",
        );
        assert!(line.starts_with("DEBUG"), "{line}");
        assert!(line.contains("outputFormat=png"), "{line}");

        // With no format at all v4 renders `png (default)`.
        let bare = lines_for(&params_with_bag(
            "gpt-image-2",
            serde_json::json!({ "output_compression": 50 }),
        ));
        assert!(
            one(&bare, "Dropping output_compression").contains("outputFormat=png (default)"),
            "{bare:?}"
        );

        // Silence: jpeg and webp take it.
        for f in ["jpeg", "webp"] {
            none(
                &lines_for(&params_with_bag(
                    "gpt-image-2",
                    serde_json::json!({ "output_format": f, "output_compression": 50 }),
                )),
                "Dropping output_compression",
            );
        }
    }

    #[test]
    fn the_calling_line_renders_every_absent_key_as_the_model_default() {
        let lines = lines_for(&params("gpt-image-2"));
        let line = one(&lines, "Calling OpenAI Images API");
        assert!(line.starts_with("DEBUG"), "{line}");
        for f in [
            "model=gpt-image-2",
            "size=1024x1024",
            "quality=(model default)",
            "background=(model default)",
            "outputFormat=(model default)",
            "outputCompression=(model default)",
            "moderation=(model default)",
            "n=1",
        ] {
            assert!(line.contains(f), "missing {f} in {line}");
        }

        // …and the assembled values when they are there. `outputCompression`
        // is a NUMBER on the bag, so it renders unquoted through `String()`.
        let full = lines_for(&params_with_bag(
            "gpt-image-2.5-sunburst",
            serde_json::json!({
                "background": "transparent",
                "output_format": "webp",
                "output_compression": 80,
                "moderation": "low"
            }),
        ));
        let line = one(&full, "Calling OpenAI Images API");
        for f in [
            "background=transparent",
            "outputFormat=webp",
            "outputCompression=80",
            "moderation=low",
        ] {
            assert!(line.contains(f), "missing {f} in {line}");
        }
    }

    /// The body's KEY ORDER is v4's assignment order, and an unrecognised model
    /// is forwarded untouched — size, quality and `n` all uncapped, plus the
    /// `response_format`/`style` a bare non-`gpt-image-` prefix still earns.
    ///
    /// Fast, no corpus: a table edit reddens here before anyone runs a regen.
    #[test]
    fn body_key_order_and_the_unknown_model_passthrough() {
        let mut p = params_with_bag(
            "gpt-image-2.5-sunburst",
            serde_json::json!({
                "background": "transparent",
                "output_format": "webp",
                "output_compression": 80,
                "moderation": "low"
            }),
        );
        p.size = Some("1536x864".into());
        p.quality = Some("max".into());
        let (_, body) = build_openai(&p);
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"model":"gpt-image-2.5-sunburst","prompt":"a cat","n":1,"size":"1536x864","quality":"max","background":"transparent","output_format":"webp","output_compression":80,"moderation":"low"}"#
        );

        // An unknown `gpt-image-*` id: no caps, so everything rides and the
        // bare-prefix fallback still keeps `response_format` and `style` off.
        let mut u = params("gpt-image-3-supernova");
        u.size = Some("4096x4096".into());
        u.quality = Some("ludicrous".into());
        u.n = Some(50.0);
        let (_, body) = build_openai(&u);
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"model":"gpt-image-3-supernova","prompt":"a cat","n":50,"size":"4096x4096","quality":"ludicrous"}"#
        );

        // An unknown NON-gpt-image id: the same passthrough, but it DOES get
        // `response_format` and a defaulted `style`.
        let mut m = params("mystery-painter-9");
        m.size = Some("4096x4096".into());
        let (_, body) = build_openai(&m);
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"model":"mystery-painter-9","prompt":"a cat","n":1,"response_format":"b64_json","size":"4096x4096","quality":"standard","style":"vivid"}"#
        );

        // No model at all: `requestParams.model = params.model` is the RAW
        // value, so the key never reaches the wire while `?? 'dall-e-3'` drove
        // every validation.
        let mut none = params("");
        none.n = Some(1.0);
        let (_, body) = build_openai(&none);
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"prompt":"a cat","n":1,"response_format":"b64_json","size":"1024x1024","quality":"standard","style":"vivid"}"#
        );
    }

    /// The GPT Image extras never reach a DALL·E body, and the readers never
    /// even run — so a bag full of rubbish on a DALL·E profile is silent.
    #[test]
    fn a_dall_e_profile_neither_sends_nor_judges_the_gpt_image_extras() {
        let p = params_with_bag(
            "dall-e-3",
            serde_json::json!({ "background": "chartreuse", "output_format": "tiff" }),
        );
        let (_, body) = build_openai(&p);
        for k in [
            "background",
            "output_format",
            "output_compression",
            "moderation",
        ] {
            assert!(body.get(k).is_none(), "dall-e-3 body carried {k}");
        }
        none(
            &lines_for(&p),
            "Ignoring unsupported OpenAI image parameter value",
        );
        assert_eq!(openai_output_format(&p), None);
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
        let err = parse_image_response("GOOGLE", &params("imagen-4"), &resp).unwrap_err();
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
        let ok =
            parse_image_response("OPENROUTER", &params(OPENROUTER_DEFAULT_MODEL), &resp).unwrap();
        assert_eq!(ok.images[0].data.as_deref(), Some("QUJD"));
        assert_eq!(ok.images[0].mime_type.as_deref(), Some("image/png"));

        let declined = WireResponse::new(
            200,
            r#"{"choices":[{"message":{"refusal":"nope, policy"}}]}"#,
        );
        let err = parse_image_response("OPENROUTER", &params(OPENROUTER_DEFAULT_MODEL), &declined)
            .unwrap_err();
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

    /// The `ca22ec45` routing widening: ANY `gemini*` id reaches generateContent,
    /// while the pre-existing exact / prefixed / substring arms still catch the
    /// curated ids. `imagen-*` (and anything else) still routes to `predict`.
    #[test]
    fn gemini_routing_covers_live_fetched_ids() {
        for m in [
            "gemini",
            "gemini-2.0-flash-preview-image-generation",
            "gemini-2.5-flash-image",
            "gemini-3-pro-image-preview",
        ] {
            assert!(is_gemini_image_model(m), "{m} should route to gemini");
        }
        // The preserved non-`gemini`-prefixed arms (substring / suffixed).
        assert!(is_gemini_image_model("models/gemini-2.5-flash-image"));
        assert!(is_gemini_image_model("gemini-2.5-flash-image-001"));
        // Imagen and unrelated ids keep the predict route.
        for m in [
            "imagen-4",
            "imagen-4-fast",
            "imagen-4.0-generate-001",
            "veo-3",
        ] {
            assert!(!is_gemini_image_model(m), "{m} should route to imagen");
        }
    }

    /// **A blinded comparand, named.** OpenRouter alone neither sorts nor dedupes
    /// its discovered ids — but the corpus cannot see that, because the SDK's
    /// zod strips every key v4's discovery reads (see `openrouter_sdk_project`),
    /// so every keyed OpenRouter row throws and `finalize_models` never gets a
    /// non-empty list to order. The rule is transcribed from v4's source and
    /// pinned here directly; the `openrouter/models_live_every_signal` corpus
    /// row is the tripwire that fires when v4 fixes the read, at which point
    /// this arm becomes oracle-observable and this test becomes redundant.
    #[test]
    fn openrouter_finalize_neither_sorts_nor_dedupes() {
        let ids = vec![
            "z/last".to_string(),
            "a/first".to_string(),
            "z/last".to_string(),
        ];
        assert_eq!(
            finalize_models("OPENROUTER", ids.clone()).unwrap(),
            ids,
            "v4 returns the accumulated push order verbatim"
        );
        assert_eq!(
            finalize_models("OPENROUTER", vec![]).unwrap_err().message,
            "OpenRouter listed no image-output models for this API key"
        );
    }

    /// The SDK projection: unknown model-level keys are STRIPPED, declared
    /// snake_case keys are RENAMED, and the architecture's `output_modalities`
    /// becomes `outputModalities` (plural) — which is exactly why v4's
    /// `architecture?.outputModality` (singular) read never fires.
    #[test]
    fn openrouter_sdk_projection_strips_and_renames() {
        let raw = serde_json::json!({
            "id": "x/y",
            "context_length": 8192,
            "output_modalities": ["image"],
            "supported_generation_methods": ["image"],
            "architecture": {
                "modality": "text->image",
                "output_modalities": ["image"],
                "outputModality": "text+image"
            }
        });
        let p = super::openrouter_sdk_project(&raw);
        // Survives, renamed.
        assert_eq!(p["id"], serde_json::json!("x/y"));
        assert_eq!(p["contextLength"], serde_json::json!(8192));
        assert_eq!(
            p["architecture"]["outputModalities"],
            serde_json::json!(["image"])
        );
        // Stripped — the three reads v4's discovery makes.
        assert!(p.get("output_modalities").is_none());
        assert!(p.get("outputModalities").is_none());
        assert!(p.get("supported_generation_methods").is_none());
        assert!(p["architecture"].get("outputModality").is_none());
        assert!(p["architecture"].get("output_modalities").is_none());
    }

    /// A model-list GET carries no body and pages only for google, whose page
    /// token is form-urlencoded onto the existing `pageSize` query.
    #[test]
    fn model_list_requests_are_bodyless_and_google_pages() {
        for p in ["OPENAI", "GOOGLE", "GROK", "Z_AI", "OPENROUTER"] {
            let r = build_models_request(p, "k", None).unwrap();
            assert_eq!(r.method, "GET", "{p} method");
            assert!(r.body.is_null(), "{p} built a body");
        }
        let paged = build_models_request("GOOGLE", "k", Some("tok en/=")).unwrap();
        assert_eq!(
            paged.url,
            "https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000&pageToken=tok+en%2F%3D"
        );
        // A page token is google's alone; nothing else appends one.
        assert_eq!(
            build_models_request("OPENAI", "k", Some("tok"))
                .unwrap()
                .url,
            "https://api.openai.com/v1/models"
        );
        assert!(build_models_request("NOPE", "k", None).is_err());
        assert!(supported_image_models("NOPE").is_err());
    }

    #[test]
    fn zai_alone_cannot_come_back_empty() {
        // Union with the two static ids; no empty-throw arm exists.
        assert_eq!(
            finalize_models("Z_AI", vec![]).unwrap(),
            vec!["cogview-4-250304".to_string(), "glm-image".to_string()]
        );
        // The other four DO throw on empty, each with its own sentence.
        for (p, want) in [
            (
                "OPENAI",
                "OpenAI /v1/models listed no image-generation models for this API key",
            ),
            (
                "GOOGLE",
                "Google models list contained no image-generation models for this API key",
            ),
            (
                "GROK",
                "xAI listed no image-generation models for this API key",
            ),
            (
                "OPENROUTER",
                "OpenRouter listed no image-output models for this API key",
            ),
        ] {
            assert_eq!(finalize_models(p, vec![]).unwrap_err().message, want, "{p}");
        }
    }

    #[test]
    fn zai_keeps_url_and_b64() {
        let resp = WireResponse::new(
            200,
            r#"{"data":[{"url":"https://z.ai/img.png"},{"b64_json":"QUJD"}]}"#,
        );
        let ok = parse_image_response("Z_AI", &params("glm-image"), &resp).unwrap();
        assert_eq!(ok.images[0].data, None);
        assert_eq!(ok.images[0].url.as_deref(), Some("https://z.ai/img.png"));
        assert_eq!(ok.images[1].data.as_deref(), Some("QUJD"));
        assert_eq!(ok.images[1].url, None);
    }
}
