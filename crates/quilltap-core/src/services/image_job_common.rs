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
use crate::model::image::{ImageGenResponse, ImageProvider};
use crate::services::dangerous_content::image_failover::{
    generate_image_with_concierge_failover, FailoverProfile, ImageFailoverContext,
    ImageFailoverError, ImageFailoverOutcome, ImagePurpose, ImageUnderstudySource,
};
use crate::services::dangerous_content::provider_routing::ApiKeyResolver;
use crate::services::dangerous_content::resolver::ResolvedConciergePolicy;
use crate::services::llm_logging::{
    log_llm_call, log_type, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage,
    LogResponse,
};
use crate::services::route_trail::{RouteAttemptVia, RouteProfileKind};

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

/// The outcome of an image-failover generation: the images + the profile that
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

/// v4 `buildImageGenParams` as the two image JOBS call it: `n: 1`, `style:
/// 'natural'`, the handler's orientation, and the profile's LoRAs + residual
/// options.
///
/// Extracted from the jobs' generation flow for P4.D184: the avatar handler must
/// derive its configuration cache key from the very params object that reaches
/// the wire, and must do it BEFORE the Concierge classification. Building the
/// object twice would key one and send the other — and would fire the
/// `[Image LoRA]` lines twice where v4 fires them once. So the caller builds,
/// keys, and hands the same object back in.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_job_image_params(
    provider: &str,
    model_name: &str,
    parameters: &Value,
    final_prompt: &str,
    orientation: Orientation,
    declarations_for: &ImageDeclarationsFn,
    log_context: &'static str,
    chat_id: Option<&str>,
    job_id: Option<&str>,
    profile_id: &str,
) -> crate::model::image::ImageGenParams {
    build_image_gen_params(
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
    .params
}

/// The two image JOBS' generation through the Concierge's failover chokepoint
/// (v4 `8bd080267`: `attemptPortrait` / `attemptBackground` handed to
/// [`generate_image_with_concierge_failover`]). The handler owns only what is
/// profile-specific, and this is it: the params for whichever profile the
/// chokepoint hands over, and the `IMAGE_GENERATION` `llm_logs` row per
/// attempt (v4 `logLLMCall`), success or failure.
///
/// Two comparisons, one closure, as v4 has them: the params are the PREBUILT
/// ones (the avatar's cache-keyed object) only for the `requested_profile_id`,
/// and every other profile rebuilds under `other_log_context`; the success
/// row's `(Concierge reroute)` suffix keys on `≠ unsuffixed_profile_id` (the
/// avatar's EFFECTIVE primary after a pre-flight reroute, the story's
/// requested profile). The handlers log their own failure and rerouted lines.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate_job_image<I: ImageProvider, A: ApiKeyResolver>(
    db: &Db,
    image_provider: &I,
    api_keys: &A,
    primary: FailoverProfile,
    primary_key: String,
    requested_profile_id: &str,
    prebuilt_params: Option<crate::model::image::ImageGenParams>,
    unsuffixed_profile_id: &str,
    final_prompt: &str,
    orientation: Orientation,
    declarations_for: &ImageDeclarationsFn,
    requested_log_context: &'static str,
    other_log_context: &'static str,
    concierge_policy: &ResolvedConciergePolicy,
    user_id: &str,
    chat_id: Option<&str>,
    // v4 `4d370a90f`: the chat, for the chokepoint's Concierge-state snapshot.
    chat: Option<&Value>,
    character_id: Option<&str>,
    job_id: Option<&str>,
    purpose: ImagePurpose,
    primary_via: RouteAttemptVia,
    // v4 `announceUnresolvedRefusal` (`ce2f1dabf`, #77): the Lantern passes
    // `false` (its own refusal bubble reports it); every other job `true`.
    announce_unresolved_refusal: bool,
) -> Result<ImageFailoverOutcome<ImageGenResponse>, ImageFailoverError> {
    let prebuilt_params = &prebuilt_params;
    let attempt = |profile: FailoverProfile, key: String| async move {
        let params = match (prebuilt_params, profile.id == requested_profile_id) {
            (Some(prebuilt), true) => prebuilt.clone(),
            (_, is_requested) => build_job_image_params(
                &profile.provider,
                &profile.model_name,
                &load_profile_parameters(db, &profile.id).await,
                final_prompt,
                orientation,
                declarations_for,
                if is_requested {
                    requested_log_context
                } else {
                    other_log_context
                },
                chat_id,
                job_id,
                &profile.id,
            ),
        };
        let suffix = if profile.id != unsuffixed_profile_id {
            " (Concierge reroute)"
        } else {
            ""
        };
        // v4 `const startTime = Date.now()` — a real wall-clock read bracketing
        // the provider attempt (NOT the handlers' pinned `now_ms`).
        let start = crate::clock::now_unix_ms();
        let result = image_provider
            .generate_image(&profile.provider, &key, &params)
            .await;
        let duration_ms = (crate::clock::now_unix_ms() - start) as f64;
        let (content, error) = match &result {
            Ok(response) => (job_success_content(response, suffix), None),
            Err(e) => (String::new(), Some(e.message.clone())),
        };
        log_image_gen_job(
            db,
            user_id,
            chat_id,
            character_id,
            &profile.provider,
            &profile.model_name,
            &profile.id,
            final_prompt,
            content,
            error,
            duration_ms,
        )
        .await;
        result
    };
    let understudy = ImageUnderstudySource {
        db,
        api_keys,
        user_id,
        uncensored_image_profile_id: concierge_policy.desk.image_profile_id.as_deref(),
    };
    generate_image_with_concierge_failover(
        (primary, primary_key),
        attempt,
        &ImageFailoverContext {
            db,
            user_id,
            chat_id,
            chat,
            purpose,
            concierge_policy,
            understudy: &understudy,
            profile_kind: RouteProfileKind::Image,
            primary_via,
            announce_unresolved_refusal,
        },
    )
    .await
}

