//! One picture, one question, one vision model — sent properly. v4
//! `lib/llm/vision-request.ts` (`9ecebf986`, bug 197; P4.D283).
//!
//! Two features hand a stored or uploaded image to a vision-capable connection
//! profile and read back text: the describe-fallback
//! (`services::file_fallback::describe_image_with_profile`, for chats whose
//! model cannot see) and the wardrobe's Import from image (v4
//! `lib/wardrobe/image-analysis.ts` — REFUSAL-ARMED in v5, the standing
//! `analyze-image` deferral, round ruling R-L; this module is bug 197's half
//! where v5 HAS the surface). Everything between "which profile" and "what did
//! it say" is the same, and every step of it has been a bug when skipped:
//!
//!  - **The profile must be able to receive the picture** — the model reads
//!    images *and* its plugin puts them on the wire
//!    (`profile_can_receive_attachment`, bug 91). [`resolve_vision_profile`]
//!    picks with that predicate.
//!  - **The bytes are cut to what a model needs** (`shrink_image_for_llm_transport`,
//!    bug 151) — never the stored original.
//!  - **The call is logged**, answered or not, under the caller's log type.
//!  - **The answer is not believed until the picture is known to have arrived**
//!    ([`verify_image_reached_model`], bug 116): a gateway that accepts an
//!    image and drops it produces confident, invented prose.
//!
//! [`send_vision_request`] is those steps, once.
//!
//! v4's `[Vision] Failed to record the vision call in llm_logs` WARN guards a
//! throw out of `logLLMCall`, which swallows its own errors — as v5's
//! `log_llm_call` does; the arm is unreachable on both sides and is not
//! emitted (pinned NEGATIVE by `vision_request_tier3`).

use serde_json::Value;

use crate::db::runtime::Db;
use crate::files::image_processing::ImageTranscoder;
use crate::files::image_transport::{profile_can_receive_attachment, AttachmentProfileView};
use crate::model::completion::{
    CompletionAttachment, CompletionError, CompletionMessage, CompletionParams, CompletionProvider,
    CompletionResponse,
};
use crate::services::llm_logging::{
    log_llm_call, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage, LogResponse,
    LogUsage,
};

const LOG_TARGET: &str = "quilltap::vision_request";

// ============================================================================
// ARRIVAL CHECK
// ============================================================================

/// A deliberately pessimistic characters-per-token ratio, used only to put a
/// *ceiling* on what the text of a request could cost on its own. Real BPE
/// tokenizers run 3.5–4.5 chars/token on English prose, so 2.5 leaves ~40%
/// headroom before a text-only prompt could climb past the ceiling and be
/// mistaken for a real one.
const MIN_CHARS_PER_TOKEN: f64 = 2.5;

/// v4 `textOnlyTokenCeiling(text)` — the most prompt tokens `text` could
/// plausibly cost on its own (`Math.ceil(text.length / 2.5)`; `.length` is
/// UTF-16 units, taken faithfully).
pub fn text_only_token_ceiling(text: &str) -> i64 {
    (crate::jsstr::utf16_len(text) as f64 / MIN_CHARS_PER_TOKEN).ceil() as i64
}

/// Verdict from [`verify_image_reached_model`] (v4 `ImageArrivalVerdict`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageArrivalVerdict {
    /// v4 `{ arrived: true }`.
    Arrived,
    /// v4 `{ arrived: false, reason }`.
    NotArrived { reason: String },
}

impl ImageArrivalVerdict {
    pub fn arrived(&self) -> bool {
        matches!(self, ImageArrivalVerdict::Arrived)
    }
    /// The refusal sentence, or `None` when the image arrived.
    pub fn reason(&self) -> Option<&str> {
        match self {
            ImageArrivalVerdict::Arrived => None,
            ImageArrivalVerdict::NotArrived { reason } => Some(reason),
        }
    }
}

