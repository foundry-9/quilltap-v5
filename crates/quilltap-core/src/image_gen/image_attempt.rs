//! One image-provider call, logged — and what comes back, decoded. v4
//! `lib/image-gen/image-attempt.ts` (`9ecebf986`, P4.D283).
//!
//! Every path that draws a picture (the avatar job, the Lantern's story
//! background, a wardrobe item's picture, the `generate_image` tool, the
//! avatar preview) hands `generate_image_with_concierge_failover` an `attempt`
//! closure that does the same four things against whichever profile the
//! chokepoint asks: create the provider, build that profile's params through
//! `build_image_gen_params`, call it, and write one `llm_logs` row whether it
//! answered or threw. [`make_logged_image_attempt`] is that closure, once —
//! replacing the four hand-rolled copies v5 carried (`image_job_common`'s, the
//! tool's, the wardrobe generation's, the host preview's).
//!
//! [`decode_provider_image`] is the tail every one of them then ran: take
//! `images[n]`, decode its base64, transcode to WebP (measuring the real
//! dimensions — providers often return a different shape than was asked) and
//! hash the stored bytes.
//!
//! **The log type is decided here.** A picture's row is `IMAGE_GENERATION`
//! unless it is a wardrobe item's picture, which keeps its own
//! `WARDROBE_ITEM_IMAGE` so the LLM Inspector can tell the two apart. Both are
//! image spend: [`crate::services::llm_logging::IMAGE_SPEND_LOG_TYPES`] is the
//! list the Almanack's per-image-profile roll-up filters on (bug 196 — wardrobe
//! pictures used to vanish from it). A new image log type joins that tuple,
//! and with it the spend, or it cannot be passed here at all.
//!
//! **Log-context delta at every site (v4 `9ecebf986`):** the pre-span
//! `.concierge-route` / `.concierge-reroute` CONTEXT STRINGS are gone; one
//! context per site, and the params builder's own lines carry `profileId` +
//! `rerouted` instead (`ImageParamsLogContext.rerouted`).
//!
//! v4's `[ImageAttempt] Failed to record the image call in llm_logs` WARN
//! guards a throw out of `logLLMCall` — which swallows its own errors, as
//! v5's `log_llm_call` does (it never returns an error). The arm is
//! UNREACHABLE on both sides and is pinned NEGATIVE by `image_attempt_tier3`.

use std::future::Future;

use serde_json::Value;

use crate::db::runtime::Db;
use crate::image_gen::params_builder::{
    build_image_gen_params, ImageDeclarationsFn, ImageGenOverrides, ImageParamsLogContext,
    ImageProfileLike,
};
use crate::image_gen::Orientation;
use crate::model::image::{
    ImageGenError, ImageGenParams, ImageGenResponse, ImageProvider, ImageTranscoder, TranscodeInput,
};
use crate::services::dangerous_content::image_failover::FailoverProfile;
use crate::services::llm_logging::{
    log_llm_call, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage, LogResponse,
};

const LOG_CONTEXT: &str = "image-gen.image-attempt";

/// v4 `DEFAULT_IMAGE_MODEL` — the params builder's model fallback.
pub const DEFAULT_IMAGE_MODEL: &str = "dall-e-3";

/// The per-call shape v4 hands `buildImageGenParams` (its
/// `LoggedImageAttemptOptions.params` — `Omit<BuildImageGenParamsOptions,
/// 'profile' | 'prompt'>`): overrides, orientation, the log context (the
/// attempt adds `profileId` + `rerouted` itself), plus the two things v5's
/// builder takes explicitly where v4 reads module state (the fallback model
/// and the provider declarations).
pub struct ImageAttemptParams<'a> {
    pub overrides: ImageGenOverrides,
    pub orientation: Option<Orientation>,
    /// v4 `logContext` minus the attempt's own `profileId` / `rerouted`.
    pub log_context: ImageParamsLogContext,
    pub fallback_model: &'a str,
    pub declarations_for: &'a ImageDeclarationsFn,
}

/// Params already built for one profile — the avatar job derives its cache
/// key from them — reused verbatim when that profile is the one asked. Every
/// other profile builds its own: its shape mechanism, LoRAs and stored options
/// are its own.
#[derive(Clone, Debug)]
pub struct PrebuiltImageParams {
    pub profile_id: String,
    pub params: ImageGenParams,
}

