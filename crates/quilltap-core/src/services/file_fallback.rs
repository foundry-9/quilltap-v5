//! v4 `lib/chat/file-attachment-fallback.ts` — the provider-can't-handle-this-file
//! fallback: text files → inline text, images → an image-description (vision LLM)
//! or a reuse of the persisted generation prompt/description, and the keep-vs-drop
//! rule for provider-supported files. (Reworked by v4 `6b6e39ad`, 2026-07-07 —
//! ported from that source.)
//!
//! ## Seams
//! * The vision-description call rides the [`CompletionProvider`] seam (v4's
//!   `provider.sendMessage(params, apiKey)` via `createLLMProvider`), carrying the
//!   image as a [`crate::model::completion::CompletionAttachment`].
//! * Image RESIZE (the pre-vision downsize) goes through the injected
//!   [`ImageTranscoder`] seam (no image codec in the core).
//! * The `IMAGE_DESCRIPTION` `logLLMCall` write goes through the real
//!   [`crate::services::llm_logging::log_llm_call`] (W4.7e).
//! * The 60 s timeout is host-timing — only the timeout→error-path MAPPING is
//!   ported (the W4.3 precedent); the timer itself is skipped.
//!
//! Profiles + chat settings are read as `serde_json::Value` (the connection /
//! chat-settings net-read shape). The API key is resolved directly off the DB
//! (host-side in v4; the canned provider ignores it, so it does not affect the
//! differential).

use serde::Serialize;
use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::files::image_processing::{
    can_resize_image, resize_image_for_provider, ImageTranscoder, DEFAULT_QUALITY,
};
use crate::model::completion::{
    CompletionAttachment, CompletionMessage, CompletionParams, CompletionProvider,
};
use crate::services::llm_logging::{
    log_llm_call, log_type, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage,
    LogResponse, LogUsage,
};

/// v4 `IMAGE_DESCRIPTION_INSTRUCTION` (byte-exact).
pub const IMAGE_DESCRIPTION_INSTRUCTION: &str = "Please describe this image in great detail. Include all visible elements, colors, composition, mood, and any text or notable features. Be thorough and descriptive.";

/// v4 `IMAGE_DESCRIPTION_TIMEOUT_MS` (the timer is host-side; only the error
/// mapping is ported).
pub const IMAGE_DESCRIPTION_TIMEOUT_MS: i64 = 60_000;

/// The default vision temperature / max tokens (v4 `modelParams.temperature ??
/// 0.7`, `modelParams.max_tokens ?? 1000`).
const DEFAULT_VISION_TEMPERATURE: f64 = 0.7;
const DEFAULT_VISION_MAX_TOKENS: i64 = 1000;

// ============================================================================
// Result shapes (v4 `FallbackResult`)
// ============================================================================

/// v4 `FallbackResult.type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackType {
    Text,
    ImageDescription,
    Unsupported,
}

/// v4 `FallbackResult.processingMetadata` (serialized camelCase; optionals
/// omitted when `None`; `originalFilename`/`originalMimeType` always present).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingMetadata {
    // v4's key is `usedImageDescriptionLLM` (uppercase acronym); the default
    // camelCase rename would lowercase it to `usedImageDescriptionLlm`.
    #[serde(
        rename = "usedImageDescriptionLLM",
        skip_serializing_if = "Option::is_none"
    )]
    pub used_image_description_llm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_uncensored_fallback: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reused_persisted_description: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_model: Option<String>,
    pub original_filename: String,
    pub original_mime_type: String,
}