/// v4 `verifyImageReachedModel(response, attachmentId, requestText)` — did the
/// image actually reach the model, or did we get confident prose about a
/// picture nobody looked at?
///
/// Bug 116: a NanoGPT route for an experimental vision model accepted the
/// `image_url` part and discarded it, then answered the text it had with a
/// detailed, entirely invented description. Two proofs were on the response
/// object and neither was read:
///
///  1. **The plugin's attachment ledger.** `attachmentResults.failed` is the
///     plugin saying it did not send the bytes.
///  2. **The response's own token count.** `promptTokens` at or below what the
///     request's text costs by itself ([`text_only_token_ceiling`] of
///     `request_text` — GENERALISED at `9ecebf986` from the describe-fallback's
///     fixed instruction ceiling) is a provider-agnostic statement that no
///     image was processed — the cheapest image tier in the field (OpenAI
///     low-detail, 85 tokens) still lands a real call clear of it.
///
/// Silence is not evidence: a missing `usage`, or a zero `promptTokens`, means
/// the provider reported nothing and must not be failed for it. Cache-read
/// tokens are added back before comparing, because every plugin normalises
/// them *out* of `promptTokens` (the 4.6.1 invariant) and a cache hit would
/// otherwise read as a dropped image.
pub fn verify_image_reached_model(
    response: &CompletionResponse,
    attachment_id: &str,
    request_text: &str,
) -> ImageArrivalVerdict {
    if let Some(results) = &response.attachment_results {
        if let Some(first) = results.failed.first() {
            let mine = results
                .failed
                .iter()
                .find(|f| f.id == attachment_id)
                .unwrap_or(first);
            // v4 `${mine.error || 'no reason given'}`.
            let error = if mine.error.is_empty() {
                "no reason given"
            } else {
                mine.error.as_str()
            };
            return ImageArrivalVerdict::NotArrived {
                reason: format!("the provider reported the attachment as not sent: {error}"),
            };
        }
    }

    let Some(prompt_tokens) = response.usage.map(|u| u.prompt_tokens) else {
        return ImageArrivalVerdict::Arrived;
    };
    if prompt_tokens <= 0 {
        return ImageArrivalVerdict::Arrived;
    }

    let ceiling = text_only_token_ceiling(request_text);
    let cache_read = response
        .cache_usage
        .map(|c| c.cache_read_input_tokens.unwrap_or(0) + c.cached_tokens.unwrap_or(0))
        .unwrap_or(0);
    let billed_input = prompt_tokens + cache_read;
    if billed_input <= ceiling {
        return ImageArrivalVerdict::NotArrived {
            reason: format!(
                "the model was billed for {billed_input} prompt tokens, which is no more than the \
                 {ceiling} the request's text costs on its own — the image was accepted and \
                 discarded before it reached the model, and any answer returned is invented"
            ),
        };
    }

    ImageArrivalVerdict::Arrived
}

// ============================================================================
// PROFILE RESOLUTION
// ============================================================================

/// Among auto-picked profiles, prefer a cheap one (the describe-fallback,
/// which runs on every unseen image) or a non-cheap one (Import from image,
/// an occasional, deliberate request where quality is the point).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisionPreference {
    Cheap,
    Capable,
}

impl VisionPreference {
    pub fn as_str(self) -> &'static str {
        match self {
            VisionPreference::Cheap => "cheap",
            VisionPreference::Capable => "capable",
        }
    }
}

/// What to do with a configured Image Description profile that cannot
/// receive images. `Honour` returns it anyway, so the caller can say why it
/// failed (the describe-fallback: the operator chose it). `SkipIncapable`
/// passes over it to an auto-pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfiguredVisionProfile {
    Honour,
    SkipIncapable,
}

/// v4 `ResolveVisionProfileOptions`.
#[derive(Clone, Copy, Debug)]
pub struct ResolveVisionProfileOptions {
    pub prefer: VisionPreference,
    pub configured: ConfiguredVisionProfile,
}