/// v4 `LoggedImageAttemptOptions`.
pub struct LoggedImageAttemptOptions<'a> {
    pub user_id: &'a str,
    /// `IMAGE_GENERATION`, or `WARDROBE_ITEM_IMAGE` for a wardrobe item's
    /// picture (`services::llm_logging::IMAGE_SPEND_LOG_TYPES`).
    pub log_type: &'static str,
    /// The final prompt — what the provider is sent and what the log records.
    pub prompt: &'a str,
    /// The profile first asked for. Any other profile the chokepoint tries is
    /// a Concierge reroute, and its log line and `logContext` say so.
    pub primary_profile_id: &'a str,
    pub chat_id: Option<&'a str>,
    pub character_id: Option<&'a str>,
    pub params: ImageAttemptParams<'a>,
    pub prebuilt: Option<PrebuiltImageParams>,
}

/// The stored `parameters` of the profile the chokepoint handed over: off its
/// row when the row carries them, else read by id (v4's `profile.parameters`
/// is always on the object — v5's `FailoverProfile.row` is `Null` where a job
/// built the primary from loose fields).
fn profile_parameters(db: &Db, profile: &FailoverProfile) -> Value {
    match profile.row.get("parameters") {
        Some(p) => p.clone(),
        None => crate::services::image_job_common::load_profile_parameters(db, &profile.id),
    }
}

/// v4 `makeLoggedImageAttempt(options)` — the attempt closure for one picture,
/// as a value: callers hand the chokepoint `|profile, key| attempt.call(profile,
/// key)`. (A struct rather than a returned closure because stable Rust cannot
/// name `impl FnMut(..) -> impl Future`, and a boxed future would fix its
/// `Send`-ness; this way the future's auto traits leak — `Send` exactly when
/// the provider is `Sync`, which the erased host provider is and a job's
/// generic `I` need not be.) Logging never fails the call: `log_llm_call`
/// swallows its own errors.
pub struct LoggedImageAttempt<'a, P> {
    db: &'a Db,
    provider: &'a P,
    options: LoggedImageAttemptOptions<'a>,
}

/// v4 `makeLoggedImageAttempt(options)`.
pub fn make_logged_image_attempt<'a, P: ImageProvider>(
    db: &'a Db,
    provider: &'a P,
    options: LoggedImageAttemptOptions<'a>,
) -> LoggedImageAttempt<'a, P> {
    LoggedImageAttempt {
        db,
        provider,
        options,
    }
}

impl<'a, P: ImageProvider> LoggedImageAttempt<'a, P> {
    /// One call against `profile` — v4's `async (profile, apiKey) => …`: the
    /// params (prebuilt for the profile first asked, built for any other with
    /// `profileId` + `rerouted` on the builder's line), the DEBUG, the provider
    /// call, the one `llm_logs` row on either arm.
    pub fn call(
        &mut self,
        profile: FailoverProfile,
        api_key: String,
    ) -> impl Future<Output = Result<ImageGenResponse, ImageGenError>> + 'a {
        let o = &self.options;
        // v4 `rerouted = profile.id !== primaryProfileId`.
        let rerouted = profile.id != o.primary_profile_id;
        let reused_prebuilt = o
            .prebuilt
            .as_ref()
            .is_some_and(|p| p.profile_id == profile.id);
        let params = match o.prebuilt.as_ref().filter(|p| p.profile_id == profile.id) {
            Some(p) => p.params.clone(),
            None => {
                let parameters = profile_parameters(self.db, &profile);
                build_image_gen_params(
                    ImageProfileLike {
                        provider: &profile.provider,
                        model_name: Some(&profile.model_name),
                        parameters: Some(&parameters),
                    },
                    o.prompt,
                    &o.params.overrides,
                    o.params.orientation,
                    o.params.fallback_model,
                    &(o.params.declarations_for)(&profile.provider),
                    &ImageParamsLogContext {
                        context: o.params.log_context.context,
                        chat_id: o.params.log_context.chat_id.clone(),
                        job_id: o.params.log_context.job_id.clone(),
                        profile_id: Some(profile.id.clone()),
                        rerouted: Some(rerouted),
                    },
                )
                .params
            }
        };