/// v4's handlers map a failed call's trail to `{ profileName, outcome }` pairs
/// for their ERROR bag (`conciergeTrail`), as JSON for the file layer's
/// `…Json` convention; `None` when there is no trail (v4's `trail?.map` →
/// `undefined`, an absent key).
pub(crate) fn concierge_trail_log_json(error: &ImageFailoverError) -> Option<String> {
    error.concierge_trail().map(|trail| {
        Value::Array(
            trail
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "profileName": a.profile_name,
                        "outcome": a.outcome.as_str(),
                    })
                })
                .collect(),
        )
        .to_string()
    })
}

/// v4's handler error: `trail && trail.length > 1 ? `${prefix} after Concierge
/// reroute: ${msg}` : `${prefix}: ${msg}``.
pub(crate) fn job_failure_message(prefix: &str, error: &ImageFailoverError) -> String {
    match error.concierge_trail() {
        Some(trail) if trail.len() > 1 => {
            format!("{prefix} after Concierge reroute: {}", error.error.message)
        }
        _ => format!("{prefix}: {}", error.error.message),
    }
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
    use crate::model::image::{GeneratedImageData, ImageGenError};
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
    //
    // P4.D225: the bug-133 reroute tests that lived here pinned a gate and
    // three log lines v4 `8bd080267` RETIRED with the per-handler reroute; the
    // chokepoint's own lines are pinned by `image_failover_tier3_equivalence`,
    // the handlers' by their tier-3 families.

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A provider that takes a real ~30 ms and then answers, or fails — with a
    /// NON-refusal error, or (for the second call) a typed refusal first.
    struct SlowImageProvider {
        fail: bool,
        refuse_first: bool,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl SlowImageProvider {
        fn new(fail: bool, refuse_first: bool) -> Self {
            Self {
                fail,
                refuse_first,
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    impl ImageProvider for SlowImageProvider {
        fn generate_image(
            &self,
            _provider: &str,
            _api_key: &str,
            _params: &crate::model::image::ImageGenParams,
        ) -> impl std::future::Future<Output = Result<ImageGenResponse, ImageGenError>> + Send
        {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let fail = self.fail;
            let refuse = self.refuse_first && n == 0;
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                if refuse {
                    return Err(ImageGenError::moderation("blocked", Some(400), None));
                }
                if fail {
                    return Err(ImageGenError::new("provider exploded"));
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

    fn open_db(dir: &std::path::Path) -> Db {
        let main_path = dir.join("main.db");
        let ll_path = dir.join("llm-logs.db");
        {
            let w = Writer::open_writable(&main_path, PEPPER).unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TABLE image_profiles (\
                       id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT, \
                       apiKeyId TEXT, baseUrl TEXT, modelName TEXT, parameters TEXT, \
                       isDefault INTEGER, isDangerousCompatible INTEGER, tags TEXT, \
                       createdAt TEXT, updatedAt TEXT);\
                     INSERT INTO image_profiles VALUES ('understudy-1', 'user-1', \
                       'Night Desk', 'GROK', 'key-1', NULL, 'grok-2-image', '{}', 0, 1, \
                       '[]', '2026-01-01', '2026-01-01');",
                )
                .unwrap();
        }
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

    async fn logged_rows(db: &Db) -> Vec<(Option<f64>, String)> {
        db.read_llm_logs(|conn| {
            let mut stmt =
                conn.prepare("SELECT durationMs, response FROM llm_logs ORDER BY rowid")?;
            let out = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, Option<f64>>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(out)
        })
        .unwrap()
    }

    struct AlwaysApiKey;
    impl ApiKeyResolver for AlwaysApiKey {
        fn resolve(&self, _api_key_id: &str, _user_id: &str) -> Option<String> {
            Some("sk-understudy".to_string())
        }
    }

    async fn run_gen<A: ApiKeyResolver>(
        db: &Db,
        provider: &SlowImageProvider,
        api_keys: &A,
        mode: &str,
    ) -> Result<ImageFailoverOutcome<ImageGenResponse>, ImageFailoverError> {
        let declarations: Box<ImageDeclarationsFn> =
            Box::new(|_p: &str| ImageDeclarations::default());
        // The retired mode, mapped as v4's migration maps it: `OFF` goes off
        // duty (no failover), anything else is on duty (failover allowed).
        let settings =
            crate::services::dangerous_content::resolver::resolve_stored_concierge_settings(
                Some(&serde_json::json!({ "enabled": mode != "OFF" })),
                None,
            );
        generate_job_image(
            db,
            provider,
            api_keys,
            FailoverProfile {
                id: "profile-1".into(),
                name: "Day Desk".into(),
                provider: "OPENAI".into(),
                model_name: "gpt-image-1".into(),
                row: Value::Null,
            },
            "sk-test".into(),
            "profile-1",
            None,
            "profile-1",
            "a prompt",
            Orientation::Square,
            &declarations,
            "test.image-job",
            "test.image-job.concierge-reroute",
            &settings,
            "user-1",
            None,
            None,
            None,
            None,
            ImagePurpose::Lantern,
            RouteAttemptVia::Primary,
            true,
        )
        .await
    }

    #[tokio::test]
    async fn success_row_duration_brackets_the_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let out = run_gen(
            &db,
            &SlowImageProvider::new(false, false),
            &NoApiKeys,
            "OFF",
        )
        .await;
        assert!(out.is_ok());
        let rows = logged_rows(&db).await;
        assert_eq!(rows.len(), 1, "exactly one llm_logs row written");
        let d = rows[0].0.expect("durationMs must be present");
        assert!(
            d >= 20.0,
            "durationMs must bracket the ~30 ms provider call, got {d}"
        );
    }

    #[tokio::test]
    async fn failure_row_duration_brackets_the_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let out = run_gen(&db, &SlowImageProvider::new(true, false), &NoApiKeys, "OFF").await;
        let err = out.expect_err("a non-refusal failure comes back untouched");
        assert!(
            err.concierge_trail().is_none(),
            "no trail on the untouched arm"
        );
        let rows = logged_rows(&db).await;
        assert_eq!(rows.len(), 1, "exactly one llm_logs row written");
        let d = rows[0].0.expect("durationMs must be present");
        assert!(
            d >= 20.0,
            "durationMs must bracket the ~30 ms provider call, got {d}"
        );
    }

    /// The rerouted run logs one row PER ATTEMPT, each with its own span, and
    /// only the understudy's success content carries v4's `(Concierge
    /// reroute)` suffix (the refused primary's row is an error row).
    #[tokio::test]
    async fn a_rerouted_run_logs_both_attempts_and_suffixes_the_understudy() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let out = run_gen(
            &db,
            &SlowImageProvider::new(false, true),
            &AlwaysApiKey,
            "AUTO_ROUTE",
        )
        .await
        .expect("the understudy answers");
        assert!(out.rerouted);
        assert_eq!(out.profile.id, "understudy-1");
        assert_eq!(out.api_key, "sk-understudy");
        let rows = logged_rows(&db).await;
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows.iter().all(|(d, _)| d.is_some_and(|d| d >= 20.0)));
        assert!(rows[0].1.contains("\"error\":\"blocked\""), "{}", rows[0].1);
        assert!(
            rows[1]
                .1
                .contains("Generated 1 image(s) (Concierge reroute)"),
            "{}",
            rows[1].1
        );
    }

    /// v4's handler error: the "after Concierge reroute" prefix only when the
    /// trail has more than the refused primary on it; the ERROR bag's
    /// `conciergeTrail` pairs.
    #[tokio::test]
    async fn the_handler_error_names_a_reroute_only_when_one_was_tried() {
        let dir = tempfile::tempdir().unwrap();
        let db = open_db(dir.path());
        let err = run_gen(
            &db,
            &SlowImageProvider::new(false, true),
            &NoApiKeys,
            "DETECT_ONLY",
        )
        .await
        .expect_err("not permitted");
        assert_eq!(
            job_failure_message("Image generation failed", &err),
            "Image generation failed: blocked"
        );
        assert_eq!(
            concierge_trail_log_json(&err).as_deref(),
            Some(r#"[{"profileName":"Day Desk","outcome":"refused"}]"#)
        );

        let err = run_gen(
            &db,
            &SlowImageProvider::new(true, true),
            &AlwaysApiKey,
            "AUTO_ROUTE",
        )
        .await
        .expect_err("the understudy fails too");
        assert_eq!(
            job_failure_message("Image generation failed", &err),
            "Image generation failed after Concierge reroute: provider exploded"
        );
        assert_eq!(
            concierge_trail_log_json(&err).as_deref(),
            Some(
                r#"[{"profileName":"Day Desk","outcome":"refused"},{"profileName":"Night Desk","outcome":"failed"}]"#
            )
        );

        let err = run_gen(
            &db,
            &SlowImageProvider::new(true, false),
            &NoApiKeys,
            "AUTO_ROUTE",
        )
        .await
        .expect_err("untouched");
        assert_eq!(concierge_trail_log_json(&err), None, "no trail → no key");
    }
}