/// v4 `FallbackResult`. Serializes to the v4 JSON shape (for the differential's
/// `fallbackResults` compare).
#[derive(Clone, Debug, Serialize)]
pub struct FallbackResult {
    #[serde(rename = "type")]
    pub type_: FallbackType,
    #[serde(rename = "textContent", skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
    #[serde(rename = "imageDescription", skip_serializing_if = "Option::is_none")]
    pub image_description: Option<String>,
    #[serde(rename = "processingMetadata", skip_serializing_if = "Option::is_none")]
    pub processing_metadata: Option<ProcessingMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl FallbackResult {
    fn unsupported(filename: &str, mime: &str, error: Option<String>) -> Self {
        FallbackResult {
            type_: FallbackType::Unsupported,
            text_content: None,
            image_description: None,
            processing_metadata: Some(ProcessingMetadata {
                original_filename: filename.to_string(),
                original_mime_type: mime.to_string(),
                ..Default::default()
            }),
            error,
        }
    }
}

/// The file metadata + attachment the fallback consumes (v4's `file` +
/// `fileAttachment`). `data` is the base64 payload (`None` when bytes were not
/// loaded).
#[derive(Clone, Debug)]
pub struct FallbackFile {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub data: Option<String>,
}

// ============================================================================
// Pure predicates
// ============================================================================

/// v4 `profileSupportsMimeType(profile, mimeType)`. Images are gated by the
/// profile's `supportsImageUpload` flag; every other type consults the
/// client-safe [`crate::files::attachment_support`] map by provider.
pub fn profile_supports_mime_type(profile: &Value, mime_type: &str) -> bool {
    if mime_type.starts_with("image/") {
        return profile.get("supportsImageUpload").and_then(Value::as_bool) == Some(true);
    }
    let provider = profile
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("");
    crate::files::attachment_support::supports_mime_type(provider, mime_type)
}

/// v4 `needsFallbackProcessing(profile, mimeType)`.
///
/// Two questions have to answer yes before raw bytes are worth sending, and
/// bug 91 (v4 `a14a1811`) was asking only the first:
///
///  1. **Does the model read this?** — `profile_supports_mime_type`, which for
///     images is the operator's per-profile `supportsImageUpload` tick.
///  2. **Can the plugin put it on the wire?** — `provider_can_transport_images`.
///     NanoGPT (pre-1.1.0), DeepSeek and OpenAI-Compatible all inherit a base
///     that marks every attachment failed, so the answer is no however
///     vision-capable the routed model happens to be.
///
/// When (1) says yes and (2) says no, the old predicate returned `false`, which
/// suppressed the describer *and* left the bytes for a plugin that discarded
/// them: the model got nothing, and nothing said so. Now that combination
/// routes to the describe-fallback, which is exactly what it's for. The
/// `image/` prefix gate is load-bearing — non-image types are unaffected by
/// the transport check.
pub fn needs_fallback_processing(profile: &Value, mime_type: &str) -> bool {
    if !profile_supports_mime_type(profile, mime_type) {
        return true;
    }
    if mime_type.starts_with("image/") {
        let provider = profile
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !crate::files::image_transport::provider_can_transport_images(provider) {
            let profile_id = profile.get("id").and_then(Value::as_str).unwrap_or("");
            let model_name = profile
                .get("modelName")
                .and_then(Value::as_str)
                .unwrap_or("");
            tracing::info!(
                profile_id,
                provider,
                model_name,
                "[Attachment] Profile claims image support but its plugin cannot transport images; routing to describe-fallback"
            );
            return true;
        }
    }
    false
}

/// v4 `isTextFile(mimeType)`.
pub fn is_text_file(mime_type: &str) -> bool {
    mime_type.starts_with("text/")
        || mime_type == "application/json"
        || mime_type == "application/xml"
}

/// v4 `isImageFile(mimeType)`.
pub fn is_image_file(mime_type: &str) -> bool {
    mime_type.starts_with("image/")
}

/// v4 `convertTextFileToInline(file, base64Data)` — decode base64 → UTF-8 and
/// wrap in the exact markers. A decode failure returns `unsupported` with the
/// error marker (the outer catch).
pub fn convert_text_file_to_inline(
    filename: &str,
    mime_type: &str,
    base64_data: &str,
) -> FallbackResult {
    match decode_text_from_base64(base64_data) {
        Ok(content) => {
            let text_content = format!(
                "[User attached text file: {filename}]\n\n{content}\n\n[End of attached file]"
            );
            FallbackResult {
                type_: FallbackType::Text,
                text_content: Some(text_content),
                image_description: None,
                processing_metadata: Some(ProcessingMetadata {
                    original_filename: filename.to_string(),
                    original_mime_type: mime_type.to_string(),
                    ..Default::default()
                }),
                error: None,
            }
        }
        Err(msg) => FallbackResult::unsupported(
            filename,
            mime_type,
            Some(format!("Failed to process text file: {msg}")),
        ),
    }
}

/// v4 `decodeTextFromBase64` — `Buffer.from(data,'base64').toString('utf-8')`.
/// Node's base64 decode is lenient (ignores invalid chars / padding); the utf-8
/// stringify is lossy (replacement chars). Reproduced via a lenient decode +
/// lossy utf-8.
fn decode_text_from_base64(data: &str) -> Result<String, String> {
    use base64::Engine;
    // Node `Buffer.from(x,'base64')` is forgiving: it strips non-alphabet bytes
    // and tolerates missing padding. `GeneralPurpose` with `NO_PAD` +
    // `decode_allow_trailing_bits` approximates it for the corpus's clean inputs.
    let engine = base64::engine::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    match engine.decode(data.trim()) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Err(e) => Err(format!("Failed to decode text file: {e}")),
    }
}