        tracing::debug!(
            context = LOG_CONTEXT,
            logType = o.log_type,
            profileId = %profile.id,
            provider = %profile.provider,
            model = %profile.model_name,
            rerouted = rerouted,
            reusedPrebuiltParams = reused_prebuilt,
            "[ImageAttempt] Calling image provider"
        );

        run_attempt(
            self.db,
            self.provider,
            o.user_id,
            o.log_type,
            o.prompt,
            o.chat_id.map(str::to_string),
            o.character_id.map(str::to_string),
            profile,
            api_key,
            params,
            rerouted,
        )
    }
}

/// The async half of one attempt: the provider call and v4's `record(profile,
/// startedAt, response)` — awaited on both arms, so the row lands BEFORE the
/// caller's `files` row.
#[allow(clippy::too_many_arguments)]
async fn run_attempt<'a, P: ImageProvider>(
    db: &'a Db,
    provider: &'a P,
    user_id: &'a str,
    log_type: &'static str,
    prompt: &'a str,
    chat_id: Option<String>,
    character_id: Option<String>,
    profile: FailoverProfile,
    api_key: String,
    params: ImageGenParams,
    rerouted: bool,
) -> Result<ImageGenResponse, ImageGenError> {
    // v4 `const startedAt = Date.now()` — a real wall-clock read bracketing
    // the provider attempt.
    let started_at = crate::clock::now_unix_ms();
    let result = provider
        .generate_image(&profile.provider, &api_key, &params)
        .await;
    let (content, error) = match &result {
        Ok(response) => (success_content(response, rerouted), None),
        Err(e) => (String::new(), Some(e.message.clone())),
    };
    log_llm_call(
        db,
        LogLlmCallParams {
            user_id: user_id.to_string(),
            log_type: log_type.to_string(),
            message_id: None,
            chat_id,
            character_id,
            provider: profile.provider.clone(),
            model_name: profile.model_name.clone(),
            connection_profile_id: None,
            image_profile_id: Some(profile.id.clone()),
            request: LogRequest {
                messages: vec![LogRequestMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                    attachments: None,
                }],
                ..Default::default()
            },
            response: LogResponse {
                content,
                error,
                ..Default::default()
            },
            usage: None,
            cache_usage: None,
            raw_provider_usage: None,
            request_hashes: None,
            duration_ms: Some((crate::clock::now_unix_ms() - started_at) as f64),
        },
        &LogContext::none(),
    )
    .await;
    result
}

/// v4 `response.images?.[0]?.revisedPrompt || \`Generated ${n} image(s)${rerouted
/// ? ' (Concierge reroute)' : ''}\`` — the success row's content (an empty
/// first-image `revisedPrompt` falls back to the count text).
pub fn success_content(response: &ImageGenResponse, rerouted: bool) -> String {
    response
        .images
        .first()
        .and_then(|i| i.revised_prompt.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            format!(
                "Generated {} image(s){}",
                response.images.len(),
                if rerouted { " (Concierge reroute)" } else { "" }
            )
        })
}

/// A provider's picture, ready to store (v4 `DecodedProviderImage`).
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedProviderImage {
    /// The stored bytes: WebP when the provider's format converts, else as sent.
    pub buffer: Vec<u8>,
    pub mime_type: String,
    /// `<stem>_<epoch ms>.<ext>`, extension following the stored format.
    pub filename: String,
    /// Measured from the stored bytes; `None` when unmeasurable (SVG, a decode
    /// failure).
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub sha256: String,
    pub revised_prompt: Option<String>,
}

