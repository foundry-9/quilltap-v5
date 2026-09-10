//! Shared helpers for the avatar + story-background job handlers: building the
//! cheap-LLM selection from the user's profiles, decoding the provider's base64
//! bytes, and the generate-with-post-hoc-reroute flow the two handlers share
//! (parameterized by the failure-message prefix + orientation).
//!
//! **The params are no longer built here.** Until v4 `84f33ce94` these two job
//! paths assembled the request inline as `{ prompt, model, n: 1, ...resolved,
//! quality, style: 'natural' }`, reading exactly ONE key off the profile — so a
//! `negativePrompt`, `seed`, `guidanceScale`, `steps` or (later) `loras`
//! configured on a profile worked in the Salon and vanished for avatars and
//! story backgrounds. v5 inherited that drift verbatim (the deleted
//! `build_job_gen_params` hard-coded it and `quality_from_parameters` WAS the
//! "reads only quality" bug). Both attempts now go through the one shared
//! [`build_image_gen_params`], so these paths GAIN every one of those fields.

use rusqlite::Connection;
use serde_json::Value;

use crate::cheap_llm::{
    get_cheap_llm_provider, CheapLlmConfig, CheapLlmProfile, CheapLlmSelection,
};
use crate::db::runtime::Db;
use crate::image_gen::params_builder::{
    build_image_gen_params, ImageDeclarations, ImageGenOverrides, ImageParamsLogContext,
    ImageProfileLike,
};
use crate::image_gen::Orientation;
use crate::model::image::{ImageGenError, ImageGenResponse, ImageProvider};
use crate::services::dangerous_content::provider_routing::{
    is_image_moderation_error, resolve_uncensored_image_profile_for_reroute, ApiKeyResolver,
};
use crate::services::llm_logging::{
    log_llm_call, log_type, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage,
    LogResponse,
};

/// The plugin-registry declaration seam (v4's `getImageGenerationModels` +
/// `getImageProviderConstraints`, per provider). `84f33ce94` widened it from
/// the orientation half to v4's whole declaration set — see
/// [`ImageDeclarations`].
pub type ImageDeclarationsFn = dyn Fn(&str) -> ImageDeclarations + Send + Sync;

/// The job handlers' fixed overrides — v4's `{ n: 1, style: 'natural' }` on
/// BOTH job paths and BOTH attempts (natural reads better for an avatar and for
/// an ambient background alike).
fn job_overrides() -> ImageGenOverrides {
    ImageGenOverrides {
        n: Some(1.0),
        style: Some("natural".to_string()),
        ..Default::default()
    }
}

/// v4 `buildImageGenParams`'s `fallbackModel` default.
const DEFAULT_IMAGE_MODEL: &str = "dall-e-3";

