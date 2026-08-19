//! Shared helpers for the avatar + story-background job handlers (the parts that
//! differ from the `generate_image` TOOL path): building the cheap-LLM selection
//! from the user's profiles, decoding the provider's base64 bytes, constructing
//! the job-path `generateImage` params (the inline `{ prompt, model, n, …resolved,
//! quality, style }` object, NOT the tool's `mergeParameters` shape), and the
//! generate-with-post-hoc-reroute flow the two handlers share (parameterized by
//! the failure-message prefix + orientation).

use rusqlite::Connection;
use serde_json::Value;

use crate::cheap_llm::{
    get_cheap_llm_provider, CheapLlmConfig, CheapLlmProfile, CheapLlmSelection,
};
use crate::db::runtime::Db;
use crate::image_gen::{resolve_orientation, Orientation};
use crate::image_gen::{ModelInfo, OrientationSupport};
use crate::model::image::{ImageGenError, ImageGenParams, ImageGenResponse, ImageProvider};
use crate::services::dangerous_content::provider_routing::{
    is_image_moderation_error, resolve_uncensored_image_profile_for_reroute, ApiKeyResolver,
};
use crate::services::llm_logging::{
    log_llm_call, log_type, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage,
    LogResponse,
};

/// The orientation-data provider signature (v4's plugin-registry
/// `getImageGenerationModels` + `getImageProviderConstraints(...).orientationSupport`,
/// per provider): `provider -> (per-model info, provider-level default support)`.
pub type OrientationDataFn =
    dyn Fn(&str) -> (Vec<ModelInfo>, Option<OrientationSupport>) + Send + Sync;

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

/// The job-path `generateImage` params: the inline `{ prompt: genPrompt, model,
/// n: 1, ...resolved.params, quality, style: 'natural' }` object both handlers
/// build. Bound verbatim into `image_gen_key` so the canned key proves prompt +
/// orientation reach the provider.
pub(crate) fn build_job_gen_params(
    gen_prompt: &str,
    model: &str,
    quality: Option<String>,
    size: Option<String>,
    aspect_ratio: Option<String>,
) -> ImageGenParams {
    ImageGenParams {
        prompt: gen_prompt.to_string(),
        negative_prompt: None,
        model: model.to_string(),
        n: Some(1),
        size,
        aspect_ratio,
        quality,
        style: Some("natural".to_string()),
        seed: None,
        guidance_scale: None,
        steps: None,
    }
}

/// The `quality` param off a profile's `parameters` object (v4
/// `(profile.parameters as Record)?.quality`).
pub(crate) fn quality_from_parameters(parameters: &Value) -> Option<String> {
    parameters
        .get("quality")
        .and_then(Value::as_str)
        .map(str::to_string)
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

/// [decd8ef9] The post-hoc reroute's optional prompt re-craft seam.
///
/// v4 does this inline in the story handler's catch: the prompt the moderated
/// provider just rejected was crafted WITH the cinematic-concealment guidance,
/// and the reroute target accepts adult content, so it is re-crafted candidly
/// before being resent. v5 shares the reroute machinery between the story and
/// avatar handlers, so the re-craft arrives as a seam — the story handler
/// passes its candid re-craft, the avatar handler passes [`NoRerouteRecraft`]
/// (v4's avatar path is unchanged by that commit).
///
/// Best-effort by contract: `None` keeps the prompt already in hand, so the
/// reroute still produces an image.
pub(crate) trait RerouteRecraft {
    /// `reroute_provider` is the RESOLVED reroute target's provider label (v4
    /// passes `reroute.profile.provider` into the re-craft context).
    fn recraft(
        &self,
        reroute_provider: &str,
    ) -> impl std::future::Future<Output = Option<String>> + Send;
}

/// The no-op re-craft: the reroute resends the prompt it already has. The
/// avatar handler's answer (v4 re-crafts only on the story path).
pub(crate) struct NoRerouteRecraft;

impl RerouteRecraft for NoRerouteRecraft {
    async fn recraft(&self, _reroute_provider: &str) -> Option<String> {
        None
    }
}

/// The generate + post-hoc Concierge reroute flow shared by both handlers.
/// `fail_prefix` is the handler's error prefix (`"Avatar image generation failed"`
/// / `"Image generation failed"`); the after-reroute message is
/// `"{fail_prefix} after Concierge reroute: {msg}"`. Orientation is resolved via
/// the injected registry seam. Each provider attempt writes an `IMAGE_GENERATION`
/// `llm_logs` row (v4 `logLLMCall`) via [`log_image_gen_job`].
#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_with_reroute<
    I: ImageProvider,
    A: ApiKeyResolver,
    R: RerouteRecraft,
>(
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
    orientation_data_for: &OrientationDataFn,
    danger_mode: &str,
    uncensored_image_profile_id: Option<&str>,
    user_id: &str,
    chat_id: Option<&str>,
    character_id: Option<&str>,
    fail_prefix: &str,
    recraft: &R,
) -> Result<GenOutcome, String> {
    // Resolve orientation + build the merged params.
    let (models, support) = orientation_data_for(provider);
    let resolved = resolve_orientation(
        Some(models.as_slice()),
        Some(model_name),
        support.as_ref(),
        orientation,
    );
    let gen_prompt = if resolved.prompt_hint.is_empty() {
        final_prompt.to_string()
    } else {
        format!("{final_prompt}\n\n{}", resolved.prompt_hint)
    };
    let params = build_job_gen_params(
        &gen_prompt,
        model_name,
        quality_from_parameters(parameters),
        resolved.size.clone(),
        resolved.aspect_ratio.clone(),
    );

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
                &error,
                final_prompt,
                orientation,
                orientation_data_for,
                danger_mode,
                uncensored_image_profile_id,
                user_id,
                chat_id,
                character_id,
                fail_prefix,
                recraft,
            )
            .await
        }
    }
}