/// v4 `formatFallbackAsMessagePrefix(result)` — the per-type prefix (byte-exact
/// markers, incl. the `⚠️` error marker). An `unsupported` result WITHOUT an
/// error (provider-supported, kept raw) yields `""`.
pub fn format_fallback_as_message_prefix(result: &FallbackResult) -> String {
    match result.type_ {
        FallbackType::Text => match &result.text_content {
            Some(tc) => format!("{tc}\n\n"),
            None => String::new(),
        },
        FallbackType::ImageDescription => match &result.image_description {
            Some(desc) => {
                let filename = result
                    .processing_metadata
                    .as_ref()
                    .map(|m| m.original_filename.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Unknown");
                format!("[Image: {filename}]\n\nImage Description (generated by AI):\n{desc}\n\n")
            }
            None => String::new(),
        },
        FallbackType::Unsupported => match &result.error {
            Some(err) => {
                let filename = result
                    .processing_metadata
                    .as_ref()
                    .map(|m| m.original_filename.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Unknown file");
                format!("⚠️ Attachment Processing Failed: {filename}\n{err}\n\n")
            }
            None => String::new(),
        },
    }
}

// ============================================================================
// Image description (the three tiers)
// ============================================================================

/// Everything the image-description path needs beyond the file itself.
pub struct FallbackDeps<'a, CMP: CompletionProvider> {
    pub db: &'a Db,
    pub completion: &'a CMP,
    pub transcoder: &'a dyn ImageTranscoder,
    pub user_id: &'a str,
    /// The wall clock for `logLLMCall`'s `durationMs` (frozen in the
    /// differential; the dump normalizes it regardless).
    pub now_ms: i64,
}

/// v4 `getImageDescriptionProfile(repos, userId)`.
async fn get_image_description_profile<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
) -> Result<Option<Value>, crate::db::DbError> {
    let user_id = deps.user_id.to_string();
    let settings = deps
        .db
        .read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &user_id))?;
    let image_desc_id = settings
        .as_ref()
        .and_then(|s| s.get("imageDescriptionProfileId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if let Some(id) = image_desc_id {
        let profile = deps
            .db
            .read_main(move |c| crate::db::connection_profiles::find_by_id(c, &id))?;
        if let Some(p) = profile {
            return Ok(Some(p));
        }
    }

    // Fallback auto-pick: vision-capable profiles, cheap first. Filter to
    // profiles that can *actually* describe an image: the model must read
    // pictures AND its plugin must be able to send them. A NanoGPT vision
    // profile passes the first test and fails the second, and picking one as
    // the describer would produce a confident description of an image the
    // model never received (bug 91, v4 `a14a1811`).
    let uid = deps.user_id.to_string();
    let available = deps
        .db
        .read_main(move |c| crate::db::connection_profiles::find_by_user_id(c, &uid))?;
    let vision: Vec<Value> = available
        .into_iter()
        .filter(|p| {
            profile_supports_mime_type(p, "image/jpeg")
                && crate::files::image_transport::provider_can_transport_images(
                    p.get("provider").and_then(Value::as_str).unwrap_or(""),
                )
        })
        .collect();
    if vision.is_empty() {
        return Ok(None);
    }
    if let Some(cheap) = vision
        .iter()
        .find(|p| p.get("isCheap").and_then(Value::as_bool) == Some(true))
    {
        return Ok(Some(cheap.clone()));
    }
    Ok(Some(vision[0].clone()))
}

/// v4 `getUncensoredImageDescriptionProfile(repos, userId)` — never auto-picked.
async fn get_uncensored_image_description_profile<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
) -> Result<Option<Value>, crate::db::DbError> {
    let user_id = deps.user_id.to_string();
    let settings = deps
        .db
        .read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &user_id))?;
    let id = settings
        .as_ref()
        .and_then(|s| s.get("uncensoredImageDescriptionProfileId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let Some(id) = id else {
        return Ok(None);
    };
    let profile = deps
        .db
        .read_main(move |c| crate::db::connection_profiles::find_by_id(c, &id))?;
    Ok(profile)
}