/// v4 `decodeProviderImage(response, filenameStem, index = 0)` — decode
/// `response.images[index]` into storable bytes. `None` when the provider sent
/// no such image, or one with no inline data (a URL-only answer is the
/// provider plugin's to download, never the caller's). `now_ms` is v4's
/// `Date.now()` in the minted filename (the jobs pass their pinned clock).
pub fn decode_provider_image(
    transcoder: &dyn ImageTranscoder,
    response: Option<&ImageGenResponse>,
    filename_stem: &str,
    index: usize,
    now_ms: i64,
) -> Option<DecodedProviderImage> {
    let image = response.and_then(|r| r.images.get(index));
    // v4 `raw = image?.data || image?.b64Json` — the decoders fold both vendor
    // spellings into `data`; an empty string is falsy.
    let raw = image.and_then(|i| i.data.as_deref().filter(|s| !s.is_empty()));
    let (Some(image), Some(raw)) = (image, raw) else {
        tracing::debug!(
            context = LOG_CONTEXT,
            index = index,
            imageCount = response.map(|r| r.images.len()).unwrap_or(0),
            hasUrl = image.is_some_and(|i| i.url.as_deref().is_some_and(|u| !u.is_empty())),
            "[ImageAttempt] Provider response carried no image data"
        );
        return None;
    };

    // `providerMimeType = image.mimeType || 'image/png'`;
    // `providerExt = providerMimeType.split('/')[1] || 'png'`.
    let provider_mime_type = image
        .mime_type
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("image/png")
        .to_string();
    let provider_ext = provider_mime_type
        .split('/')
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or("png");
    let converted = transcoder.transcode(&TranscodeInput {
        bytes: crate::services::image_job_common::decode_base64_node(raw),
        mime_type: provider_mime_type.clone(),
        filename: format!("{filename_stem}_{now_ms}.{provider_ext}"),
    });

    let decoded = DecodedProviderImage {
        sha256: crate::photos::keep_image_markdown::sha256_of_buffer(&converted.bytes),
        buffer: converted.bytes,
        mime_type: converted.mime_type,
        filename: converted.filename,
        width: converted.width,
        height: converted.height,
        // v4 `image.revisedPrompt || null` — empty → None.
        revised_prompt: image.revised_prompt.clone().filter(|s| !s.is_empty()),
    };

    // v4 renders an unmeasured dimension as `null` (the key is present).
    tracing::debug!(
        context = LOG_CONTEXT,
        index = index,
        providerMimeType = %provider_mime_type,
        storedMimeType = %decoded.mime_type,
        bytes = decoded.buffer.len(),
        width = %js_dimension(decoded.width),
        height = %js_dimension(decoded.height),
        "[ImageAttempt] Decoded provider image"
    );

    Some(decoded)
}

/// A measured dimension as JS renders it in a log bag: `null` when absent, the
/// integer otherwise (an integral `f64` prints without `.0`).
fn js_dimension(v: Option<f64>) -> String {
    match v {
        None => "null".to_string(),
        Some(f) if f.fract() == 0.0 && f.abs() < 9e15 => format!("{}", f as i64),
        Some(f) => format!("{f}"),
    }
}

/// A [`crate::services::file_storage::PixelCodec`] as the decode's transcoder
/// — v4's `convertToWebP` policy over the codec (the wardrobe generation and
/// the avatar preview carry a codec, not a transcoder).
pub struct PixelCodecTranscoder<'a>(pub &'a dyn crate::services::file_storage::PixelCodec);

impl ImageTranscoder for PixelCodecTranscoder<'_> {
    fn transcode(&self, input: &TranscodeInput) -> crate::model::image::TranscodeOutput {
        let r = crate::services::file_storage::convert_to_webp(
            self.0,
            &input.bytes,
            &input.mime_type,
            &input.filename,
        );
        crate::model::image::TranscodeOutput {
            bytes: r.buffer,
            mime_type: r.mime_type,
            filename: r.filename,
            width: r.width.map(|w| w as f64),
            height: r.height.map(|h| h as f64),
        }
    }
}

/// The erased host provider as an [`ImageProvider`], so the wardrobe
/// generation's `WardrobeItemImageSeams.provider` (an
/// [`crate::model::image::ErasedImageGenerate`]) drives the shared attempt
/// like every concrete provider does. Its future is `Send` (the erased inner
/// trait is `Send + Sync`).
impl ImageProvider for crate::model::image::ErasedImageGenerate {
    fn generate_image(
        &self,
        provider: &str,
        api_key: &str,
        params: &ImageGenParams,
    ) -> impl Future<Output = Result<ImageGenResponse, ImageGenError>> + Send {
        crate::model::image::ErasedImageGenerate::generate_image(self, provider, api_key, params)
    }
}