/// v4 `resolveVisionProfile(repos, userId, options)` — the vision profile for
/// a request: the operator's configured Image Description profile (Chat
/// settings) first, else any profile that can receive an image
/// (`profileCanReceiveAttachment`), by the stated preference. The whole
/// profile row (v4's `ConnectionProfile`). Every read is the repository's
/// (`findByUserId` / `findById` — a read error propagates, as v4's does).
pub fn resolve_vision_profile(
    db: &Db,
    user_id: &str,
    options: ResolveVisionProfileOptions,
) -> Result<Option<Value>, crate::db::DbError> {
    let uid = user_id.to_string();
    let chat_settings =
        db.read_main(move |c| crate::db::chat_settings::find_by_user_id(c, &uid))?;
    let configured_id = chat_settings
        .as_ref()
        .and_then(|s| s.get("imageDescriptionProfileId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(id) = configured_id {
        let configured =
            db.read_main(move |c| crate::db::connection_profiles::find_by_id(c, &id))?;
        if let Some(configured) = configured {
            if options.configured == ConfiguredVisionProfile::Honour
                || profile_can_receive_attachment(
                    AttachmentProfileView::from_json(&configured),
                    "image/jpeg",
                )
            {
                tracing::debug!(
                    target: LOG_TARGET,
                    profileId = %str_field(&configured, "id"),
                    provider = %str_field(&configured, "provider"),
                    model = %str_field(&configured, "modelName"),
                    "[Vision] Using the configured image description profile"
                );
                return Ok(Some(configured));
            }
            tracing::debug!(
                target: LOG_TARGET,
                profileId = %str_field(&configured, "id"),
                provider = %str_field(&configured, "provider"),
                model = %str_field(&configured, "modelName"),
                "[Vision] Configured image description profile cannot receive images; auto-picking"
            );
        }
    }

    let uid = user_id.to_string();
    let available =
        db.read_main(move |c| crate::db::connection_profiles::find_by_user_id(c, &uid))?;
    let vision_profiles: Vec<Value> = available
        .into_iter()
        .filter(|p| {
            profile_can_receive_attachment(AttachmentProfileView::from_json(p), "image/jpeg")
        })
        .collect();
    if vision_profiles.is_empty() {
        tracing::debug!(
            target: LOG_TARGET,
            userId = user_id,
            "[Vision] No profile can receive images"
        );
        return Ok(None);
    }

    // v4 `prefer === 'cheap' ? p.isCheap === true : !p.isCheap`.
    let is_cheap = |p: &Value| p.get("isCheap").and_then(Value::as_bool) == Some(true);
    let preferred = vision_profiles.iter().find(|p| match options.prefer {
        VisionPreference::Cheap => is_cheap(p),
        VisionPreference::Capable => !p.get("isCheap").and_then(Value::as_bool).unwrap_or(false),
    });
    let picked = preferred.unwrap_or(&vision_profiles[0]).clone();
    tracing::debug!(
        target: LOG_TARGET,
        profileId = %str_field(&picked, "id"),
        provider = %str_field(&picked, "provider"),
        model = %str_field(&picked, "modelName"),
        isCheap = %js_field(&picked, "isCheap"),
        prefer = options.prefer.as_str(),
        "[Vision] Auto-picked a vision profile"
    );
    Ok(Some(picked))
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A field as a JS log bag renders it: a string bare, `null` for null/absent,
/// a number or boolean as itself.
fn js_field(v: &Value, key: &str) -> String {
    match v.get(key) {
        None | Some(Value::Null) => "null".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

// ============================================================================
// THE REQUEST
// ============================================================================

/// v4 `VisionRequestInput.sampling`.
#[derive(Clone, Copy, Debug, Default)]
pub struct VisionSampling {
    pub temperature: Option<f64>,
    pub max_tokens: Option<i64>,
    pub top_p: Option<f64>,
}

/// v4 `VisionRequestInput`.
pub struct VisionRequestInput<'a> {
    /// The vision profile row (v4 `ConnectionProfile`).
    pub profile: &'a Value,
    pub user_id: &'a str,
    /// The picture. Base64 `data` is shrunk for transport here; an attachment
    /// the plugin loads itself (no `data`) is passed through as given.
    pub attachment: &'a CompletionAttachment,
    /// Optional system message.
    pub system_prompt: Option<&'a str>,
    /// The user message the picture rides on.
    pub instruction: &'a str,
    pub sampling: VisionSampling,
    pub timeout_ms: u64,
    /// The `llm_logs` type the call is recorded under.
    pub log_type: &'static str,
    /// Refuse (rather than send keyless) when the profile has no API key.
    /// Ollama is exempt.
    pub require_api_key: bool,
    /// The transport shrink's codec (v4 `sharp`).
    pub transcoder: &'a dyn ImageTranscoder,
}

/// Why a vision request produced no usable answer without throwing (v4
/// `VisionRequestRefusal`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisionRequestRefusal {
    ModelCannotRead,
    PluginCannotSend,
    MissingApiKey,
    ImageNotReceived,
}

impl VisionRequestRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            VisionRequestRefusal::ModelCannotRead => "model-cannot-read",
            VisionRequestRefusal::PluginCannotSend => "plugin-cannot-send",
            VisionRequestRefusal::MissingApiKey => "missing-api-key",
            VisionRequestRefusal::ImageNotReceived => "image-not-received",
        }
    }
}