/// A reasoning model per v4 `modelName.toLowerCase().match(/o1|o3|gpt-5|reasoning/)`.
fn is_reasoning_model(model_name: &str) -> bool {
    let lower = model_name.to_lowercase();
    lower.contains("o1")
        || lower.contains("o3")
        || lower.contains("gpt-5")
        || lower.contains("reasoning")
}

/// v4 `describeImageWithProfile(file, imageDescProfile, repos, userId)`.
async fn describe_image_with_profile<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
    file: &FallbackFile,
    profile: &Value,
) -> FallbackResult {
    // v4 `file-attachment-fallback.ts:205` — `const describeStart = Date.now()`,
    // captured at the top of the function so BOTH `logLLMCall` sites (the success
    // row at `:351` and the failure row at `:460`) subtract from the same mark.
    let describe_start_ms = crate::clock::now_unix_ms();
    let provider = profile
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let model_name = profile
        .get("modelName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let base_url = profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Support check (v4: the profile might not support the specific mimeType).
    // Both halves matter — see `needs_fallback_processing`. A describer whose
    // plugin drops the bytes would answer from the prompt alone and invent a
    // picture (bug 91, v4 `a14a1811`).
    if !profile_supports_mime_type(profile, &file.mime_type) {
        let mut result = FallbackResult::unsupported(
            &file.filename,
            &file.mime_type,
            Some(format!(
                "Image description profile ({provider} {model_name}) does not support image files"
            )),
        );
        set_description_metadata(&mut result, &profile_id, &provider, &model_name);
        return result;
    }

    // v4 `0ba942b1` (bug 97): the provider list in the message below names every
    // entry in v4's `PROVIDER_ATTACHMENT_CAPABILITIES`
    // (`crate::files::attachment_support`) that carries an `image/*` MIME type —
    // keep it in step with that map. Bug 97 was this sentence recommending
    // OpenRouter in the same breath as refusing an OpenRouter profile, because
    // the plugin's own declaration disagreed with the map; v4's
    // `__tests__/unit/lib/llm/image-transport.test.ts` now holds the two sources
    // together, and `image_transport_equivalence` is its v5 twin.
    if !crate::files::image_transport::provider_can_transport_images(&provider) {
        let mut result = FallbackResult::unsupported(
            &file.filename,
            &file.mime_type,
            Some(format!(
                "Image description profile ({provider} {model_name}) cannot send images — the \
                 {provider} plugin does not forward image attachments. Pick a describer on a \
                 provider that does (OpenAI, Anthropic, Google, Grok, OpenRouter, NanoGPT, \
                 Z.AI)."
            )),
        );
        set_description_metadata(&mut result, &profile_id, &provider, &model_name);
        return result;
    }

    // API key (host-side; the canned provider ignores it, so it does not affect
    // the differential — resolved for faithfulness).
    let _api_key = resolve_api_key(deps, profile).await;

    // Parameters (snake_case input keys; camelCase wire output). v4 `d9c5a1c7`
    // replaced the raw `imageDescProfile.parameters` cast with
    // `profileParams(imageDescProfile) ?? {}`, so the bag is ALWAYS an object
    // here and the Ollama `num_ctx` injection reaches the vision call.
    let params_obj = crate::cheap_llm::profile_params_value(profile).unwrap_or_else(|| json!({}));
    // P4.D83 (v4 `d89babc4`): the three hand-rolled `typeof === 'number'` reads
    // became `resolveSamplingParams(modelParams)` + the two defaults. The
    // reasoning-floor logic below is unchanged; what moves is that a
    // string-valued or camelCase knob now counts, and `top_p` reaches the wire
    // (v4 sets `messageParams.topP`; v5's completion path could not carry one
    // until this lane widened `CompletionParams`).
    let sampling = crate::sampling_params::resolve_sampling_params(Some(&params_obj));
    let temperature = sampling.temperature.unwrap_or(DEFAULT_VISION_TEMPERATURE);
    let top_p = sampling.top_p;
    let mut max_tokens = sampling
        .max_tokens
        .map(|n| n as i64)
        .unwrap_or(DEFAULT_VISION_MAX_TOKENS);
    let reasoning = is_reasoning_model(&model_name);
    if reasoning && max_tokens < 4000 {
        max_tokens = 4000;
    }

    // Downsize to the description provider's limit FIRST (best-effort).
    let mut attachment_mime = file.mime_type.clone();
    let mut attachment_data = file.data.clone().unwrap_or_default();
    if let Some(data) = &file.data {
        if can_resize_image(&file.mime_type) {
            if let Some((resized_data, resized_mime)) =
                try_downsize(deps, &provider, data, &file.mime_type)
            {
                attachment_data = resized_data;
                attachment_mime = resized_mime;
            }
        }
    }

    // Build the vision call.
    let messages = vec![CompletionMessage::user(IMAGE_DESCRIPTION_INSTRUCTION)];
    let attachments = vec![CompletionAttachment {
        id: file.id.clone(),
        filename: file.filename.clone(),
        mime_type: attachment_mime.clone(),
        data: attachment_data,
    }];
    let call_params = CompletionParams {
        messages: messages.clone(),
        model: model_name.clone(),
        temperature: Some(temperature),
        // v4 `if (maxTokens !== undefined && maxTokens > 0) messageParams.maxTokens = …`
        // — a non-positive cap leaves the key OFF, so the provider default
        // applies. v5 used to pass `0`, which the builders emitted literally.
        max_tokens: if max_tokens > 0 {
            Some(max_tokens)
        } else {
            None
        },
        strict_max_tokens: false,
        // v4 `if (topP !== undefined) messageParams.topP = topP` — no default.
        top_p,
        cache_key: None,
        // v4's guard is `if (modelParams && typeof modelParams === 'object')`
        // over a value the `?? {}` above already made an object (or an array,
        // which also passes `typeof`), so it is unconditionally set.
        profile_parameters: Some(params_obj.clone()),
        attachments: attachments.clone(),
        // v4's `generateImageDescription` sets no `requestTimeoutMs`.
        request_timeout_ms: None,
    };

    let response = deps
        .completion
        .send_message(&provider, base_url.as_deref(), &call_params)
        .await;

    match response {
        Ok(resp) => {
            // Success-path logLLMCall (IMAGE_DESCRIPTION), best-effort.
            log_description_success(
                deps,
                &provider,
                &model_name,
                &file.filename,
                &attachment_mime,
                temperature,
                max_tokens,
                &resp,
                describe_start_ms,
            )
            .await;

            let trimmed = crate::jsstr::js_trim(&resp.content).to_string();
            if trimmed.is_empty() {
                // Reasoning-token exhaustion vs generic empty.
                if resp.finish_reason.as_deref() == Some("length") && reasoning {
                    let tokens = resp
                        .usage
                        .and_then(|u| {
                            if u.completion_tokens != 0 {
                                Some(u.completion_tokens)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(max_tokens);
                    let mut result = FallbackResult::unsupported(&file.filename, &file.mime_type, Some(format!(
                        "Image description failed - {model_name} is a reasoning model that used all {tokens} tokens for internal reasoning and didn't output a description. Reasoning models are expensive and slow for this task. Switch to gpt-4o-mini, claude-haiku-4-5, or gemini-2.0-flash instead."
                    )));
                    set_description_metadata(&mut result, &profile_id, &provider, &model_name);
                    return result;
                }
                let mut result = FallbackResult::unsupported(&file.filename, &file.mime_type, Some(format!(
                    "Image could not be processed - {provider} {model_name} returned empty response. The model may not support vision. Try using gpt-4o-mini, claude-haiku-4-5, or gemini-2.0-flash as your image description profile."
                )));
                set_description_metadata(&mut result, &profile_id, &provider, &model_name);
                return result;
            }

            let content_lower = trimmed.to_lowercase();
            if content_lower.contains("error")
                || content_lower.contains("cannot")
                || content_lower.contains("unable to")
                || content_lower.contains("failed to")
                || content_lower.contains("not support")
                || content_lower.contains("invalid")
                || crate::jsstr::utf16_len(&trimmed) < 20
            {
                let snippet = crate::jsstr::utf16_truncate(&trimmed, 100);
                let mut result = FallbackResult::unsupported(&file.filename, &file.mime_type, Some(format!(
                    "The image description profile responded with: \"{snippet}...\". This appears to be an error rather than an image description. The model may not support images or there's a parameter mismatch. Try using gpt-4o-mini, claude-haiku-4-5, or gemini-2.0-flash."
                )));
                set_description_metadata(&mut result, &profile_id, &provider, &model_name);
                return result;
            }

            // Success — imageDescription is the RAW content (not trimmed).
            FallbackResult {
                type_: FallbackType::ImageDescription,
                text_content: None,
                image_description: Some(resp.content.clone()),
                processing_metadata: Some(ProcessingMetadata {
                    used_image_description_llm: Some(true),
                    description_profile_id: Some(profile_id.clone()),
                    description_provider: Some(provider.clone()),
                    description_model: Some(model_name.clone()),
                    original_filename: file.filename.clone(),
                    original_mime_type: file.mime_type.clone(),
                    ..Default::default()
                }),
                error: None,
            }
        }
        Err(err) => {
            // Failure-path logLLMCall (IMAGE_DESCRIPTION), best-effort.
            log_description_failure(
                deps,
                &provider,
                &model_name,
                &err.message,
                describe_start_ms,
            )
            .await;
            let mut result = FallbackResult::unsupported(
                &file.filename,
                &file.mime_type,
                Some(format!(
                    "Failed to generate image description: {}",
                    err.message
                )),
            );
            set_description_metadata(&mut result, &profile_id, &provider, &model_name);
            result
        }
    }
}

fn set_description_metadata(
    result: &mut FallbackResult,
    profile_id: &str,
    provider: &str,
    model: &str,
) {
    if let Some(m) = result.processing_metadata.as_mut() {
        m.description_profile_id = Some(profile_id.to_string());
        m.description_provider = Some(provider.to_string());
        m.description_model = Some(model.to_string());
    }
}

/// Resolve the profile's API key off the DB (v4 `findApiKeyByIdAndUserId`).
async fn resolve_api_key<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
    profile: &Value,
) -> Option<String> {
    let api_key_id = profile.get("apiKeyId").and_then(Value::as_str)?.to_string();
    let user_id = deps.user_id.to_string();
    deps.db
        .read_main(move |c| crate::db::api_keys::find_by_id_and_user_id(c, &api_key_id, &user_id))
        .ok()
        .flatten()
        .map(|k| k.key_value)
}

/// Downsize a base64 image to the description provider's limit. Returns
/// `(base64, mimeType)` only when the resize actually happened.
fn try_downsize<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
    provider: &str,
    base64_data: &str,
    mime_type: &str,
) -> Option<(String, String)> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data.trim())
        .ok()?;
    let result = resize_image_for_provider(
        provider,
        &bytes,
        mime_type,
        DEFAULT_QUALITY,
        deps.transcoder,
    );
    if result.was_resized {
        let encoded = base64::engine::general_purpose::STANDARD.encode(&result.buffer);
        Some((encoded, result.mime_type))
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
async fn log_description_success<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
    provider: &str,
    model_name: &str,
    filename: &str,
    attachment_mime: &str,
    temperature: f64,
    max_tokens: i64,
    resp: &crate::model::completion::CompletionResponse,
    describe_start_ms: i64,
) {
    let params = LogLlmCallParams {
        user_id: deps.user_id.to_string(),
        log_type: log_type::IMAGE_DESCRIPTION.to_string(),
        message_id: None,
        chat_id: None,
        character_id: None,
        provider: provider.to_string(),
        model_name: model_name.to_string(),
        // v4 `0cde7fbc` did NOT touch `lib/chat/file-attachment-fallback.ts` —
        // this call site sets no profile id on either side.
        connection_profile_id: None,
        image_profile_id: None,
        request: LogRequest {
            messages: vec![LogRequestMessage {
                role: "user".to_string(),
                content: IMAGE_DESCRIPTION_INSTRUCTION.to_string(),
                attachments: Some(vec![serde_json::json!({
                    "filename": filename,
                    "mimeType": attachment_mime,
                })]),
            }],
            temperature: Some(temperature),
            max_tokens: Some(max_tokens),
            tools: None,
        },
        response: LogResponse {
            content: resp.content.clone(),
            error: None,
            finish_reason: resp.finish_reason.clone(),
            tool_calls: None,
        },
        usage: resp.usage.map(|u| LogUsage {
            prompt_tokens: Some(u.prompt_tokens),
            completion_tokens: Some(u.completion_tokens),
            total_tokens: Some(u.total_tokens),
        }),
        cache_usage: None,
        raw_provider_usage: None,
        request_hashes: None,
        duration_ms: Some((crate::clock::now_unix_ms() - describe_start_ms) as f64),
    };
    let _ = log_llm_call(deps.db, params, &LogContext::none()).await;
}

async fn log_description_failure<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
    provider: &str,
    model_name: &str,
    error: &str,
    describe_start_ms: i64,
) {
    let params = LogLlmCallParams {
        user_id: deps.user_id.to_string(),
        log_type: log_type::IMAGE_DESCRIPTION.to_string(),
        message_id: None,
        chat_id: None,
        character_id: None,
        provider: provider.to_string(),
        model_name: model_name.to_string(),
        // v4 `0cde7fbc` did NOT touch `lib/chat/file-attachment-fallback.ts` —
        // this call site sets no profile id on either side.
        connection_profile_id: None,
        image_profile_id: None,
        request: LogRequest {
            messages: vec![LogRequestMessage {
                role: "user".to_string(),
                content: IMAGE_DESCRIPTION_INSTRUCTION.to_string(),
                attachments: None,
            }],
            temperature: None,
            max_tokens: None,
            tools: None,
        },
        response: LogResponse {
            content: String::new(),
            error: Some(error.to_string()),
            finish_reason: None,
            tool_calls: None,
        },
        usage: None,
        cache_usage: None,
        raw_provider_usage: None,
        request_hashes: None,
        duration_ms: Some((crate::clock::now_unix_ms() - describe_start_ms) as f64),
    };
    let _ = log_llm_call(deps.db, params, &LogContext::none()).await;
}