/// v4 `cheapLLMProfile` field extraction from a connection-profile `Value`.
pub(crate) fn cheap_llm_profile_from_value(v: &Value) -> CheapLlmProfile {
    CheapLlmProfile {
        id: v
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        provider: v
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model_name: v
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        base_url: v.get("baseUrl").and_then(Value::as_str).map(str::to_string),
        is_cheap: v.get("isCheap").and_then(Value::as_bool) == Some(true),
        is_dangerous_compatible: v.get("isDangerousCompatible").and_then(Value::as_bool)
            == Some(true),
        parameters: v.get("parameters").cloned(),
        max_tokens: v.get("maxTokens").and_then(Value::as_f64),
        max_context: v.get("maxContext").and_then(Value::as_f64),
        model_class: v
            .get("modelClass")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// v4's `CheapLLMConfig` from a chat-settings `cheapLLMSettings` sub-object (or the
/// `DEFAULT_CHEAP_LLM_CONFIG` defaults when absent).
pub(crate) fn cheap_llm_config_from_settings(cheap: Option<&Value>) -> CheapLlmConfig {
    match cheap {
        Some(c) => CheapLlmConfig {
            strategy: c
                .get("strategy")
                .and_then(Value::as_str)
                .unwrap_or("PROVIDER_CHEAPEST")
                .to_string(),
            user_defined_profile_id: c
                .get("userDefinedProfileId")
                .and_then(Value::as_str)
                .map(str::to_string),
            default_cheap_profile_id: c
                .get("defaultCheapProfileId")
                .and_then(Value::as_str)
                .map(str::to_string),
            fallback_to_local: c
                .get("fallbackToLocal")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        None => CheapLlmConfig {
            strategy: "PROVIDER_CHEAPEST".to_string(),
            user_defined_profile_id: None,
            default_cheap_profile_id: None,
            fallback_to_local: false,
        },
    }
}

/// v4's repeated selection block: build the cheap-LLM selection from `allProfiles`
/// (default profile → `getCheapLLMProvider`), `None` when there are no profiles.
pub(crate) fn build_cheap_llm_selection(
    all_profiles: &[Value],
    cheap_settings: Option<&Value>,
) -> Option<CheapLlmSelection> {
    let profiles: Vec<CheapLlmProfile> = all_profiles
        .iter()
        .map(cheap_llm_profile_from_value)
        .collect();
    let default_index = all_profiles
        .iter()
        .position(|v| v.get("isDefault").and_then(Value::as_bool) == Some(true))
        .unwrap_or(0);
    let default_profile = profiles.get(default_index)?;
    let config = cheap_llm_config_from_settings(cheap_settings);
    Some(get_cheap_llm_provider(
        default_profile,
        &config,
        &profiles,
        false,
        None,
    ))
}

/// Node `Buffer.from(s, 'base64')`: decode standard/URL-safe base64, ignoring any
/// non-alphabet character (whitespace) and tolerating missing padding.
pub(crate) fn decode_base64_node(s: &str) -> Vec<u8> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }
    let symbols: Vec<u8> = s.bytes().filter_map(val).collect();
    let mut out = Vec::with_capacity(symbols.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &sym in &symbols {
        acc = (acc << 6) | sym as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// Load an image profile's `parameters` object by id (v4 reads
/// `reroute.profile.parameters`; the `RouteProfile` doesn't carry them).
pub(crate) async fn load_profile_parameters(db: &Db, profile_id: &str) -> Value {
    let pid = profile_id.to_string();
    db.read_main(move |conn| crate::db::image_profiles::find_by_id(conn, &pid))
        .ok()
        .flatten()
        .and_then(|p| p.get("parameters").cloned())
        .unwrap_or(Value::Null)
}

/// The outcome of [`generate_with_reroute`]: the images + the profile that
/// actually produced them (for the file `generationModel`).
pub(crate) struct GenOutcome {
    pub images: Vec<crate::model::image::GeneratedImageData>,
    #[allow(dead_code)]
    pub active_provider: String,
    pub active_model: String,
}

/// v4 `logLLMCall` (character-avatar.ts / story-background.ts) — one
/// `IMAGE_GENERATION` row per provider attempt. The avatar handler passes a
/// `characterId`; the story handler does not (`character_id = None`). `durationMs`
/// is v4's `Date.now() - genStartTime` — a REAL wall clock bracketing the
/// provider call (the P4.D49 cheap-path pattern; the differentials normalize
/// non-NULL durations, so a measured value stays oracle-neutral). Since v4
/// `0cde7fbc` (the Almanack) it feeds real latency figures, so a hardcoded 0
/// reads as an unmeasured row. Awaited (the writer never throws);
/// `LogContext::none()` on the job path (Unit 4 supplies the autonomous run id
/// later).
#[allow(clippy::too_many_arguments)]
async fn log_image_gen_job(
    db: &Db,
    user_id: &str,
    chat_id: Option<&str>,
    character_id: Option<&str>,
    provider: &str,
    model_name: &str,
    // v4 `0cde7fbc`: `effectiveImageProfile.id` on the primary arms,
    // `reroute.profile.id` on the Concierge-reroute arms.
    image_profile_id: &str,
    prompt: &str,
    content: String,
    error: Option<String>,
    duration_ms: f64,
) {
    let params = LogLlmCallParams {
        user_id: user_id.to_string(),
        log_type: log_type::IMAGE_GENERATION.to_string(),
        message_id: None,
        chat_id: chat_id.map(str::to_string),
        character_id: character_id.map(str::to_string),
        provider: provider.to_string(),
        model_name: model_name.to_string(),
        connection_profile_id: None,
        image_profile_id: Some(image_profile_id.to_string()),
        request: LogRequest {
            messages: vec![LogRequestMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
                attachments: None,
            }],
            temperature: None,
            max_tokens: None,
            tools: None,
        },
        response: LogResponse {
            content,
            error,
            finish_reason: None,
            tool_calls: None,
        },
        usage: None,
        cache_usage: None,
        raw_provider_usage: None,
        request_hashes: None,
        duration_ms: Some(duration_ms),
    };
    let _ = log_llm_call(db, params, &LogContext::none()).await;
}

/// v4 `revisedPrompt || \`Generated ${n} image(s)${suffix}\`` — the success-log
/// content.
fn job_success_content(response: &ImageGenResponse, suffix: &str) -> String {
    response
        .images
        .first()
        .and_then(|i| i.revised_prompt.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Generated {} image(s){suffix}", response.images.len()))
}

/// Which handler is driving [`generate_with_reroute`] — v4's two catch blocks,
/// which differ in what gates their post-hoc reroute.
///
/// [cc65d6bfc] The story path's door is barred for a chat the operator left
/// moderated (bug 133): a background nobody asked for is the wrong place to
/// discover an uncensored provider, and treating a refusal as licence to try a
/// franker one lets the provider's moderation *promote* the chat — the ratchet
/// pointing exactly the wrong way. v4's avatar handler is untouched by that
/// commit: its gate stays the moderation error alone.
///
/// [decd8ef9] The candid re-craft this seam used to carry is GONE, because v4
/// deleted it in the same commit: the reroute is now reachable only for a chat
/// whose prompt was already crafted candidly, so there was nothing left to
/// un-drape and re-crafting here was how a moderated chat got escalated.
#[derive(Debug, Clone, Copy)]
pub(crate) enum RerouteHandler {
    /// v4 `background-jobs/handlers/story-background.ts`.
    StoryBackground {
        /// v4's `rerouteAllowed = moderationRejection && isDangerousChat`
        /// second conjunct — AND a key of the failure bag.
        is_dangerous_chat: bool,
        /// A failure-bag key only (v4 `Boolean(uncensoredImageProfileId)`);
        /// it has not gated the reroute since `cc65d6bfc`, and the resolver
        /// has always re-derived it for itself.
        has_uncensored_image_provider: bool,
    },
    /// v4 `background-jobs/handlers/character-avatar.ts` — no chat gate, and
    /// (measured at `cc65d6bfc`, against this order's survey, which claimed
    /// otherwise) NO `hasUncensoredImageProvider` key in its failure bag.
    CharacterAvatar,
}

impl RerouteHandler {
    /// v4's per-handler second conjunct on the reroute gate. The avatar has
    /// none, which is `true` (`moderationRejection` alone decides).
    fn chat_reroute_allowed(self) -> bool {
        match self {
            RerouteHandler::StoryBackground {
                is_dangerous_chat, ..
            } => is_dangerous_chat,
            RerouteHandler::CharacterAvatar => true,
        }
    }

    /// v4's `logger.error('[…] Image generation failed', {…}, error)` — the
    /// arm taken when no second door opens. Each handler's message, `context`
    /// and bag are v4's own; the story bag gained `rerouteAllowed` and
    /// `isDangerousChat` at `cc65d6bfc`.
    ///
    /// `target:` must be a literal for `tracing`'s static callsite, so the
    /// per-handler lines are spelled out here rather than parameterised.
    fn log_failure(self, job_id: Option<&str>, error: &str, moderation_rejection: bool) {
        match self {
            RerouteHandler::StoryBackground {
                is_dangerous_chat,
                has_uncensored_image_provider,
            } => tracing::error!(
                target: "quilltap::story_background",
                context = "background-jobs.story-background",
                job_id = job_id.unwrap_or(""),
                error = error,
                moderation_rejection = moderation_rejection,
                reroute_allowed = moderation_rejection && is_dangerous_chat,
                is_dangerous_chat = is_dangerous_chat,
                has_uncensored_image_provider = has_uncensored_image_provider,
                "[StoryBackground] Image generation failed"
            ),
            RerouteHandler::CharacterAvatar => tracing::error!(
                target: "quilltap::character_avatar",
                context = "background-jobs.character-avatar",
                job_id = job_id.unwrap_or(""),
                error = error,
                moderation_rejection = moderation_rejection,
                "[CharacterAvatar] Image generation failed"
            ),
        }
    }

    /// v4's `logger.info('[…] Image provider rejected for content moderation,
    /// rerouting through Concierge uncensored profile', {…})` — the same
    /// quartet plus `originalError` in both handlers.
    #[allow(clippy::too_many_arguments)]
    fn log_rerouting(
        self,
        job_id: Option<&str>,
        original_profile_id: &str,
        original_provider: &str,
        fallback_profile_id: &str,
        fallback_provider: &str,
        original_error: &str,
    ) {
        match self {
            RerouteHandler::StoryBackground { .. } => tracing::info!(
                target: "quilltap::story_background",
                context = "background-jobs.story-background",
                job_id = job_id.unwrap_or(""),
                original_profile_id = original_profile_id,
                original_provider = original_provider,
                fallback_profile_id = fallback_profile_id,
                fallback_provider = fallback_provider,
                original_error = original_error,
                "[StoryBackground] Image provider rejected for content moderation, rerouting through Concierge uncensored profile"
            ),
            RerouteHandler::CharacterAvatar => tracing::info!(
                target: "quilltap::character_avatar",
                context = "background-jobs.character-avatar",
                job_id = job_id.unwrap_or(""),
                original_profile_id = original_profile_id,
                original_provider = original_provider,
                fallback_profile_id = fallback_profile_id,
                fallback_provider = fallback_provider,
                original_error = original_error,
                "[CharacterAvatar] Image provider rejected for content moderation, rerouting through Concierge uncensored profile"
            ),
        }
    }

    /// v4's `logger.error('[…] Image generation failed (Concierge reroute also
    /// failed)', { context, jobId, originalError, rerouteError }, rerouteError)`.
    fn log_after_reroute_failure(
        self,
        job_id: Option<&str>,
        original_error: &str,
        reroute_error: &str,
    ) {
        match self {
            RerouteHandler::StoryBackground { .. } => tracing::error!(
                target: "quilltap::story_background",
                context = "background-jobs.story-background",
                job_id = job_id.unwrap_or(""),
                original_error = original_error,
                reroute_error = reroute_error,
                "[StoryBackground] Image generation failed (Concierge reroute also failed)"
            ),
            RerouteHandler::CharacterAvatar => tracing::error!(
                target: "quilltap::character_avatar",
                context = "background-jobs.character-avatar",
                job_id = job_id.unwrap_or(""),
                original_error = original_error,
                reroute_error = reroute_error,
                "[CharacterAvatar] Image generation failed (Concierge reroute also failed)"
            ),
        }
    }
}

/// The generate + post-hoc Concierge reroute flow shared by both handlers.
/// `fail_prefix` is the handler's error prefix (`"Avatar image generation failed"`
/// / `"Image generation failed"`); the after-reroute message is
/// `"{fail_prefix} after Concierge reroute: {msg}"`. Orientation is resolved via
/// the injected registry seam. Each provider attempt writes an `IMAGE_GENERATION`
/// `llm_logs` row (v4 `logLLMCall`) via [`log_image_gen_job`].
#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_with_reroute<I: ImageProvider, A: ApiKeyResolver>(
    db: &Db,
    image_provider: &I,
    api_keys: &A,
    profile_id: &str,
    provider: &str,
    model_name: &str,
    parameters: &Value,
    api_key: &str,
    final_prompt: &str,
    orientation: Orientation,
    declarations_for: &ImageDeclarationsFn,
    danger_mode: &str,
    uncensored_image_profile_id: Option<&str>,
    user_id: &str,
    chat_id: Option<&str>,
    character_id: Option<&str>,
    fail_prefix: &str,
    // v4's `logContext.context` literals for this handler's two attempts
    // (`'background-jobs.character-avatar'` / `'…concierge-reroute'`, and the
    // story-background pair) plus the job id it folds in.
    log_context: &'static str,
    reroute_log_context: &'static str,
    job_id: Option<&str>,
    handler: RerouteHandler,
) -> Result<GenOutcome, String> {
    // The shared builder maps the handler's orientation onto the provider's own
    // size / aspect ratio / prompt wording AND attaches the profile's LoRAs and
    // residual options — the same params the Salon's `generate_image` gets, so
    // a LoRA configured for a profile does not work in chat and quietly vanish
    // here. `style: 'natural'` and `n: 1` stay the handler's fixed choices.
    let params = build_image_gen_params(
        ImageProfileLike {
            provider,
            model_name: Some(model_name),
            parameters: Some(parameters),
        },
        final_prompt,
        &job_overrides(),
        Some(orientation),
        DEFAULT_IMAGE_MODEL,
        &declarations_for(provider),
        &ImageParamsLogContext {
            context: log_context,
            chat_id: chat_id.map(str::to_string),
            job_id: job_id.map(str::to_string),
            profile_id: Some(profile_id.to_string()),
        },
    )
    .params;

    // v4 `const genStartTime = Date.now()` — a real wall-clock read bracketing
    // the provider attempt (NOT the handlers' pinned `now_ms`, which stamps
    // filenames/timestamps and would make every duration structurally 0).
    let gen_start = crate::clock::now_unix_ms();
    match image_provider
        .generate_image(provider, api_key, &params)
        .await
    {
        Ok(response) => {
            let gen_duration_ms = (crate::clock::now_unix_ms() - gen_start) as f64;
            log_image_gen_job(
                db,
                user_id,
                chat_id,
                character_id,
                provider,
                model_name,
                profile_id,
                final_prompt,
                job_success_content(&response, ""),
                None,
                gen_duration_ms,
            )
            .await;
            Ok(GenOutcome {
                images: response.images,
                active_provider: provider.to_string(),
                active_model: model_name.to_string(),
            })
        }
        Err(error) => {
            let gen_duration_ms = (crate::clock::now_unix_ms() - gen_start) as f64;
            log_image_gen_job(
                db,
                user_id,
                chat_id,
                character_id,
                provider,
                model_name,
                profile_id,
                final_prompt,
                String::new(),
                Some(error.message.clone()),
                gen_duration_ms,
            )
            .await;
            reroute_or_fail(
                db,
                image_provider,
                api_keys,
                profile_id,
                provider,
                &error,
                final_prompt,
                orientation,
                declarations_for,
                danger_mode,
                uncensored_image_profile_id,
                user_id,
                chat_id,
                character_id,
                fail_prefix,
                reroute_log_context,
                job_id,
                handler,
            )
            .await
        }
    }
}

/// The post-hoc moderation reroute half of [`generate_with_reroute`].
#[allow(clippy::too_many_arguments)]
async fn reroute_or_fail<I: ImageProvider, A: ApiKeyResolver>(
    db: &Db,
    image_provider: &I,
    api_keys: &A,
    profile_id: &str,
    // v4's `imageProfile.provider` / `effectiveImageProfile.provider` — a
    // rerouting-log field only.
    original_provider: &str,
    error: &ImageGenError,
    final_prompt: &str,
    orientation: Orientation,
    declarations_for: &ImageDeclarationsFn,
    danger_mode: &str,
    uncensored_image_profile_id: Option<&str>,
    user_id: &str,
    chat_id: Option<&str>,
    character_id: Option<&str>,
    fail_prefix: &str,
    reroute_log_context: &'static str,
    job_id: Option<&str>,
    handler: RerouteHandler,
) -> Result<GenOutcome, String> {
    // [cc65d6bfc] v4's `moderationRejection` / `rerouteAllowed` pair, computed
    // ONCE: the story handler's failure bag reports both, and the second
    // conjunct is what bars the door for a moderated chat (bug 133).
    let moderation_rejection = is_image_moderation_error(&error.message);
    let reroute_allowed = moderation_rejection && handler.chat_reroute_allowed();
    let reroute = if reroute_allowed {
        let uid = user_id.to_string();
        let mode = danger_mode.to_string();
        let uncensored = uncensored_image_profile_id.map(str::to_string);
        let current_id = profile_id.to_string();
        db.read_main(move |conn| {
            resolve_uncensored_image_profile_for_reroute(
                conn,
                api_keys,
                &current_id,
                &mode,
                uncensored.as_deref(),
                &uid,
            )
        })
        .ok()
        .flatten()
    } else {
        None
    };

    let Some(reroute) = reroute else {
        handler.log_failure(job_id, &error.message, moderation_rejection);
        return Err(format!("{fail_prefix}: {}", error.message));
    };

    handler.log_rerouting(
        job_id,
        profile_id,
        // v4 logs the ORIGINAL attempt's provider, which is the one the params
        // builder was handed; `reroute_or_fail` is only reached from that arm.
        original_provider,
        &reroute.profile.id,
        &reroute.profile.provider,
        &error.message,
    );

    // [cc65d6bfc] v4 `const rerouteBasePrompt = finalPrompt!;`. The reroute is
    // gated on the chat already being flagged, so the prompt that just got
    // rejected was crafted with `uncensoredImageTarget` set — candid already.
    // It goes to the reroute target as-is; there is nothing left to un-drape,
    // and re-crafting here is how a moderated chat used to get escalated
    // (bug 133), so the [decd8ef9] re-craft seam is deleted with it.
    let reroute_base_prompt = final_prompt.to_string();

    // Rebuild for the reroute provider/model — its shape mechanism, its LoRA
    // support, and its stored options are all its own.
    let reroute_params = load_profile_parameters(db, &reroute.profile.id).await;
    let params = build_image_gen_params(
        ImageProfileLike {
            provider: &reroute.profile.provider,
            model_name: Some(&reroute.profile.model_name),
            parameters: Some(&reroute_params),
        },
        &reroute_base_prompt,
        &job_overrides(),
        Some(orientation),
        DEFAULT_IMAGE_MODEL,
        &declarations_for(&reroute.profile.provider),
        &ImageParamsLogContext {
            context: reroute_log_context,
            chat_id: chat_id.map(str::to_string),
            job_id: job_id.map(str::to_string),
            profile_id: Some(reroute.profile.id.clone()),
        },
    )
    .params;

    // v4 `const rerouteStartTime = Date.now()` — the reroute attempt gets its
    // own wall-clock span.
    let reroute_start = crate::clock::now_unix_ms();
    match image_provider
        .generate_image(&reroute.profile.provider, &reroute.api_key, &params)
        .await
    {
        Ok(response) => {
            let reroute_duration_ms = (crate::clock::now_unix_ms() - reroute_start) as f64;
            log_image_gen_job(
                db,
                user_id,
                chat_id,
                character_id,
                &reroute.profile.provider,
                &reroute.profile.model_name,
                &reroute.profile.id,
                &reroute_base_prompt,
                job_success_content(&response, " (Concierge reroute)"),
                None,
                reroute_duration_ms,
            )
            .await;
            Ok(GenOutcome {
                images: response.images,
                active_provider: reroute.profile.provider.clone(),
                active_model: reroute.profile.model_name.clone(),
            })
        }
        Err(reroute_error) => {
            let reroute_duration_ms = (crate::clock::now_unix_ms() - reroute_start) as f64;
            log_image_gen_job(
                db,
                user_id,
                chat_id,
                character_id,
                &reroute.profile.provider,
                &reroute.profile.model_name,
                &reroute.profile.id,
                &reroute_base_prompt,
                String::new(),
                Some(reroute_error.message.clone()),
                reroute_duration_ms,
            )
            .await;
            handler.log_after_reroute_failure(job_id, &error.message, &reroute_error.message);
            Err(format!(
                "{fail_prefix} after Concierge reroute: {}",
                reroute_error.message
            ))
        }
    }
}

/// v4 `resolveDangerousContentSettings` on a chat + global settings — a thin
/// wrapper that pulls the global `dangerousContentSettings` off a chat-settings
/// `Value` and resolves against the chat. Shared by both handlers.
pub(crate) fn resolve_danger_settings_for_chat(
    chat_settings: Option<&Value>,
    chat: &Value,
) -> crate::db::chat_settings::DangerousContentSettings {
    let global = chat_settings
        .and_then(|cs| cs.get("dangerousContentSettings"))
        .and_then(|d| {
            serde_json::from_value::<crate::db::chat_settings::DangerousContentSettings>(d.clone())
                .ok()
        });
    crate::services::dangerous_content::resolver::resolve_dangerous_content_settings(
        global,
        Some(chat),
    )
    .settings
}

/// The project-store `fileStorageManager.uploadFile` seam (the host FsSeam). The
/// handlers land project-scoped images through this; the corpus keeps the
/// database-backed (vault / Lantern) branches primary with a recorded project
/// case. Async — the real upload is a host call. `folder_path` is `/character-
/// avatars/` (avatar) or `/story-backgrounds/` (story). `Err` is v4
/// `uploadFile`'s throw — the handlers propagate it and the job FAILS before
/// the `files` row / avatar update / Lantern announcement (dogfood finding
/// #16: the old infallible shape buried the error in a sentinel storageKey
/// and let the job "succeed" with an unservable file).
pub trait ProjectImageUpload {
    fn upload(
        &self,
        filename: &str,
        content: &[u8],
        content_type: &str,
        project_id: &str,
        folder_path: &str,
    ) -> impl std::future::Future<Output = Result<ProjectUploadResult, String>> + Send;
}

/// The upload seam's result (v4 `uploadFile`'s `{ storageKey, storedMimeType,
/// sizeBytes }`).
#[derive(Clone, Debug)]
pub struct ProjectUploadResult {
    pub storage_key: String,
    pub stored_mime_type: String,
    pub size_bytes: usize,
}

/// The off-path default upload seam (no project-store host wired). Returns a
/// deterministic placeholder — only reached on the project branch, which the
/// primary corpus does not exercise.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoProjectImageUpload;
impl ProjectImageUpload for NoProjectImageUpload {
    async fn upload(
        &self,
        _filename: &str,
        content: &[u8],
        content_type: &str,
        _project_id: &str,
        _folder_path: &str,
    ) -> Result<ProjectUploadResult, String> {
        Ok(ProjectUploadResult {
            storage_key: "fs-seam:unwired".to_string(),
            stored_mime_type: content_type.to_string(),
            size_bytes: content.len(),
        })
    }
}

/// Read a JSON string field, `None` when absent/null/non-string.
pub(crate) fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// Read a JSON string field into an owned `String` when present & non-empty.
pub(crate) fn owned_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Read a Connection instance for both DBs and run a closure needing both (via a
/// serialized write on the writer thread — the established dual-read pattern).
pub(crate) async fn with_both_conns<T, F>(db: &Db, f: F) -> Result<T, crate::db::DbError>
where
    T: Send + 'static,
    F: FnOnce(&Connection, &Connection) -> Result<T, crate::db::DbError> + Send + 'static,
{
    db.write(move |writers| {
        let mount = writers
            .mount_index()
            .ok_or_else(|| {
                crate::db::DbError::Internal(
                    "image job requires the mount-index database".to_string(),
                )
            })?
            .connection();
        let main = writers.main().connection();
        f(main, mount)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::db::Writer;
    use crate::image_gen::Orientation;
    use crate::model::image::GeneratedImageData;
    use crate::services::dangerous_content::provider_routing::NoApiKeys;

    // === The f7f1a956-round §3 finding: `durationMs` must be a MEASURED span ===
    //
    // The differentials cannot see this — `normalize_duration_ms` collapses
    // every non-NULL duration to a placeholder, which is how a hardcoded
    // `Some(0.0)` survived differential-verified for a whole phase. Since v4
    // `0cde7fbc` (the Almanack) `durationMs` feeds real latency figures
    // (`durationMs > 0` filters, averages, medians), so these pins assert the
    // two properties no oracle diff can: the row's duration is PRESENT, and the
    // span BRACKETS the provider call (a provider that takes a real ~30 ms must
    // produce a duration in that ballpark, not 0).

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A provider that takes a real ~30 ms and then answers (or fails with a
    /// NON-moderation error, so no reroute is attempted).
    struct SlowImageProvider {
        fail: bool,
    }

    impl ImageProvider for SlowImageProvider {
        fn generate_image(
            &self,
            _provider: &str,
            _api_key: &str,
            _params: &crate::model::image::ImageGenParams,
        ) -> impl std::future::Future<Output = Result<ImageGenResponse, ImageGenError>> + Send
        {
            let fail = self.fail;
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                if fail {
                    Err(ImageGenError {
                        message: "provider exploded".to_string(),
                    })
                } else {
                    Ok(ImageGenResponse {
                        images: vec![GeneratedImageData {
                            data: Some("aGk=".to_string()),
                            url: None,
                            mime_type: Some("image/png".to_string()),
                            revised_prompt: None,
                        }],
                    })
                }
            }
        }
    }

    fn open_db(dir: &std::path::Path) -> Db {
        let main_path = dir.join("main.db");
        let ll_path = dir.join("llm-logs.db");
        drop(Writer::open_writable(&main_path, PEPPER).unwrap());
        {
            let w = Writer::open_writable(&ll_path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE llm_logs (\
                       id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
                       chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
                       modelName TEXT, connectionProfileId TEXT, imageProfileId TEXT, \
                       request TEXT, response TEXT, usage TEXT, \
                       cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
                       durationMs REAL, createdAt TEXT, updatedAt TEXT);",
                )
                .unwrap();
        }
        Db::open(
            DbPaths {
                main: main_path,
                mount_index: None,
                llm_logs: Some(ll_path),
            },
            PEPPER,
        )
        .unwrap()
    }

    async fn logged_durations(db: &Db) -> Vec<Option<f64>> {
        db.read_llm_logs(|conn| {
            let mut stmt = conn.prepare("SELECT durationMs FROM llm_logs")?;
            let out = stmt
                .query_map([], |row| row.get::<_, Option<f64>>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(out)
        })
        .unwrap()
    }

    async fn run_gen(db: &Db, fail: bool) -> Result<GenOutcome, String> {
        let declarations: Box<ImageDeclarationsFn> =
            Box::new(|_p: &str| ImageDeclarations::default());
        generate_with_reroute(
            db,
            &SlowImageProvider { fail },
            &NoApiKeys,
            "profile-1",
            "OPENAI",
            "gpt-image-1",
            &Value::Null,
            "sk-test",
            "a prompt",
            Orientation::Square,
            &declarations,
            "OFF",
            None,
            "user-1",
            None,
            None,
            "Image generation failed",
            "test.image-job",
            "test.image-job.concierge-reroute",
            None,
            RerouteHandler::CharacterAvatar,
        )
        .await
    }

    // === [cc65d6bfc] bug 133: the reroute-path log lines (Tier 1 item 5) ===
    //
    // v4 announces at all three points of its two catch blocks; v5's whole
    // reroute path was SILENT (the #103/#110 class — a moderated chat's
    // backdrop simply did not appear, with nothing in the log saying why).
    // The differentials cannot see a log line, so these are the proof.
    //
    // A `provider` whose failure IS a moderation rejection, so the gate's
    // second conjunct is what decides.
    struct BlockedImageProvider {
        /// The FIRST attempt's error. A moderation rejection unless a test is
        /// asking what a plain provider error does.
        first_error: &'static str,
        /// The reroute attempt (the SECOND call) fails too when set.
        fail_reroute: bool,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ImageProvider for BlockedImageProvider {
        fn generate_image(
            &self,
            _provider: &str,
            _api_key: &str,
            _params: &crate::model::image::ImageGenParams,
        ) -> impl std::future::Future<Output = Result<ImageGenResponse, ImageGenError>> + Send
        {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let fail_reroute = self.fail_reroute;
            let first_error = self.first_error;
            async move {
                if n == 0 {
                    return Err(ImageGenError {
                        message: first_error.to_string(),
                    });
                }
                if fail_reroute {
                    return Err(ImageGenError {
                        message: "the second door slammed too".to_string(),
                    });
                }
                Ok(ImageGenResponse {
                    images: vec![GeneratedImageData {
                        data: Some("aGk=".to_string()),
                        url: None,
                        mime_type: Some("image/png".to_string()),
                        revised_prompt: None,
                    }],
                })
            }
        }
    }

    /// An `api_keys`-free resolver that always answers, so the reroute resolves
    /// on the strength of the seeded `image_profiles` row alone.
    struct AlwaysApiKey;
    impl ApiKeyResolver for AlwaysApiKey {
        fn resolve(&self, _api_key_id: &str, _user_id: &str) -> Option<String> {
            Some("sk-reroute".to_string())
        }
    }

    const UNCENSORED_ID: &str = "e5000000-0000-4000-8000-000000000004";

    /// Seed the one `image_profiles` row the reroute resolver reads.
    async fn seed_uncensored_profile(db: &Db) {
        db.write(|w| {
            let conn = w.main().connection();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS image_profiles (\
                   id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT, \
                   apiKeyId TEXT, baseUrl TEXT, modelName TEXT, parameters TEXT, \
                   isDefault INTEGER, isDangerousCompatible INTEGER, tags TEXT, \
                   createdAt TEXT, updatedAt TEXT);",
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO image_profiles VALUES \
                   (?1, 'user-1', 'Uncensored Images', 'OPENAI', 'key-1', NULL, \
                    'uncensored-model', '{}', 0, 1, '[]', '2026-01-01', '2026-01-01')",
                [UNCENSORED_ID],
            )?;
            Ok(())
        })
        .await
        .expect("seed the uncensored image profile");
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_blocked(
        db: &Db,
        handler: RerouteHandler,
        uncensored: Option<&str>,
        fail_reroute: bool,
    ) -> (Result<GenOutcome, String>, usize) {
        run_blocked_with(
            db,
            handler,
            uncensored,
            fail_reroute,
            "content policy violation on this prompt",
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_blocked_with(
        db: &Db,
        handler: RerouteHandler,
        uncensored: Option<&str>,
        fail_reroute: bool,
        first_error: &'static str,
    ) -> (Result<GenOutcome, String>, usize) {
        let declarations: Box<ImageDeclarationsFn> =
            Box::new(|_p: &str| ImageDeclarations::default());
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let out = generate_with_reroute(
            db,
            &BlockedImageProvider {
                first_error,
                fail_reroute,
                calls: calls.clone(),
            },
            &AlwaysApiKey,
            "profile-1",
            "OPENAI",
            "blocked-model",
            &Value::Null,
            "sk-test",
            "a prompt",
            Orientation::Square,
            &declarations,
            "AUTO_ROUTE",
            uncensored,
            "user-1",
            None,
            None,
            "Image generation failed",
            "test.image-job",
            "test.image-job.concierge-reroute",
            Some("job-7"),
            handler,
        )
        .await;
        (out, calls.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// The whole point of bug 133: a MODERATED chat's refusal ends the matter.
    /// v4's failure bag gained `rerouteAllowed` and `isDangerousChat`, and the
    /// resolver is never even asked (one provider call, not two).
    #[tokio::test]
    async fn moderated_story_failure_logs_the_bug_133_keys_and_never_reroutes() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        seed_uncensored_profile(&db).await;
        let handler = RerouteHandler::StoryBackground {
            is_dangerous_chat: false,
            has_uncensored_image_provider: true,
        };
        let (out, calls, lines) = {
            let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let subscriber = {
                use tracing_subscriber::layer::SubscriberExt;
                tracing_subscriber::registry().with(crate::test_support::CaptureLayer(logs.clone()))
            };
            let guard = tracing::subscriber::set_default(subscriber);
            let (out, calls) = run_blocked(&db, handler, Some(UNCENSORED_ID), false).await;
            drop(guard);
            let lines = logs.lock().unwrap().clone();
            (out, calls, lines)
        };

        assert_eq!(
            out.err().as_deref(),
            Some("Image generation failed: content policy violation on this prompt"),
            "a moderated chat's refused backdrop fails the job"
        );
        assert_eq!(calls, 1, "the second door was never opened");

        let failure = one_line(&lines, "[StoryBackground] Image generation failed");
        assert!(
            failure.starts_with("ERROR quilltap::story_background"),
            "level + target: {failure}"
        );
        for field in [
            "context=background-jobs.story-background",
            "job_id=job-7",
            "error=content policy violation on this prompt",
            "moderation_rejection=true",
            "reroute_allowed=false",
            "is_dangerous_chat=false",
            "has_uncensored_image_provider=true",
        ] {
            assert!(failure.contains(field), "missing {field} in: {failure}");
        }
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("rerouting through Concierge")),
            "no rerouting line when the door is barred: {lines:?}"
        );
    }

    /// A FLAGGED chat still goes through, and both handlers announce it with
    /// v4's quartet plus `originalError`.
    #[tokio::test]
    async fn flagged_story_reroute_announces_the_second_door() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        seed_uncensored_profile(&db).await;
        let handler = RerouteHandler::StoryBackground {
            is_dangerous_chat: true,
            has_uncensored_image_provider: true,
        };
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let (out, calls) = {
            use tracing_subscriber::layer::SubscriberExt;
            let subscriber = tracing_subscriber::registry()
                .with(crate::test_support::CaptureLayer(logs.clone()));
            let guard = tracing::subscriber::set_default(subscriber);
            let r = run_blocked(&db, handler, Some(UNCENSORED_ID), false).await;
            drop(guard);
            r
        };
        let lines = logs.lock().unwrap().clone();

        assert!(out.is_ok(), "the reroute produced an image");
        assert_eq!(calls, 2, "original attempt + reroute attempt");

        let info = one_line(
            &lines,
            "[StoryBackground] Image provider rejected for content moderation, rerouting through Concierge uncensored profile",
        );
        assert!(
            info.starts_with("INFO quilltap::story_background"),
            "level + target: {info}"
        );
        for field in [
            "context=background-jobs.story-background",
            "job_id=job-7",
            "original_profile_id=profile-1",
            "original_provider=OPENAI",
            &format!("fallback_profile_id={UNCENSORED_ID}"),
            "fallback_provider=OPENAI",
            "original_error=content policy violation on this prompt",
        ] {
            assert!(info.contains(field), "missing {field} in: {info}");
        }
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("[StoryBackground] Image generation failed")),
            "no failure line on a successful reroute: {lines:?}"
        );
    }

    /// v4's third line: the reroute target refused too.
    #[tokio::test]
    async fn a_failed_reroute_logs_both_errors() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        seed_uncensored_profile(&db).await;
        let handler = RerouteHandler::StoryBackground {
            is_dangerous_chat: true,
            has_uncensored_image_provider: true,
        };
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let out = {
            use tracing_subscriber::layer::SubscriberExt;
            let subscriber = tracing_subscriber::registry()
                .with(crate::test_support::CaptureLayer(logs.clone()));
            let guard = tracing::subscriber::set_default(subscriber);
            let (out, _) = run_blocked(&db, handler, Some(UNCENSORED_ID), true).await;
            drop(guard);
            out
        };
        let lines = logs.lock().unwrap().clone();

        assert_eq!(
            out.err().as_deref(),
            Some("Image generation failed after Concierge reroute: the second door slammed too")
        );
        let line = one_line(
            &lines,
            "[StoryBackground] Image generation failed (Concierge reroute also failed)",
        );
        assert!(line.starts_with("ERROR quilltap::story_background"));
        assert!(line.contains("original_error=content policy violation on this prompt"));
        assert!(line.contains("reroute_error=the second door slammed too"));
    }

    /// v4's avatar handler is UNTOUCHED by bug 133: no chat gate, and its
    /// failure bag carries `moderationRejection` and nothing else new. Its
    /// three lines are the same three sentences under its own name.
    #[tokio::test]
    async fn the_avatar_handler_logs_its_own_three_sentences() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        // Reroute path: the resolver finds the profile, so both lines fire.
        seed_uncensored_profile(&db).await;
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        {
            use tracing_subscriber::layer::SubscriberExt;
            let subscriber = tracing_subscriber::registry()
                .with(crate::test_support::CaptureLayer(logs.clone()));
            let guard = tracing::subscriber::set_default(subscriber);
            let (out, calls) = run_blocked(
                &db,
                RerouteHandler::CharacterAvatar,
                Some(UNCENSORED_ID),
                true,
            )
            .await;
            drop(guard);
            assert!(out.is_err());
            assert_eq!(calls, 2, "the avatar's door is NOT barred by a chat state");
        }
        let lines = logs.lock().unwrap().clone();
        let info = one_line(
            &lines,
            "[CharacterAvatar] Image provider rejected for content moderation, rerouting through Concierge uncensored profile",
        );
        assert!(info.starts_with("INFO quilltap::character_avatar"));
        assert!(info.contains("context=background-jobs.character-avatar"));
        let after = one_line(
            &lines,
            "[CharacterAvatar] Image generation failed (Concierge reroute also failed)",
        );
        assert!(after.starts_with("ERROR quilltap::character_avatar"));

        // And the avatar's own failure arm, with NO uncensored profile to find:
        // v4's bag is `{context, jobId, error, moderationRejection}` — measured
        // at `cc65d6bfc` against this order's survey, which claimed it also
        // carried `hasUncensoredImageProvider`. It does not.
        let dir2 = tempfile::tempdir().unwrap();
        let db2 = open_db(dir2.path());
        let logs2 = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        {
            use tracing_subscriber::layer::SubscriberExt;
            let subscriber = tracing_subscriber::registry()
                .with(crate::test_support::CaptureLayer(logs2.clone()));
            let guard = tracing::subscriber::set_default(subscriber);
            let (out, _) = run_blocked(&db2, RerouteHandler::CharacterAvatar, None, false).await;
            drop(guard);
            assert!(out.is_err());
        }
        let lines2 = logs2.lock().unwrap().clone();
        let failure = one_line(&lines2, "[CharacterAvatar] Image generation failed");
        assert!(failure.contains("moderation_rejection=true"));
        assert!(
            !failure.contains("has_uncensored_image_provider"),
            "v4's avatar bag has no such key: {failure}"
        );
        assert!(
            !failure.contains("reroute_allowed"),
            "nor `rerouteAllowed` — that local is the story handler's: {failure}"
        );
    }

    /// The silence arm — and the ONLY thing that measures the gate's FIRST
    /// conjunct. Everything else is arranged so the reroute would be refused
    /// downstream anyway (mode OFF, or no profile), which makes dropping
    /// `moderationRejection` invisible. Here the chat is flagged, the mode is
    /// AUTO_ROUTE and an uncensored profile IS configured — so the door is one
    /// conjunct away from opening, and only "this was not a moderation
    /// rejection" keeps it shut. A gate reading `chat_reroute_allowed()` alone
    /// reroutes a plain provider error and reddens this arm.
    #[tokio::test]
    async fn a_non_moderation_failure_reports_it_and_never_opens_the_second_door() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        seed_uncensored_profile(&db).await;
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let (out, calls) = {
            use tracing_subscriber::layer::SubscriberExt;
            let subscriber = tracing_subscriber::registry()
                .with(crate::test_support::CaptureLayer(logs.clone()));
            let guard = tracing::subscriber::set_default(subscriber);
            let r = run_blocked_with(
                &db,
                RerouteHandler::StoryBackground {
                    is_dangerous_chat: true,
                    has_uncensored_image_provider: true,
                },
                Some(UNCENSORED_ID),
                false,
                "the provider exploded",
            )
            .await;
            drop(guard);
            r
        };
        let lines = logs.lock().unwrap().clone();

        assert_eq!(
            out.err().as_deref(),
            Some("Image generation failed: the provider exploded"),
            "a plain provider error fails the job outright"
        );
        assert_eq!(calls, 1, "the second door stayed shut");
        let failure = one_line(&lines, "[StoryBackground] Image generation failed");
        assert!(
            failure.contains("moderation_rejection=false"),
            "a plain provider error is not a moderation rejection: {failure}"
        );
        assert!(
            failure.contains("reroute_allowed=false"),
            "and so the reroute is not allowed, flagged chat or no: {failure}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("rerouting through Concierge")),
            "nothing to reroute: {lines:?}"
        );
    }

    /// Exactly one captured line contains `needle`; hand it back.
    fn one_line<'a>(lines: &'a [String], needle: &str) -> &'a str {
        let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one line containing {needle:?}, got {hits:?} (all: {lines:?})"
        );
        hits[0]
    }

    #[tokio::test]
    async fn success_row_duration_brackets_the_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let out = run_gen(&db, false).await;
        assert!(out.is_ok());
        let durations = logged_durations(&db).await;
        assert_eq!(durations.len(), 1, "exactly one llm_logs row written");
        let d = durations[0].expect("durationMs must be present");
        assert!(
            d >= 20.0,
            "durationMs must bracket the ~30 ms provider call, got {d}"
        );
    }

    #[tokio::test]
    async fn failure_row_duration_brackets_the_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let out = run_gen(&db, true).await;
        assert!(out.is_err());
        let durations = logged_durations(&db).await;
        assert_eq!(durations.len(), 1, "exactly one llm_logs row written");
        let d = durations[0].expect("durationMs must be present");
        assert!(
            d >= 20.0,
            "durationMs must bracket the ~30 ms provider call, got {d}"
        );
    }
}