/// v4 `VisionRequestResult`.
#[derive(Clone, Debug)]
pub enum VisionRequestResult {
    /// v4 `{ ok: true, response, sentAttachment }`.
    Ok {
        response: CompletionResponse,
        sent_attachment: CompletionAttachment,
    },
    /// v4 `{ ok: false, refusal, detail, response? }`.
    Refused {
        refusal: VisionRequestRefusal,
        detail: String,
        response: Option<CompletionResponse>,
    },
}

/// What the shrink reported, for the `Sending image` DEBUG (v4 `shrunk?.…`).
struct ShrinkFacts {
    original_bytes: usize,
    sent_bytes: usize,
    was_shrunk: bool,
}

/// v4 `sendVisionRequest(input)` — send one picture to one vision profile and
/// return its answer, only once the picture is known to have arrived. A
/// provider error or timeout is logged and rethrown (`Err`); the four refusals
/// come back as [`VisionRequestResult::Refused`].
pub async fn send_vision_request<CMP: CompletionProvider>(
    db: &Db,
    completion: &CMP,
    input: VisionRequestInput<'_>,
) -> Result<VisionRequestResult, CompletionError> {
    let profile = input.profile;
    let provider = str_field(profile, "provider").to_string();
    let model_name = str_field(profile, "modelName").to_string();
    let profile_id = str_field(profile, "id").to_string();
    let base_url = profile
        .get("baseUrl")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let attachment = input.attachment;
    // v4 `const startedAt = Date.now()`.
    let started_at = crate::clock::now_unix_ms();

    // Both halves of `profileCanReceiveAttachment`, asked separately so the
    // refusal can say which one failed.
    let view = AttachmentProfileView::from_json(profile);
    if !crate::files::attachment_support::profile_supports_mime_type(view, &attachment.mime_type) {
        return Ok(VisionRequestResult::Refused {
            refusal: VisionRequestRefusal::ModelCannotRead,
            detail: format!(
                "{provider} {model_name} does not read {} files",
                attachment.mime_type
            ),
            response: None,
        });
    }
    if attachment.mime_type.starts_with("image/")
        && !crate::files::image_transport::provider_can_transport_images(&provider)
    {
        return Ok(VisionRequestResult::Refused {
            refusal: VisionRequestRefusal::PluginCannotSend,
            detail: format!("the {provider} plugin does not forward image attachments"),
            response: None,
        });
    }

    // v4: `apiKeyValue = ''`; `if (profile.apiKeyId)` → `findApiKeyByIdAndUserId`
    // (the scoped fallback read) → its `key_value`.
    let api_key_value = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .and_then(|id| crate::services::api_key_service::read_api_key_scoped(db, id, input.user_id))
        .map(|k| k.key_value)
        .unwrap_or_default();
    if input.require_api_key && api_key_value.is_empty() && provider != "OLLAMA" {
        return Ok(VisionRequestResult::Refused {
            refusal: VisionRequestRefusal::MissingApiKey,
            detail: format!("API key not found for provider {provider}"),
            response: None,
        });
    }

    // What a model needs, not what the archive keeps (bug 151). Never throws:
    // an unshrinkable image goes as it came.
    let (sent_attachment, shrink) = if attachment.data.is_empty() {
        (attachment.clone(), None)
    } else {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(attachment.data.trim())
            .unwrap_or_default();
        let shrunk = crate::files::llm_image_budget::shrink_image_for_llm_transport(
            input.transcoder,
            &bytes,
            &attachment.mime_type,
            Some(&provider),
            Some(&attachment.filename),
        );
        let facts = ShrinkFacts {
            original_bytes: shrunk.original_size,
            sent_bytes: shrunk.final_size,
            was_shrunk: shrunk.was_shrunk,
        };
        let sent = if shrunk.was_shrunk {
            CompletionAttachment {
                id: attachment.id.clone(),
                filename: attachment.filename.clone(),
                mime_type: shrunk.mime_type.clone(),
                data: base64::engine::general_purpose::STANDARD.encode(&shrunk.buffer),
            }
        } else {
            attachment.clone()
        };
        (sent, Some(facts))
    };

    // v4 `profileParams(profile)` — forwarded so a "reasoning off" setting on
    // the vision profile takes effect; the guard `modelParams && typeof
    // modelParams === 'object'` passes whenever the bag is an object.
    let model_params = crate::cheap_llm::profile_params_value(profile);
    let mut messages: Vec<CompletionMessage> = Vec::new();
    if let Some(system) = input.system_prompt {
        messages.push(CompletionMessage::system(system));
    }
    messages.push(CompletionMessage::user(input.instruction));
    let params = CompletionParams {
        messages: messages.clone(),
        model: model_name.clone(),
        temperature: input.sampling.temperature,
        // v4 `if (maxTokens !== undefined && maxTokens > 0) messageParams.maxTokens = …`.
        max_tokens: input.sampling.max_tokens.filter(|n| *n > 0),
        strict_max_tokens: false,
        top_p: input.sampling.top_p,
        cache_key: None,
        profile_parameters: model_params.filter(|p| p.is_object() || p.is_array()),
        attachments: vec![sent_attachment.clone()],
        request_timeout_ms: None,
    };

    // v4 `loggedRequest` — the same shape on the success AND failure rows:
    // the system message (when given), the instruction with the attachment's
    // filename + SENT mime, the sampling's temperature and maxTokens.
    let logged_request = || LogRequest {
        messages: {
            let mut m: Vec<LogRequestMessage> = Vec::new();
            if let Some(system) = input.system_prompt {
                m.push(LogRequestMessage {
                    role: "system".to_string(),
                    content: system.to_string(),
                    attachments: None,
                });
            }
            m.push(LogRequestMessage {
                role: "user".to_string(),
                content: input.instruction.to_string(),
                attachments: Some(vec![serde_json::json!({
                    "filename": attachment.filename,
                    "mimeType": sent_attachment.mime_type,
                })]),
            });
            m
        },
        temperature: input.sampling.temperature,
        max_tokens: input.sampling.max_tokens,
        tools: None,
    };

    tracing::debug!(
        target: LOG_TARGET,
        logType = input.log_type,
        profileId = %profile_id,
        provider = %provider,
        model = %model_name,
        originalBytes = %shrink.as_ref().map_or("null".to_string(), |s| s.original_bytes.to_string()),
        sentBytes = %shrink.as_ref().map_or("null".to_string(), |s| s.sent_bytes.to_string()),
        shrunk = shrink.as_ref().is_some_and(|s| s.was_shrunk),
        "[Vision] Sending image to vision model"
    );

    // v4 `withTimeout(provider.sendMessage(messageParams, apiKeyValue),
    // timeoutMs, `Vision request timed out after ${timeoutMs}ms`)` — a race,
    // not an abort.
    let call =
        completion.send_message_keyed(&provider, base_url.as_deref(), &api_key_value, &params);
    let response = match tokio::time::timeout(
        std::time::Duration::from_millis(input.timeout_ms),
        call,
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            record(
                db,
                input.user_id,
                input.log_type,
                &provider,
                &model_name,
                logged_request(),
                LogResponse {
                    content: String::new(),
                    error: Some(e.message.clone()),
                    ..Default::default()
                },
                None,
                started_at,
            )
            .await;
            return Err(e);
        }
        Err(_elapsed) => {
            let message = format!("Vision request timed out after {}ms", input.timeout_ms);
            record(
                db,
                input.user_id,
                input.log_type,
                &provider,
                &model_name,
                logged_request(),
                LogResponse {
                    content: String::new(),
                    error: Some(message.clone()),
                    ..Default::default()
                },
                None,
                started_at,
            )
            .await;
            return Err(CompletionError::new(message));
        }
    };

    record(
        db,
        input.user_id,
        input.log_type,
        &provider,
        &model_name,
        logged_request(),
        LogResponse {
            content: response.content.clone(),
            error: None,
            finish_reason: response.finish_reason.clone(),
            tool_calls: None,
        },
        response.usage.map(|u| LogUsage {
            prompt_tokens: Some(u.prompt_tokens),
            completion_tokens: Some(u.completion_tokens),
            total_tokens: Some(u.total_tokens),
        }),
        started_at,
    )
    .await;

    // Before believing a word of it: did the image arrive? (bug 116)
    let request_text = format!("{}{}", input.system_prompt.unwrap_or(""), input.instruction);
    let arrival = verify_image_reached_model(&response, &sent_attachment.id, &request_text);
    if let ImageArrivalVerdict::NotArrived { reason } = arrival {
        tracing::warn!(
            target: LOG_TARGET,
            logType = input.log_type,
            profileId = %profile_id,
            provider = %provider,
            model = %model_name,
            reason = %reason,
            promptTokens = response.usage.map(|u| u.prompt_tokens),
            contentLength = crate::jsstr::utf16_len(&response.content),
            "[Vision] Model answered without the image; discarding its answer"
        );
        return Ok(VisionRequestResult::Refused {
            refusal: VisionRequestRefusal::ImageNotReceived,
            detail: reason,
            response: Some(response),
        });
    }

    tracing::debug!(
        target: LOG_TARGET,
        logType = input.log_type,
        profileId = %profile_id,
        contentLength = crate::jsstr::utf16_len(&response.content),
        durationMs = crate::clock::now_unix_ms() - started_at,
        "[Vision] Vision answer received"
    );
    Ok(VisionRequestResult::Ok {
        response,
        sent_attachment,
    })
}