/// v4 `generateImageDescription(file, repos, userId)` — the three tiers in order.
pub async fn generate_image_description<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
    file: &FallbackFile,
) -> FallbackResult {
    // Tier 1 — persisted-text reuse FIRST (no vision call).
    if !file.id.is_empty() {
        let file_id = file.id.clone();
        let entry = deps
            .db
            .read_main(move |c| crate::db::files::FilesRepository::new(c).find_by_id(&file_id));
        match entry {
            Ok(Some(entry)) => {
                let reused = first_non_empty_trimmed(&[
                    entry.generation_revised_prompt.as_deref(),
                    entry.generation_prompt.as_deref(),
                    entry.description.as_deref(),
                ]);
                if let Some(reused) = reused {
                    return FallbackResult {
                        type_: FallbackType::ImageDescription,
                        text_content: None,
                        image_description: Some(reused),
                        processing_metadata: Some(ProcessingMetadata {
                            used_image_description_llm: Some(false),
                            reused_persisted_description: Some(true),
                            original_filename: file.filename.clone(),
                            original_mime_type: file.mime_type.clone(),
                            ..Default::default()
                        }),
                        error: None,
                    };
                }
            }
            // Not found → fall through to vision (v4's `entry?.` short-circuits).
            Ok(None) => {}
            // Lookup failure → warn + fall through (v4's catch).
            Err(_) => {}
        }
    }

    // Tier 2 — profile selection + vision + uncensored retry.
    let image_desc_profile = match get_image_description_profile(deps).await {
        Ok(Some(p)) => p,
        Ok(None) | Err(_) => {
            return FallbackResult::unsupported(
                &file.filename,
                &file.mime_type,
                Some("No image description profile available. Configure one in Settings → Chat Settings → Image Description Profile".to_string()),
            );
        }
    };

    let primary = describe_image_with_profile(deps, file, &image_desc_profile).await;
    if primary.type_ == FallbackType::ImageDescription {
        return primary;
    }

    let fallback_profile = get_uncensored_image_description_profile(deps)
        .await
        .ok()
        .flatten();
    let primary_id = image_desc_profile
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let fallback_profile = match fallback_profile {
        Some(fp) if fp.get("id").and_then(Value::as_str) != Some(primary_id) => fp,
        // No fallback, or the same profile → return the primary result.
        _ => return primary,
    };

    let fallback_result = describe_image_with_profile(deps, file, &fallback_profile).await;
    if fallback_result.type_ == FallbackType::ImageDescription {
        let mut result = fallback_result;
        if let Some(m) = result.processing_metadata.as_mut() {
            m.used_uncensored_fallback = Some(true);
        }
        return result;
    }

    // Both failed — return the primary with a combined error.
    let mut result = primary;
    let primary_err = result
        .error
        .clone()
        .unwrap_or_else(|| "Primary failed".to_string());
    let fallback_err = fallback_result
        .error
        .unwrap_or_else(|| "unknown".to_string());
    result.error = Some(format!(
        "{primary_err} (uncensored fallback also failed: {fallback_err})"
    ));
    result
}