/// The post-hoc moderation reroute half of [`generate_with_reroute`].
#[allow(clippy::too_many_arguments)]
async fn reroute_or_fail<I: ImageProvider, A: ApiKeyResolver, R: RerouteRecraft>(
    db: &Db,
    image_provider: &I,
    api_keys: &A,
    profile_id: &str,
    error: &ImageGenError,
    final_prompt: &str,
    orientation: Orientation,
    orientation_data_for: &OrientationDataFn,
    danger_mode: &str,
    uncensored_image_profile_id: Option<&str>,
    user_id: &str,
    chat_id: Option<&str>,
    character_id: Option<&str>,
    fail_prefix: &str,
    recraft: &R,
) -> Result<GenOutcome, String> {
    let reroute = if is_image_moderation_error(&error.message) {
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
        return Err(format!("{fail_prefix}: {}", error.message));
    };

    // [decd8ef9] The rejected prompt was crafted for a MODERATED provider, so
    // unless the chat was already flagged it carries the cinematic-concealment
    // guidance. The reroute target accepts adult content, so give the handler
    // its chance to re-craft candidly rather than sending a needlessly draped
    // scene to a provider that never asked for one. Best-effort: `None` keeps
    // the prompt we already have, so the reroute still happens. Sits before the
    // profile/orientation resolution exactly as v4's block sits before
    // `createImageProvider`.
    let reroute_base_prompt = recraft
        .recraft(&reroute.profile.provider)
        .await
        .unwrap_or_else(|| final_prompt.to_string());

    // Re-resolve for the reroute provider/model; load its `parameters`.
    let reroute_params = load_profile_parameters(db, &reroute.profile.id).await;
    let (models, support) = orientation_data_for(&reroute.profile.provider);
    let resolved = resolve_orientation(
        Some(models.as_slice()),
        Some(&reroute.profile.model_name),
        support.as_ref(),
        orientation,
    );
    let reroute_prompt = if resolved.prompt_hint.is_empty() {
        reroute_base_prompt.clone()
    } else {
        format!("{reroute_base_prompt}\n\n{}", resolved.prompt_hint)
    };
    let params = build_job_gen_params(
        &reroute_prompt,
        &reroute.profile.model_name,
        quality_from_parameters(&reroute_params),
        resolved.size.clone(),
        resolved.aspect_ratio.clone(),
    );

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
            _params: &ImageGenParams,
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
        let orientation_data: Box<OrientationDataFn> = Box::new(|_p: &str| (Vec::new(), None));
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
            &orientation_data,
            "OFF",
            None,
            "user-1",
            None,
            None,
            "Image generation failed",
            &NoRerouteRecraft,
        )
        .await
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