/// v4 `record(call)` — one `llm_logs` row under the caller's type, answered
/// or not. `log_llm_call` never fails the picture.
#[allow(clippy::too_many_arguments)]
async fn record(
    db: &Db,
    user_id: &str,
    log_type: &'static str,
    provider: &str,
    model_name: &str,
    request: LogRequest,
    response: LogResponse,
    usage: Option<LogUsage>,
    started_at: i64,
) {
    let _ = log_llm_call(
        db,
        LogLlmCallParams {
            user_id: user_id.to_string(),
            log_type: log_type.to_string(),
            message_id: None,
            chat_id: None,
            character_id: None,
            provider: provider.to_string(),
            model_name: model_name.to_string(),
            connection_profile_id: None,
            image_profile_id: None,
            request,
            response,
            usage,
            cache_usage: None,
            raw_provider_usage: None,
            request_hashes: None,
            duration_ms: Some((crate::clock::now_unix_ms() - started_at) as f64),
        },
        &LogContext::none(),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceiling_is_ceil_of_utf16_length_over_two_and_a_half() {
        assert_eq!(text_only_token_ceiling(""), 0);
        assert_eq!(text_only_token_ceiling("ab"), 1);
        assert_eq!(text_only_token_ceiling("abcde"), 2);
        assert_eq!(text_only_token_ceiling("abcdef"), 3);
        // A supplementary-plane character is TWO UTF-16 units.
        assert_eq!(text_only_token_ceiling("🙂🙂🙂"), 3);
    }
}