/// The first non-empty JS-trimmed value in priority order (v4's `a?.trim() ||
/// b?.trim() || c?.trim()`).
fn first_non_empty_trimmed(candidates: &[Option<&str>]) -> Option<String> {
    for s in candidates.iter().flatten() {
        let t = crate::jsstr::js_trim(s);
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// v4 `processFileAttachmentFallback(file, fileAttachment, profile, repos,
/// userId)` — the dispatch + keep-vs-drop (realized in the RESULT SHAPE, not by
/// stripping bytes here).
pub async fn process_file_attachment_fallback<CMP: CompletionProvider>(
    deps: &FallbackDeps<'_, CMP>,
    file: &FallbackFile,
    profile: &Value,
) -> FallbackResult {
    // Provider supports it → no fallback: KEEP raw bytes (unsupported, NO error).
    if !needs_fallback_processing(profile, &file.mime_type) {
        return FallbackResult::unsupported(&file.filename, &file.mime_type, None);
    }

    if is_text_file(&file.mime_type) {
        let Some(data) = &file.data else {
            return FallbackResult::unsupported(
                &file.filename,
                &file.mime_type,
                Some(
                    "Text file data was not loaded - file may be missing or inaccessible"
                        .to_string(),
                ),
            );
        };
        return convert_text_file_to_inline(&file.filename, &file.mime_type, data);
    }

    if is_image_file(&file.mime_type) {
        return generate_image_description(deps, file).await;
    }

    let provider = profile
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("");
    FallbackResult::unsupported(
        &file.filename,
        &file.mime_type,
        Some(format!(
            "File type {} is not supported by provider {provider} and no fallback is available",
            file.mime_type
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn needs_fallback_image_gated_by_flag() {
        let no_img = json!({ "provider": "OPENAI", "supportsImageUpload": false });
        let yes_img = json!({ "provider": "OPENAI", "supportsImageUpload": true });
        assert!(needs_fallback_processing(&no_img, "image/png"));
        assert!(!needs_fallback_processing(&yes_img, "image/png"));
        // Non-image: consult the client-safe map.
        let anthropic = json!({ "provider": "ANTHROPIC", "supportsImageUpload": true });
        assert!(!needs_fallback_processing(&anthropic, "application/pdf"));
        assert!(needs_fallback_processing(&anthropic, "application/zip"));
    }

    #[test]
    fn text_inline_markers_are_exact() {
        use base64::Engine;
        let content = "hello\nworld";
        let b64 = base64::engine::general_purpose::STANDARD.encode(content);
        let r = convert_text_file_to_inline("notes.txt", "text/plain", &b64);
        assert_eq!(r.type_, FallbackType::Text);
        assert_eq!(
            r.text_content.unwrap(),
            "[User attached text file: notes.txt]\n\nhello\nworld\n\n[End of attached file]"
        );
    }

    #[test]
    fn prefix_markers_are_exact() {
        let text = FallbackResult {
            type_: FallbackType::Text,
            text_content: Some("BODY".to_string()),
            image_description: None,
            processing_metadata: None,
            error: None,
        };
        assert_eq!(format_fallback_as_message_prefix(&text), "BODY\n\n");

        let img = FallbackResult {
            type_: FallbackType::ImageDescription,
            text_content: None,
            image_description: Some("a cat".to_string()),
            processing_metadata: Some(ProcessingMetadata {
                original_filename: "cat.png".to_string(),
                original_mime_type: "image/png".to_string(),
                ..Default::default()
            }),
            error: None,
        };
        assert_eq!(
            format_fallback_as_message_prefix(&img),
            "[Image: cat.png]\n\nImage Description (generated by AI):\na cat\n\n"
        );

        let unsupported =
            FallbackResult::unsupported("bad.xyz", "application/xyz", Some("boom".to_string()));
        assert_eq!(
            format_fallback_as_message_prefix(&unsupported),
            "⚠️ Attachment Processing Failed: bad.xyz\nboom\n\n"
        );

        // Kept-raw (unsupported, no error) → empty prefix.
        let kept = FallbackResult::unsupported("ok.png", "image/png", None);
        assert_eq!(format_fallback_as_message_prefix(&kept), "");
    }
}
