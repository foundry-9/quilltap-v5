//! The `generate_image` tool handler (v4
//! `lib/tools/handlers/image-generation-handler.ts`) — the stateful image
//! generation orchestration:
//!
//!   1. validate input + load/validate the image profile (API key via the
//!      [`ApiKeyResolver`] seam),
//!   2. resolve the Concierge (dangerous-content) settings + build the cheap-LLM
//!      selection,
//!   3. classify the user prompt (scanImagePrompts) + AUTO_ROUTE reroute,
//!   4. resolve character appearances (sceneState fast path / cheap-LLM /
//!      defaults) + the sanitize gate,
//!   5. expand the prompt (`{{placeholder}}` → craftImagePrompt, with the
//!      tier-concatenation fallback) + classify the expanded prompt
//!      (scanImageGeneration),
//!   6. generate images via the [`ImageProvider`] seam (`resolveOrientation`
//!      mutating the merged params, the post-hoc moderation reroute), and
//!   7. [`save_generated_image`] each: WebP transcode (the [`ImageTranscoder`]
//!      seam) → SHA-256 → the Lantern Backgrounds store write under `tool/` →
//!      the `files` row → tag inheritance → the Lantern notification (the
//!      recorded [`LanternNotificationSink`] seam, W4.6b).
//!
//! The whole handler composes the ported W4.2 dangerous-content subsystem
//! (`resolve_dangerous_content_settings` / `classify_content` /
//! `resolve_image_provider_for_dangerous_content` /
//! `resolve_uncensored_image_profile_for_reroute` /
//! `is_image_moderation_error`), the cheap-LLM resolution (`get_cheap_llm_provider`
//! / `resolve_uncensored_cheap_llm_selection`), the appearance resolution
//! ([`crate::services::appearance_resolution`]), the image-gen pure leaves
//! (`resolve_orientation` / `parse_placeholders`), and the document-store
//! primitive (`link_blob_content`).
//!
//! ## Model boundaries (tier-3 seams — the same ones the v4 oracle mocks)
//!   - [`ImageProvider`] — `provider.generateImage(params, apiKey)`.
//!   - [`CompletionProvider`] — the cheap-LLM tasks + classification.
//!   - [`ModerationProvider`] — the Concierge moderation-provider path.
//!   - [`ImageTranscoder`] — the WebP transcode (`convertToWebP`).
//!   - [`ApiKeyResolver`] — the profile's decrypted API key.
//!   - [`LanternNotificationSink`] — the personified Lantern post (W4.6b).
//!
//! ## Tracked deferrals (host / cross-subsystem seams)
//!   - The aesthetic subsystem (`resolveAesthetic` / `resolveDepictionGuidelines`)
//!     — v4 error-swallows it (an ad-hoc image never breaks on a guidance read),
//!     so the port supplies `None` and keeps the swallow shape. Injectable later.
//!   - `logLLMCall` — v4 fire-and-forgets an llm-logs row (never awaited). The
//!     host-side logging service is deferred (the `cheap_llm_exec` precedent).
//!   - The Lantern store's WebP `transcodeImages` uses the injected seam; the real
//!     encoder is W4.7f-adjacent host work.

use serde_json::Value;

use crate::cheap_llm::{
    get_cheap_llm_provider, resolve_uncensored_cheap_llm_selection, CheapLlmConfig,
    CheapLlmProfile, CheapLlmSelection, DangerousContentSettings as CheapDangerSettings,
};
use crate::db::chat_settings::DangerousContentSettings;
use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkBlobInput};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::image_gen::{resolve_orientation, ModelInfo, Orientation};
use crate::model::completion::CompletionProvider;
use crate::model::image::{ImageGenParams, ImageProvider, ImageTranscoder, TranscodeInput};
use crate::pronoun_gender::gender_from_pronouns;
use crate::services::appearance_resolution::{
    resolve_character_appearances, sanitize_appearances_if_needed, AppearanceResolutionInput,
    PhysicalDescription, ResolvedCharacterAppearance, SceneStateCharacter, WardrobeItemInput,
};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::dangerous_content::chat_override::is_chat_active_dangerous;
use crate::services::dangerous_content::gatekeeper::{classify_content, ModerationProvider};
use crate::services::dangerous_content::provider_routing::{
    is_image_moderation_error, resolve_image_provider_for_dangerous_content,
    resolve_uncensored_image_profile_for_reroute, ApiKeyResolver, RouteProfile,
};
use crate::services::dangerous_content::resolver::resolve_dangerous_content_settings;
use crate::services::image_scene_tasks::ChatMessage;
use crate::services::image_scene_tasks::{
    craft_image_prompt, DepictionGuideline, ExpansionClothing, ExpansionPlaceholder,
    ExpansionTiers, ImagePromptExpansionContext,
};
use crate::services::llm_logging::{
    log_llm_call, log_type, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage,
    LogResponse,
};

/// The tool output (v4 `ImageGenerationToolOutput`).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageGenerationToolOutput {
    pub success: bool,
    pub images: Vec<GeneratedImageResult>,
    pub error: Option<String>,
    pub message: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub expanded_prompt: Option<String>,
}

/// One saved image (v4 `GeneratedImageResult`). The dispatcher maps `images` into
/// the tool result array; `filepath` + `id` are what `process_tool_calls` looks
/// for to thread a generated-image descriptor into the finalizer link loop.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedImageResult {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub revised_prompt: Option<String>,
    pub filepath: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub sha256: Option<String>,
}

/// The tool input the dispatcher hands the handler (v4 `ImageGenerationToolInput`
/// — the validated Zod shape). `count`/`size`/`style` carry the schema defaults
/// materialized by the dispatcher before the handler runs.
#[derive(Clone, Debug)]
pub struct ImageGenerationToolInput {
    pub prompt: String,
    pub negative_prompt: Option<String>,
    /// `"portrait" | "landscape" | "square"` (default `square` in the handler).
    pub orientation: Option<String>,
    pub size: Option<String>,
    pub style: Option<String>,
    pub quality: Option<String>,
    pub aspect_ratio: Option<String>,
    pub count: Option<i64>,
}

/// v4 `validateImageGenerationInput`: prompt required, non-empty, ≤4000 (UTF-16).
pub fn validate_image_generation_input(input: &ImageGenerationToolInput) -> bool {
    let n = crate::jsstr::utf16_len(&input.prompt);
    (1..=4000).contains(&n)
}

/// The execution context (v4 `ImageToolExecutionContext`).
#[derive(Clone, Debug)]
pub struct ImageToolExecutionContext {
    pub user_id: String,
    pub profile_id: String,
    pub chat_id: Option<String>,
    pub calling_participant_id: Option<String>,
}

/// The Lantern image-notification writer seam (v4 `postLanternImageNotification`).
/// Given the chat + saved file + the requester name, post the byte-exact
/// `character-image` notification. [`RealLanternNotification`] delegates to the
/// W4.6b writer ([`crate::services::lantern_notifications`]); [`NoLanternNotification`]
/// is the off-path no-op.
pub trait LanternNotificationSink {
    fn post_character_image(
        &self,
        chat_id: &str,
        file_id: &str,
        requester_name: &str,
    ) -> impl std::future::Future<Output = ()> + Send;
}

/// A [`LanternNotificationSink`] that does nothing (off-path / read-only callers).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLanternNotification;
impl LanternNotificationSink for NoLanternNotification {
    async fn post_character_image(&self, _chat_id: &str, _file_id: &str, _requester_name: &str) {}
}

/// The production Lantern sink (Round-3 unification): posts the byte-exact
/// `character-image` notification via the W4.6b writer
/// [`crate::services::lantern_notifications::post_lantern_image_notification`]
/// (which composes the canonical `build_content`). Best-effort — the writer
/// swallows failures.
pub struct RealLanternNotification<'a> {
    pub db: &'a Db,
}
impl LanternNotificationSink for RealLanternNotification<'_> {
    async fn post_character_image(&self, chat_id: &str, file_id: &str, requester_name: &str) {
        let _ = crate::services::lantern_notifications::post_lantern_image_notification(
            self.db,
            crate::services::lantern_notifications::LanternPostParams {
                chat_id: chat_id.to_string(),
                file_id: file_id.to_string(),
                kind: crate::services::lantern_notifications::LanternNotificationKind::CharacterImage {
                    requester_name: requester_name.to_string(),
                },
                // The character-image body ignores the prompt (v4 `buildContent`).
                prompt: None,
            },
        )
        .await;
    }
}

// ===========================================================================
// The erased image-generation runner (the executor holds this)
// ===========================================================================

/// The object-safe image-generation boundary the tool dispatcher holds behind an
/// `Arc<dyn …>`, so `generate_image` routes without threading the handler's seven
/// generics through [`BuiltInToolRunner`] (the [`ErasedEmbeddingProvider`] precedent).
/// `run` executes the whole handler (steps 1–8) for a resolved image profile.
pub trait ImageGenerationRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        db: &'a Db,
        input: &'a ImageGenerationToolInput,
        ctx: &'a ImageToolExecutionContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ImageGenerationToolOutput> + Send + 'a>>;
}

/// A type-erased image-generation runner (an `Arc<dyn …>`).
#[derive(Clone)]
pub struct ErasedImageGeneration(std::sync::Arc<dyn ImageGenerationRunner>);

impl ErasedImageGeneration {
    /// Wrap a concrete runner.
    pub fn new<R: ImageGenerationRunner + 'static>(runner: R) -> Self {
        Self(std::sync::Arc::new(runner))
    }

    /// The not-configured default (no image subsystem wired) — faithful to the
    /// dispatcher's `if (!imageProfileId)` guard, returning v4's exact error.
    pub fn none() -> Self {
        Self(std::sync::Arc::new(NotConfiguredImageGeneration))
    }

    /// Run the handler.
    pub async fn run(
        &self,
        db: &Db,
        input: &ImageGenerationToolInput,
        ctx: &ImageToolExecutionContext,
    ) -> ImageGenerationToolOutput {
        self.0.run(db, input, ctx).await
    }
}

/// v4 `result.images` serialization (`GeneratedImageResult` → the JSON object v4's
/// generate route ships as each `data[]` element): `{id, url, filename,
/// revisedPrompt?, filepath?, mimeType?, size?, width?, height?, sha256?}`, with the
/// optional keys dropped when absent (JSON.stringify-of-undefined semantics) and the
/// numbers rendered JS-bare. The tool-executor keeps a private copy for the
/// tool-call array; the `imageProfileGenerate` route arm (P4.6ai) reuses this one.
pub fn generated_image_to_value(img: &GeneratedImageResult) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("id".into(), Value::String(img.id.clone()));
    m.insert("url".into(), Value::String(img.url.clone()));
    m.insert("filename".into(), Value::String(img.filename.clone()));
    if let Some(v) = &img.revised_prompt {
        m.insert("revisedPrompt".into(), Value::String(v.clone()));
    }
    if let Some(v) = &img.filepath {
        m.insert("filepath".into(), Value::String(v.clone()));
    }
    if let Some(v) = &img.mime_type {
        m.insert("mimeType".into(), Value::String(v.clone()));
    }
    if let Some(v) = img.size {
        m.insert("size".into(), crate::db::js_number_to_json(v));
    }
    if let Some(v) = img.width {
        m.insert("width".into(), crate::db::js_number_to_json(v));
    }
    if let Some(v) = img.height {
        m.insert("height".into(), crate::db::js_number_to_json(v));
    }
    if let Some(v) = &img.sha256 {
        m.insert("sha256".into(), Value::String(v.clone()));
    }
    Value::Object(m)
}

/// The default runner: image generation is not enabled for this chat. Returns v4's
/// dispatcher guard error (`lib/chat/tool-executor.ts` — `if (!imageProfileId)`).
struct NotConfiguredImageGeneration;
impl ImageGenerationRunner for NotConfiguredImageGeneration {
    fn run<'a>(
        &'a self,
        _db: &'a Db,
        _input: &'a ImageGenerationToolInput,
        _ctx: &'a ImageToolExecutionContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ImageGenerationToolOutput> + Send + 'a>>
    {
        Box::pin(async move {
            ImageGenerationToolOutput {
                success: false,
                images: Vec::new(),
                error: Some("Image generation is not enabled for this chat".to_string()),
                message: None,
                provider: None,
                model: None,
                expanded_prompt: None,
            }
        })
    }
}

/// A concrete [`ImageGenerationRunner`] over owned seams. The production host + the
/// differential each build one of these; it forwards to
/// [`execute_image_generation_tool`]. `orientation_data_for` is an owned closure.
pub struct TypedImageGeneration<I, C, M, A, T, L, F> {
    pub image_provider: I,
    pub completion: C,
    pub moderation: M,
    pub api_keys: A,
    pub transcoder: T,
    pub lantern: L,
    pub executor: CheapLlmTaskExecutor,
    pub now_ms: i64,
    /// `Fn(provider) -> (models, provider-support)` (the plugin-registry seam).
    pub orientation_data_for: F,
}

impl<I, C, M, A, T, L, F> ImageGenerationRunner for TypedImageGeneration<I, C, M, A, T, L, F>
where
    I: ImageProvider + Send + Sync,
    C: CompletionProvider + Send + Sync,
    M: ModerationProvider + Send + Sync,
    A: ApiKeyResolver + Send + Sync,
    T: ImageTranscoder + Send + Sync,
    L: LanternNotificationSink + Send + Sync,
    F: Fn(&str) -> (Vec<ModelInfo>, Option<crate::image_gen::OrientationSupport>)
        + Send
        + Sync
        + 'static,
{
    fn run<'a>(
        &'a self,
        db: &'a Db,
        input: &'a ImageGenerationToolInput,
        ctx: &'a ImageToolExecutionContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ImageGenerationToolOutput> + Send + 'a>>
    {
        Box::pin(async move {
            let deps = ImageGenDeps {
                image_provider: &self.image_provider,
                completion: &self.completion,
                moderation: &self.moderation,
                api_keys: &self.api_keys,
                transcoder: &self.transcoder,
                lantern: &self.lantern,
                executor: &self.executor,
                now_ms: self.now_ms,
                orientation_data_for: &self.orientation_data_for,
            };
            execute_image_generation_tool(db, &deps, input, ctx).await
        })
    }
}

/// The orientation-data provider signature (v4's plugin-registry
/// `getImageGenerationModels` + `getImageProviderConstraints(...).orientationSupport`,
/// per provider): `provider -> (per-model info, provider-level default support)`.
pub type OrientationDataFn =
    dyn Fn(&str) -> (Vec<ModelInfo>, Option<crate::image_gen::OrientationSupport>) + Send + Sync;

/// The bag of injected seams the handler needs (grouped so the top-level signature
/// stays legible). Providers are borrowed for the handler's lifetime.
pub struct ImageGenDeps<'a, I, C, M, A, T, L> {
    pub image_provider: &'a I,
    pub completion: &'a C,
    pub moderation: &'a M,
    pub api_keys: &'a A,
    pub transcoder: &'a T,
    pub lantern: &'a L,
    /// The executor holding the session no-custom-temperature cache (v4's
    /// module-global). One per handler run in the differential.
    pub executor: &'a CheapLlmTaskExecutor,
    /// Injected wall clock for the `generated_<ts>.<ext>` filename (v4
    /// `Date.now()`). The differential pins it so the provider filename is stable.
    pub now_ms: i64,
    /// Injected orientation data per provider (v4's plugin-registry
    /// `getImageGenerationModels` + `getImageProviderConstraints(...).orientationSupport`).
    /// Returns `(per-model info, provider-level default support)`, both consulted
    /// by `resolveOrientation`.
    pub orientation_data_for: &'a OrientationDataFn,
}

// ===========================================================================
// mergeParameters + applyOrientation
// ===========================================================================

/// A JSON value's number as `i64` when integer-valued (v4 reads
/// `profile.n as number` etc.; SQLite REAL cells round-trip integers).
fn as_i64(v: Option<&Value>) -> Option<i64> {
    v.and_then(Value::as_i64)
        .or_else(|| v.and_then(Value::as_f64).map(|f| f as i64))
}

/// v4 `mergeParameters`: tool input over profile defaults, model from the profile.
fn merge_parameters(
    input: &ImageGenerationToolInput,
    profile_defaults: &Value,
    model: Option<&str>,
) -> ImageGenParams {
    let d = |k: &str| profile_defaults.get(k);
    let str_default = |k: &str| d(k).and_then(Value::as_str).map(str::to_string);
    ImageGenParams {
        prompt: input.prompt.clone(),
        negative_prompt: input
            .negative_prompt
            .clone()
            .or_else(|| str_default("negativePrompt")),
        model: model
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| str_default("model"))
            .unwrap_or_else(|| "dall-e-3".to_string()),
        n: input.count.or_else(|| as_i64(d("n"))).or(Some(1)),
        size: input.size.clone().or_else(|| str_default("size")),
        aspect_ratio: input
            .aspect_ratio
            .clone()
            .or_else(|| str_default("aspectRatio")),
        quality: input.quality.clone().or_else(|| str_default("quality")),
        style: input.style.clone().or_else(|| str_default("style")),
        seed: as_i64(d("seed")),
        guidance_scale: d("guidanceScale").and_then(Value::as_f64),
        steps: as_i64(d("steps")),
    }
}

/// v4 `applyOrientation`: mutate `size`/`aspectRatio`/prompt in place per the
/// resolved orientation (orientation wins over any raw size/aspectRatio). The
/// per-provider `models` + provider-level `provider_support` come from the plugin
/// registry (v4's `getImageGenerationModels` / `getImageProviderConstraints`),
/// supplied via the injected [`ImageGenDeps::orientation_data_for`].
fn apply_orientation(
    params: &mut ImageGenParams,
    orientation: Orientation,
    models: &[ModelInfo],
    provider_support: Option<&crate::image_gen::OrientationSupport>,
) {
    let model = params.model.clone();
    let resolved = resolve_orientation(Some(models), Some(&model), provider_support, orientation);
    if let Some(size) = resolved.size {
        params.size = Some(size);
    }
    if let Some(aspect) = resolved.aspect_ratio {
        params.aspect_ratio = Some(aspect);
    }
    if !resolved.prompt_hint.is_empty() {
        params.prompt = format!("{}\n\n{}", params.prompt, resolved.prompt_hint);
    }
}

/// v4's `input.orientation ?? 'square'` → the [`Orientation`] enum.
fn orientation_of(input: &ImageGenerationToolInput) -> Orientation {
    match input.orientation.as_deref() {
        Some("portrait") => Orientation::Portrait,
        Some("landscape") => Orientation::Landscape,
        _ => Orientation::Square,
    }
}

// ===========================================================================
// The loaded image profile (v4 `imageProfile` + `apiKey`)
// ===========================================================================

/// A fully-loaded, API-key-validated image profile (v4 `profileResult.profile`).
#[derive(Clone, Debug)]
struct LoadedProfile {
    id: String,
    name: String,
    provider: String,
    model_name: String,
    parameters: Value,
    api_key: String,
}

/// v4 `loadAndValidateProfile`: read the profile, verify ownership + API key.
/// Returns `Ok(Some(profile))`, `Ok(None)` (a structured tool-output failure),
/// or `Err` (a DB error → v4's `DATABASE_ERROR`).
fn load_and_validate_profile<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    profile_id: &str,
    user_id: &str,
) -> Result<Result<LoadedProfile, ImageGenerationToolOutput>, DbError> {
    let pid = profile_id.to_string();
    let profile = db.read_main(move |conn| crate::db::image_profiles::find_by_id(conn, &pid))?;

    let profile = match profile {
        Some(p) if p.get("userId").and_then(Value::as_str) == Some(user_id) => p,
        _ => {
            return Ok(Err(ImageGenerationToolOutput {
                success: false,
                images: Vec::new(),
                error: Some("Image profile not found or not authorized".to_string()),
                message: Some(format!(
                    "Image profile \"{profile_id}\" does not exist or you do not have access to it"
                )),
                provider: None,
                model: None,
                expanded_prompt: None,
            }));
        }
    };

    // Get the API key if the profile has one.
    let api_key = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .and_then(|id| api_keys.resolve(id, user_id))
        .filter(|k| !k.is_empty());

    let Some(api_key) = api_key else {
        let name = profile.get("name").and_then(Value::as_str).unwrap_or("");
        return Ok(Err(ImageGenerationToolOutput {
            success: false,
            images: Vec::new(),
            error: Some("No API key configured".to_string()),
            message: Some(format!(
                "Image profile \"{name}\" does not have a valid API key configured"
            )),
            provider: None,
            model: None,
            expanded_prompt: None,
        }));
    };

    Ok(Ok(LoadedProfile {
        id: profile
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        name: profile
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        provider: profile
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model_name: profile
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        parameters: profile.get("parameters").cloned().unwrap_or(Value::Null),
        api_key,
    }))
}

// ===========================================================================
// Placeholder resolution (v4 `resolvePlaceholders`)
// ===========================================================================

/// One resolved placeholder (v4 `PlaceholderInfo`, the subset the handler uses).
#[derive(Clone, Debug, Default)]
struct ResolvedPlaceholder {
    placeholder: String,
    name: String,
    entity_id: Option<String>,
    /// The character's `physicalDescription` (a single description, or none).
    physical_description: Option<Value>,
    /// The character's `pronouns` object (for the gender hint).
    pronouns: Option<Value>,
}

/// v4 `resolvePlaceholders`. Reads the chat + characters (both connections for the
/// overlaid `physicalDescription`). The `{{me}}`/`{{I}}`/`{{char}}` +
/// `{{user}}` + by-name/alias resolution.
fn resolve_placeholders(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    placeholders: &[crate::image_gen::Placeholder],
    user_id: &str,
    chat: Option<&Value>,
    calling_participant_id: Option<&str>,
) -> Vec<ResolvedPlaceholder> {
    let mut resolved = Vec::new();

    let participants = chat
        .and_then(|c| c.get("participants"))
        .and_then(Value::as_array);

    let read_char = |id: &str| -> Option<Value> {
        crate::db::characters_read::find_by_id(main, mount, id)
            .ok()
            .flatten()
    };

    let from_character =
        |character: &Value| -> (Option<String>, Option<Value>, String, Option<Value>) {
            let entity_id = character
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let descriptions = character
                .get("physicalDescription")
                .filter(|v| !v.is_null())
                .cloned();
            let name = character
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let pronouns = character.get("pronouns").filter(|v| !v.is_null()).cloned();
            (entity_id, descriptions, name, pronouns)
        };

    for ph in placeholders {
        let lower = ph.name.to_lowercase();

        // {{me}} / {{I}} / {{char}} = the caller.
        if lower == "me" || lower == "i" || lower == "char" {
            let mut entry = ResolvedPlaceholder {
                placeholder: ph.placeholder.clone(),
                name: ph.name.clone(),
                ..Default::default()
            };
            let resolved_char =
                if let (Some(cpid), Some(parts)) = (calling_participant_id, participants) {
                    parts
                        .iter()
                        .find(|p| p.get("id").and_then(Value::as_str) == Some(cpid))
                        .and_then(|p| p.get("characterId").and_then(Value::as_str))
                        .and_then(read_char)
                } else if let Some(parts) = participants {
                    // No calling participant — fall back to the first CHARACTER participant.
                    parts
                        .iter()
                        .find(|p| p.get("type").and_then(Value::as_str) == Some("CHARACTER"))
                        .and_then(|p| p.get("characterId").and_then(Value::as_str))
                        .and_then(read_char)
                } else {
                    None
                };
            if let Some(character) = resolved_char {
                let (entity_id, descriptions, name, pronouns) = from_character(&character);
                entry.entity_id = entity_id;
                entry.physical_description = descriptions;
                entry.name = name;
                entry.pronouns = pronouns;
            }
            resolved.push(entry);
            continue;
        }

        // {{user}} = the OTHER participant (user-controlled when the LLM calls).
        if lower == "user" {
            let mut entry = ResolvedPlaceholder {
                placeholder: ph.placeholder.clone(),
                name: ph.name.clone(),
                ..Default::default()
            };
            if let Some(parts) = participants {
                let mut other: Option<&Value> = None;
                if let Some(cpid) = calling_participant_id {
                    if let Some(caller) = parts
                        .iter()
                        .find(|p| p.get("id").and_then(Value::as_str) == Some(cpid))
                    {
                        let caller_is_user =
                            caller.get("controlledBy").and_then(Value::as_str) == Some("user");
                        other = parts.iter().find(|p| {
                            if p.get("id").and_then(Value::as_str) == Some(cpid) {
                                return false;
                            }
                            let cb = p.get("controlledBy").and_then(Value::as_str);
                            if caller_is_user {
                                cb == Some("llm") || cb.is_none()
                            } else {
                                cb == Some("user")
                            }
                        });
                    }
                }
                if other.is_none() {
                    other = parts
                        .iter()
                        .find(|p| p.get("controlledBy").and_then(Value::as_str) == Some("user"));
                }
                if let Some(character) = other
                    .and_then(|p| p.get("characterId").and_then(Value::as_str))
                    .and_then(read_char)
                {
                    let (entity_id, descriptions, name, pronouns) = from_character(&character);
                    entry.entity_id = entity_id;
                    entry.physical_description = descriptions;
                    entry.name = name;
                    entry.pronouns = pronouns;
                }
            }
            resolved.push(entry);
            continue;
        }

        // By name or alias among the user's characters.
        let by_name = crate::db::characters_read::find_by_user_id(main, mount, user_id)
            .ok()
            .unwrap_or_default()
            .into_iter()
            .find(|c| {
                let cname = c.get("name").and_then(Value::as_str).unwrap_or_default();
                if cname.to_lowercase() == lower {
                    return true;
                }
                c.get("aliases")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter().any(|al| {
                            al.as_str()
                                .map(|s| s.to_lowercase() == lower)
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            });

        if let Some(character) = by_name {
            let (entity_id, descriptions, name, pronouns) = from_character(&character);
            resolved.push(ResolvedPlaceholder {
                placeholder: ph.placeholder.clone(),
                name,
                entity_id,
                physical_description: descriptions,
                pronouns,
            });
            continue;
        }

        // Not found — include with no descriptions.
        resolved.push(ResolvedPlaceholder {
            placeholder: ph.placeholder.clone(),
            name: ph.name.clone(),
            ..Default::default()
        });
    }

    resolved
}

// ===========================================================================
// buildExpansionContext (v4)
// ===========================================================================

/// v4 `PROVIDER_LIMITS` — provider → prompt char limit.
fn provider_limit(provider: &str) -> i64 {
    match provider {
        "OPENAI" => 4000,
        "GROK" => 1000,
        "GOOGLE_IMAGEN" => 1920,
        // v4 indexes `PROVIDER_LIMITS[provider]` — an unknown provider is
        // `undefined` there, which the handler never hits (the enum is fixed).
        // Use the OpenAI limit as a safe default.
        _ => 4000,
    }
}

/// Read a physicalDescription tier field (`None` when absent/empty/non-string —
/// v4 `primary.<x>Prompt || undefined`).
fn desc_tier(desc: Option<&Value>, key: &str) -> Option<String> {
    desc.and_then(|d| d.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// v4 `buildExpansionContext`: per-placeholder detail (resolved appearance wins
/// over raw description tiers).
fn build_expansion_context(
    original_prompt: &str,
    resolved: &[ResolvedPlaceholder],
    provider: &str,
    resolved_appearances: Option<&[ResolvedCharacterAppearance]>,
) -> ImagePromptExpansionContext {
    let placeholders = resolved
        .iter()
        .map(|ph| {
            let gender = gender_from_pronouns(
                ph.pronouns
                    .as_ref()
                    .and_then(|p| p.get("subject"))
                    .and_then(Value::as_str),
            )
            .map(str::to_string);

            // A resolved appearance for this character?
            let resolved_app = resolved_appearances.and_then(|apps| {
                ph.entity_id
                    .as_deref()
                    .and_then(|eid| apps.iter().find(|a| a.character_id == eid))
            });

            if let Some(app) = resolved_app {
                let clothing = if app.clothing_description.is_empty() {
                    Vec::new()
                } else {
                    vec![ExpansionClothing {
                        name: if app.clothing_source == "narrative" {
                            "Current outfit (from story)".to_string()
                        } else {
                            "Current outfit".to_string()
                        },
                        usage_context: None,
                        description: Some(app.clothing_description.clone()),
                    }]
                };
                ExpansionPlaceholder {
                    placeholder: ph.placeholder.clone(),
                    name: ph.name.clone(),
                    gender,
                    usage_context: Some(app.physical_description_name.clone()),
                    tiers: ExpansionTiers {
                        complete: Some(app.physical_description.clone()),
                        ..Default::default()
                    },
                    clothing,
                }
            } else {
                // No resolved appearance — raw description data.
                let d = ph.physical_description.as_ref();
                let usage_context = d
                    .and_then(|x| x.get("usageContext"))
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                ExpansionPlaceholder {
                    placeholder: ph.placeholder.clone(),
                    name: ph.name.clone(),
                    gender,
                    usage_context,
                    tiers: ExpansionTiers {
                        short: desc_tier(d, "shortPrompt"),
                        medium: desc_tier(d, "mediumPrompt"),
                        long: desc_tier(d, "longPrompt"),
                        complete: desc_tier(d, "completePrompt"),
                    },
                    clothing: Vec::new(),
                }
            }
        })
        .collect();

    ImagePromptExpansionContext {
        original_prompt: original_prompt.to_string(),
        placeholders,
        target_length: provider_limit(provider),
        provider: provider.to_string(),
        style_trigger_phrase: None,
        style_name: None,
        scene_aesthetic: None,
        character_aesthetic: None,
        depiction_guidelines: Vec::<DepictionGuideline>::new(),
    }
}

// ===========================================================================
// saveGeneratedImage
// ===========================================================================

/// v4 `getInheritedTags`: merge tags from each linked entity (character / chat /
/// connection profile / image profile). For `saveGeneratedImage`'s `linkedTo`
/// (`[chatId]` or `[]`), the chat branch resolves; the others are graceful
/// fall-throughs (v4's per-entity try/catch swallows). Embedding-profile tags are
/// not reachable for a chat id (`embedding_profiles::find_by_id` unported — a
/// non-matching id simply falls through, byte-identical for the chat-linked case).
fn get_inherited_tags(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    linked_entity_ids: &[String],
    user_id: &str,
) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut add_tags = |v: &Value| {
        if let Some(tags) = v.get("tags").and_then(Value::as_array) {
            for t in tags {
                if let Some(s) = t.as_str() {
                    if seen.insert(s.to_string()) {
                        all.push(s.to_string());
                    }
                }
            }
        }
    };
    for entity_id in linked_entity_ids {
        // character
        if let Ok(Some(c)) = crate::db::characters_read::find_by_id(main, mount, entity_id) {
            if c.get("userId").and_then(Value::as_str) == Some(user_id) {
                add_tags(&c);
                continue;
            }
        }
        // chat
        if let Ok(Some(chat)) = crate::db::chats_read::find_by_id(main, entity_id) {
            if chat.get("userId").and_then(Value::as_str) == Some(user_id) {
                add_tags(&chat);
                continue;
            }
        }
        // connection profile
        if let Ok(Some(cp)) = crate::db::connection_profiles::find_by_id(main, entity_id) {
            if cp.get("userId").and_then(Value::as_str) == Some(user_id) {
                add_tags(&cp);
                continue;
            }
        }
        // image profile
        if let Ok(Some(ip)) = crate::db::image_profiles::find_by_id(main, entity_id) {
            if ip.get("userId").and_then(Value::as_str) == Some(user_id) {
                add_tags(&ip);
                continue;
            }
        }
        // embedding profile: unported find-by-id → graceful fall-through (a chat id
        // is never an embedding profile, so no tags are missed for saveGeneratedImage).
    }
    all
}

/// v4 `normaliseBlobRelativePath`: force a `.webp` extension when the stored mime
/// is `image/webp` (the Lantern store's transcode path).
fn normalise_blob_relative_path(relative_path: &str, stored_mime_type: &str) -> String {
    if stored_mime_type != "image/webp" {
        return relative_path.to_string();
    }
    if relative_path.to_lowercase().ends_with(".webp") {
        return relative_path.to_string();
    }
    let last_dot = relative_path.rfind('.');
    let last_slash = relative_path.rfind('/');
    match last_dot {
        Some(dot) if last_slash.map(|s| dot > s).unwrap_or(true) => {
            format!("{}.webp", &relative_path[..dot])
        }
        _ => format!("{relative_path}.webp"),
    }
}

/// v4 `sanitizeLeafName`: strip path components, replace unsafe chars with `_`,
/// collapse runs, trim leading/trailing `_`/`.`, `'unnamed'` when empty.
fn sanitize_leaf_name(filename: &str) -> String {
    let basename = filename.rsplit(['\\', '/']).next().unwrap_or(filename);
    // UNSAFE_LEAF_CHARS in v4: /[<>:"/\\|?*\x00-\x1F]/g plus whitespace runs? v4's
    // regex is /[^a-zA-Z0-9._ -]/g equivalent — reproduce the documented set.
    let mut safe: String = basename
        .chars()
        .map(|c| if is_unsafe_leaf_char(c) { '_' } else { c })
        .collect();
    // Collapse `__+` → `_`.
    while safe.contains("__") {
        safe = safe.replace("__", "_");
    }
    let trimmed = safe.trim_matches(|c| c == '_' || c == '.');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// v4 `UNSAFE_LEAF_CHARS` = `/[<>:"/\\|?*-]/g`.
fn is_unsafe_leaf_char(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) <= 0x1F
}

/// v4 `sha256OfBuffer`: lowercase hex SHA-256 of the bytes.
fn sha256_of_buffer(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The DB-write half of v4 `saveGeneratedImage`: Lantern store write under
/// `tool/` → `files` row → tag inheritance. Runs inside the writer closure (both
/// connections) with the already-transcoded bytes (the WebP transcode + base64
/// decode happen at the async layer, off the writer thread — the seams never
/// cross into the writer). Returns the saved image or a `STORAGE_ERROR` message.
/// The Lantern notification (fire-and-forget) is posted by the caller after this
/// returns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn save_generated_image(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    lantern_mount_point_id: Option<&str>,
    converted: &crate::model::image::TranscodeOutput,
    original_mime_type: &str,
    user_id: &str,
    chat_id: Option<&str>,
    metadata: &SaveMetadata,
) -> Result<GeneratedImageResult, String> {
    let mime_type = original_mime_type;
    let sha256 = sha256_of_buffer(&converted.bytes);
    let linked_to: Vec<String> = chat_id.map(|c| vec![c.to_string()]).unwrap_or_default();

    // The Lantern Backgrounds mount must be provisioned (refuse rather than leak
    // bytes into the catch-all _general/ space).
    let Some(mount_point_id) = lantern_mount_point_id.filter(|s| !s.is_empty()) else {
        return Err(
            "Failed to save generated image: Lantern Backgrounds mount is not provisioned; cannot persist generate_image output."
                .to_string(),
        );
    };

    // Write into `tool/<safeFilename>` — the store's WebP transcode already ran
    // (the injected seam), so the blob relative path takes the `.webp` extension.
    let safe_name = sanitize_leaf_name(&converted.filename);
    let desired_path = format!("tool/{safe_name}");
    let final_path = normalise_blob_relative_path(&desired_path, &converted.mime_type);

    let links = DocMountFileLinksRepository::new(mount);
    // `unique-suffix` collision strategy: pick a free path (v4 `resolveUniqueRelativePath`).
    let unique_path = resolve_unique_relative_path(mount, mount_point_id, &final_path)
        .map_err(|e| format!("Failed to save generated image: {e}"))?;

    // Ensure the parent folder row(s).
    if let Some(dir) = posix_dirname(&unique_path) {
        links
            .ensure_folder_path(mount_point_id, &dir)
            .map_err(|e| format!("Failed to save generated image: {e}"))?;
    }

    let file_name = unique_path
        .rsplit('/')
        .next()
        .unwrap_or(&unique_path)
        .to_string();
    let written = links
        .link_blob_content(&LinkBlobInput {
            mount_point_id: mount_point_id.to_string(),
            relative_path: unique_path.clone(),
            file_name: file_name.clone(),
            file_type: Some("blob".to_string()),
            original_file_name: safe_name.clone(),
            original_mime_type: mime_type.to_string(),
            stored_mime_type: converted.mime_type.clone(),
            sha256: sha256.clone(),
            data: converted.bytes.clone(),
            description: None,
            conversion_status: None,
            extracted_text: None,
            extracted_text_sha256: None,
            extraction_status: None,
        })
        .map_err(|e| format!("Failed to save generated image: {e}"))?;
    let storage_key = format!("mount-blob:{mount_point_id}:{}", written.blob_id);

    // Inherit tags from linked entities (e.g. the chat).
    let inherited_tags = get_inherited_tags(main, mount, &linked_to, user_id);

    // Create the `files` metadata row with the new id.
    let file_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let files = crate::db::files::FilesRepository::new(main);
    files
        .create(
            &crate::db::files::FileCreate {
                user_id: user_id.to_string(),
                sha256: sha256.clone(),
                original_filename: safe_name.clone(),
                mime_type: converted.mime_type.clone(),
                size: converted.bytes.len() as f64,
                width: converted.width,
                height: converted.height,
                is_plain_text: None,
                linked_to: linked_to.clone(),
                source: "GENERATED".to_string(),
                category: "IMAGE".to_string(),
                generation_prompt: Some(metadata.prompt.clone()),
                generation_model: Some(metadata.model.clone()),
                generation_revised_prompt: metadata.revised_prompt.clone(),
                description: None,
                tags: inherited_tags,
                project_id: None,
                folder_path: None,
                storage_key: Some(storage_key),
                file_status: "ok".to_string(),
            },
            &crate::db::files::CreateOptions {
                id: file_id.clone(),
                created_at: now.clone(),
                updated_at: now,
            },
        )
        .map_err(|e| format!("Failed to save generated image: {e}"))?;

    // The Lantern notification (v4 `postLanternImageNotification`) is a
    // fire-and-forget the caller posts after this write returns (the seam stays
    // off the writer thread).

    Ok(GeneratedImageResult {
        id: file_id.clone(),
        url: format!("/api/v1/images/{file_id}"),
        filename: safe_name,
        revised_prompt: metadata.revised_prompt.clone(),
        filepath: Some(format!("/api/v1/files/{file_id}")),
        mime_type: Some(converted.mime_type.clone()),
        size: Some(converted.bytes.len() as f64),
        width: converted.width,
        height: converted.height,
        sha256: Some(sha256),
    })
}

/// The metadata `saveGeneratedImage` writes onto the file row. `provider` is
/// carried for v4 parity (v4's `metadata.provider`) but the `files` row only
/// stores `generationModel` — the provider isn't persisted, matching v4.
#[derive(Clone, Debug)]
pub(crate) struct SaveMetadata {
    pub prompt: String,
    pub revised_prompt: Option<String>,
    pub model: String,
    #[allow(dead_code)]
    pub provider: String,
}

/// POSIX `path.dirname` for a relative blob path (`None` when there is no folder
/// segment — v4's `folderDir !== '.' && folderDir !== ''` gate).
fn posix_dirname(path: &str) -> Option<String> {
    match path.rfind('/') {
        Some(i) if i > 0 => Some(path[..i].to_string()),
        _ => None,
    }
}

/// v4 `resolveUniqueRelativePath` (the `unique-suffix` collision strategy): a free
/// path under a mount. If `desired` is taken, bump `(2)`, `(3)`, … up to 999. The
/// sha1-tagged fallback past 999 is a documented non-deterministic seam (never hit
/// by the corpus — the differential pins `now_ms` and generates one image).
fn resolve_unique_relative_path(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
    desired: &str,
) -> Result<String, DbError> {
    let blobs = crate::db::doc_mount_blobs::DocMountBlobsRepository::new(mount);
    if blobs
        .find_by_mount_point_and_path(mount_point_id, desired)?
        .is_none()
    {
        return Ok(desired.to_string());
    }
    let dir = posix_dirname(desired);
    let (stem, ext) = split_stem_ext(desired);
    let prefix = match &dir {
        Some(d) => format!("{d}/"),
        None => String::new(),
    };
    for attempt in 2..=999u32 {
        let candidate = format!("{prefix}{stem} ({attempt}){ext}");
        if blobs
            .find_by_mount_point_and_path(mount_point_id, &candidate)?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    // Documented non-deterministic seam: sha1(`${desired}:${now}`)[:8]. Reproduced
    // with sha2 (the workspace hash) — never reached by the corpus.
    let tag = &sha256_of_buffer(format!("{desired}:{}", crate::clock::now_iso()).as_bytes())[..8];
    Ok(format!("{prefix}{stem} ({tag}){ext}"))
}

// ===========================================================================
// The cheap-LLM selection (v4's repeated `getCheapLLMProvider` block)
// ===========================================================================

/// Build a [`CheapLlmProfile`] from a connection-profile net-read value (v4
/// treats a connection profile as a cheap-LLM profile candidate).
fn cheap_llm_profile_from_value(v: &Value) -> CheapLlmProfile {
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
        model_class: v
            .get("modelClass")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

/// v4's `CheapLLMConfig` from a chat-settings `cheapLLMSettings` sub-object (or the
/// defaults when absent).
fn cheap_llm_config_from_settings(cheap: Option<&Value>) -> CheapLlmConfig {
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
        // v4 `DEFAULT_CHEAP_LLM_CONFIG`.
        None => CheapLlmConfig {
            strategy: "PROVIDER_CHEAPEST".to_string(),
            user_defined_profile_id: None,
            default_cheap_profile_id: None,
            fallback_to_local: false,
        },
    }
}

/// v4's repeated selection block: build the cheap-LLM selection from
/// `allProfiles` (default profile → `getCheapLLMProvider`), `None` when there are
/// no profiles. `registry_cheapest` is the injected registry default (unused by
/// the corpus's non-fallback paths).
fn build_cheap_llm_selection(
    all_profiles: &[Value],
    cheap_settings: Option<&Value>,
) -> Option<CheapLlmSelection> {
    let profiles: Vec<CheapLlmProfile> = all_profiles
        .iter()
        .map(cheap_llm_profile_from_value)
        .collect();
    // v4 `allProfiles.find(p => p.isDefault) || allProfiles[0]` — `profiles[i]`
    // corresponds 1:1 to `all_profiles[i]`, so zip to find the default.
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

/// Split a POSIX path's basename into `(stem, ext)` where `ext` includes the dot
/// (or is empty). Matches `path.posix.extname` / the stem-without-ext split.
fn split_stem_ext(path: &str) -> (String, String) {
    let basename = path.rsplit('/').next().unwrap_or(path);
    match basename.rfind('.') {
        // A leading-dot file (`.foo`) has no extension in POSIX terms.
        Some(i) if i > 0 => (basename[..i].to_string(), basename[i..].to_string()),
        _ => (basename.to_string(), String::new()),
    }
}

/// Node `Buffer.from(s, 'base64')`: decode standard/URL-safe base64, ignoring any
/// non-alphabet character (whitespace, newlines) and tolerating missing padding.
fn decode_base64_node(s: &str) -> Vec<u8> {
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
    // Collect only alphabet symbols (drop whitespace, padding, invalid chars).
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

// ===========================================================================
// The DB-context gather (one read closure) + the async orchestration
// ===========================================================================

/// Everything the async pipeline needs from the database, gathered up front so
/// the model calls run outside the writer closure.
struct DbContext {
    chat: Option<Value>,
    is_dangerous_chat: bool,
    recent_chat_messages: Vec<ChatMessage>,
    /// All the user's connection profiles (for cheap-LLM selection).
    all_profiles: Vec<Value>,
    /// The chat settings net-read (`cheapLLMSettings` / `dangerousContentSettings`).
    chat_settings: Option<Value>,
    /// The appearance-resolution inputs built from placeholders + wardrobe.
    appearance_inputs: Vec<AppearanceResolutionInput>,
}

/// Gather the DB context inside one both-connections read (v4 interleaves these;
/// the port batches them so the model pipeline is a clean async sequence). Never
/// throws — each sub-read swallows like v4 (the appearance/wardrobe reads are all
/// under a fail-safe in v4).
fn gather_db_context(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    ctx: &ImageToolExecutionContext,
    tool_input: &ImageGenerationToolInput,
) -> DbContext {
    let chat = ctx
        .chat_id
        .as_deref()
        .and_then(|id| crate::db::chats_read::find_by_id(main, id).ok().flatten());

    let is_dangerous_chat = is_chat_active_dangerous(chat.as_ref());

    // Recent chat messages: last 20 USER/ASSISTANT message events.
    let recent_chat_messages: Vec<ChatMessage> = ctx
        .chat_id
        .as_deref()
        .and_then(|id| crate::db::chats_messages_read::get_messages(main, id).ok())
        .map(|events| {
            let msgs: Vec<ChatMessage> = events
                .iter()
                .filter(|e| e.get("type").and_then(Value::as_str) == Some("message"))
                .filter(|e| {
                    matches!(
                        e.get("role").and_then(Value::as_str),
                        Some("USER") | Some("ASSISTANT")
                    )
                })
                .map(|e| {
                    let role = if e.get("role").and_then(Value::as_str) == Some("USER") {
                        "user"
                    } else {
                        "assistant"
                    };
                    ChatMessage {
                        role: role.to_string(),
                        content: e
                            .get("content")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    }
                })
                .collect();
            let start = msgs.len().saturating_sub(20);
            msgs[start..].to_vec()
        })
        .unwrap_or_default();

    let all_profiles =
        crate::db::connection_profiles::find_by_user_id(main, &ctx.user_id).unwrap_or_default();

    let chat_settings = crate::db::chat_settings::find_by_user_id(main, &ctx.user_id)
        .ok()
        .flatten();

    // Build appearance inputs from resolved placeholders + equipped wardrobe.
    let mut appearance_inputs: Vec<AppearanceResolutionInput> = Vec::new();
    let placeholders = crate::image_gen::parse_placeholders(&tool_input.prompt);
    if !placeholders.is_empty() {
        let resolved = resolve_placeholders(
            main,
            mount,
            &placeholders,
            &ctx.user_id,
            chat.as_ref(),
            ctx.calling_participant_id.as_deref(),
        );
        let project_mount_point_ids: Vec<String> = ctx
            .chat_id
            .as_deref()
            .map(|chat_id| {
                crate::tools::wardrobe_shared::resolve_project_mount_point_ids_for_chat(
                    main, mount, chat_id,
                )
            })
            .unwrap_or_default();
        let docs = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
        for p in resolved
            .iter()
            .filter(|p| p.entity_id.is_some() && p.physical_description.is_some())
        {
            let entity_id = p.entity_id.clone().unwrap();
            // Equipped wardrobe (per-slot leaf items) — v4's fail-safe try/catch.
            let mut equipped: Vec<WardrobeItemInput> = Vec::new();
            if let Some(chat_id) = ctx.chat_id.as_deref() {
                let slots = crate::tools::wardrobe_shared::load_current_wardrobe_state(
                    main, chat_id, &entity_id,
                )
                .unwrap_or_else(|_| crate::tools::wardrobe_shared::empty_equipped_state());
                if let Ok(leaves) =
                    crate::tools::wardrobe_shared::resolve_equipped_leaf_items_by_slot(
                        main,
                        &docs,
                        &entity_id,
                        &slots,
                        // Group stores follow each character's own memberships,
                        // so they can't come from the shared project list.
                        &crate::wardrobe_tiers::shared_wardrobe_tiers_for_character(
                            main,
                            mount,
                            &entity_id,
                            &project_mount_point_ids,
                        ),
                    )
                {
                    equipped = leaves
                        .into_iter()
                        .map(|l| WardrobeItemInput {
                            slot: l.slot,
                            title: l.title,
                            description: l.description,
                            image_prompt: l.image_prompt,
                        })
                        .collect();
                }
            }
            appearance_inputs.push(AppearanceResolutionInput {
                character_id: entity_id,
                character_name: p.name.clone(),
                physical_description: p
                    .physical_description
                    .as_ref()
                    .map(physical_description_from_value),
                equipped_wardrobe_items: equipped,
            });
        }
    }

    DbContext {
        chat,
        is_dangerous_chat,
        recent_chat_messages,
        all_profiles,
        chat_settings,
        appearance_inputs,
    }
}

/// Extract the appearance-resolution [`PhysicalDescription`] from a character's
/// overlaid `physicalDescription` JSON value.
fn physical_description_from_value(v: &Value) -> PhysicalDescription {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    PhysicalDescription {
        id: s("id").unwrap_or_default(),
        name: s("name").unwrap_or_default(),
        usage_context: s("usageContext"),
        short_prompt: s("shortPrompt"),
        medium_prompt: s("mediumPrompt"),
        long_prompt: s("longPrompt"),
        complete_prompt: s("completePrompt"),
    }
}

/// v4 `resolveRequesterName`: the calling character's name, else "the storyteller".
fn resolve_requester_name(chat: Option<&Value>, calling_participant_id: Option<&str>) -> String {
    let default = "the storyteller".to_string();
    let (Some(chat), Some(cpid)) = (chat, calling_participant_id) else {
        return default;
    };
    chat.get("participants")
        .and_then(Value::as_array)
        .and_then(|parts| {
            parts
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(cpid))
        })
        .and_then(|p| p.get("characterName").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or(default)
}

// ===========================================================================
// executeImageGenerationTool (the async spine)
// ===========================================================================

/// A tool-output failure (v4's early `{ success:false, ... }` returns).
fn output_error(error: &str, message: &str) -> ImageGenerationToolOutput {
    ImageGenerationToolOutput {
        success: false,
        images: Vec::new(),
        error: Some(error.to_string()),
        message: Some(message.to_string()),
        provider: None,
        model: None,
        expanded_prompt: None,
    }
}

/// v4 `executeImageGenerationTool`. The async spine composing steps 1–8.
#[allow(clippy::too_many_arguments)]
pub async fn execute_image_generation_tool<I, C, M, A, T, L>(
    db: &Db,
    deps: &ImageGenDeps<'_, I, C, M, A, T, L>,
    input: &ImageGenerationToolInput,
    ctx: &ImageToolExecutionContext,
) -> ImageGenerationToolOutput
where
    I: ImageProvider,
    C: CompletionProvider,
    M: ModerationProvider,
    A: ApiKeyResolver,
    T: ImageTranscoder,
    L: LanternNotificationSink,
{
    // 1. Validate input.
    if !validate_image_generation_input(input) {
        return output_error(
            "Invalid input: prompt is required and must be a non-empty string",
            "Image generation tool received invalid parameters",
        );
    }

    // 2. Load + validate the profile (API key via the seam).
    let load = match load_and_validate_profile(db, deps.api_keys, &ctx.profile_id, &ctx.user_id) {
        Ok(r) => r,
        Err(e) => {
            // v4 wraps a DB error as DATABASE_ERROR → caught by the outer handler.
            return output_error(
                "DATABASE_ERROR",
                &format!("An unexpected error occurred: {e}"),
            );
        }
    };
    let mut image_profile = match load {
        Ok(p) => p,
        Err(out) => return out,
    };

    // 4b. Resolve dangerous-content settings (chat may be off-duty).
    // Gather the DB context in one both-connections read.
    let db_ctx = match gather_db_context_via_db(db, ctx, input).await {
        Ok(c) => c,
        Err(e) => {
            return output_error(
                "UNKNOWN_ERROR",
                &format!("An unexpected error occurred: {e}"),
            )
        }
    };

    let global_danger = db_ctx
        .chat_settings
        .as_ref()
        .and_then(|cs| cs.get("dangerousContentSettings"))
        .and_then(|d| serde_json::from_value::<DangerousContentSettings>(d.clone()).ok());
    let resolved = resolve_dangerous_content_settings(global_danger, db_ctx.chat.as_ref());
    let danger_settings = resolved.settings;

    let cheap_settings = db_ctx
        .chat_settings
        .as_ref()
        .and_then(|cs| cs.get("cheapLLMSettings"));

    // 4c. Build the cheap-LLM selection for danger classification.
    let cheap_llm_selection = if danger_settings.mode != "OFF"
        && (danger_settings.scan_image_prompts || danger_settings.scan_image_generation)
    {
        build_cheap_llm_selection(&db_ctx.all_profiles, cheap_settings)
    } else {
        None
    };

    // 5-5b. Classify the user prompt (scanImagePrompts) + AUTO_ROUTE reroute.
    let mut image_prompt_dangerous = false;
    if danger_settings.mode != "OFF" && danger_settings.scan_image_prompts {
        if let Some(selection) = &cheap_llm_selection {
            let classification = classify_content(
                db,
                deps.moderation,
                deps.completion,
                &input.prompt,
                selection,
                &ctx.user_id,
                &danger_settings,
                ctx.chat_id.as_deref(),
            )
            .await;
            if classification.is_dangerous {
                image_prompt_dangerous = true;
                if danger_settings.mode == "AUTO_ROUTE" {
                    if let Some(rerouted) = reroute_image_profile(
                        db,
                        deps.api_keys,
                        &image_profile,
                        &danger_settings,
                        &ctx.user_id,
                    ) {
                        image_profile = rerouted;
                    }
                }
            }
        }
    }
    let effective_profile = image_profile.clone();

    // 5c-5d. Resolve character appearances + the sanitize gate.
    let mut resolved_appearances: Option<Vec<ResolvedCharacterAppearance>> = None;
    if !crate::image_gen::parse_placeholders(&input.prompt).is_empty()
        && !db_ctx.appearance_inputs.is_empty()
    {
        // Build the appearance cheap-LLM selection (standard, then uncensored
        // upgrade for a dangerous chat).
        let mut appearance_selection = cheap_llm_selection.clone().or_else(|| {
            build_cheap_llm_selection(
                &db_ctx.all_profiles,
                db_ctx
                    .chat_settings
                    .as_ref()
                    .and_then(|cs| cs.get("cheapLLMSettings")),
            )
        });
        if db_ctx.is_dangerous_chat {
            if let Some(sel) = appearance_selection.take() {
                let profiles: Vec<CheapLlmProfile> = db_ctx
                    .all_profiles
                    .iter()
                    .map(cheap_llm_profile_from_value)
                    .collect();
                let cheap_danger = CheapDangerSettings {
                    mode: danger_settings.mode.clone(),
                    uncensored_text_profile_id: danger_settings.uncensored_text_profile_id.clone(),
                };
                appearance_selection = Some(resolve_uncensored_cheap_llm_selection(
                    sel,
                    true,
                    Some(&cheap_danger),
                    &profiles,
                ));
            }
        }

        if let Some(selection) = &appearance_selection {
            let resolution = resolve_character_appearances(
                deps.executor,
                deps.completion,
                &db_ctx.appearance_inputs,
                &db_ctx.recent_chat_messages,
                &input.prompt,
                selection,
                &ctx.user_id,
                ctx.chat_id.as_deref(),
                // v4's tool handler does NOT pass sceneState (story-background only).
                None::<&[SceneStateCharacter]>,
            )
            .await;

            let has_uncensored_image_provider = danger_settings
                .uncensored_image_profile_id
                .as_deref()
                .map(|s| !s.is_empty())
                .unwrap_or(false);

            let sanitized = sanitize_appearances_if_needed(
                db,
                deps.executor,
                deps.moderation,
                deps.completion,
                resolution.appearances,
                &danger_settings,
                db_ctx.is_dangerous_chat,
                has_uncensored_image_provider,
                selection,
                &ctx.user_id,
                ctx.chat_id.as_deref(),
            )
            .await;
            resolved_appearances = Some(sanitized);
        }
    }

    // 6. Expand the prompt with descriptions.
    let (expanded_prompt, final_profile) = expand_prompt_with_context(
        db,
        deps,
        input,
        &image_profile,
        effective_profile,
        &danger_settings,
        cheap_llm_selection.as_ref(),
        image_prompt_dangerous,
        resolved_appearances.as_deref(),
        &db_ctx,
        ctx,
    )
    .await;

    // 7. Generate images (using the effective profile which may have been rerouted).
    let final_input = ImageGenerationToolInput {
        prompt: expanded_prompt.clone(),
        ..input.clone()
    };
    let saved = match generate_images_with_provider(
        db,
        deps,
        &final_input,
        &final_profile,
        input,
        &danger_settings,
        &db_ctx,
        ctx,
    )
    .await
    {
        Ok(images) => images,
        Err(e) => {
            let mut out = output_error(&e.code, &e.message);
            out.provider = Some(image_profile.provider.clone());
            out.model = Some(image_profile.model_name.clone());
            return out;
        }
    };

    // 8. Return success.
    let count = saved.len();
    ImageGenerationToolOutput {
        success: true,
        images: saved,
        message: Some(format!(
            "Successfully generated {count} image(s) using {}",
            final_profile.model_name
        )),
        provider: Some(final_profile.provider.clone()),
        model: Some(final_profile.model_name.clone()),
        expanded_prompt: Some(expanded_prompt),
        error: None,
    }
}

/// Gather the DB context via a `Db` read (both connections through a read closure
/// on the writer, matching the photo/wardrobe precedent — a read inside the writer
/// thread just serializes).
async fn gather_db_context_via_db(
    db: &Db,
    ctx: &ImageToolExecutionContext,
    input: &ImageGenerationToolInput,
) -> Result<DbContext, DbError> {
    let ctx = ctx.clone();
    let input = input.clone();
    db.write(move |writers| {
        let Some(mount_w) = writers.mount_index() else {
            return Err(DbError::Key(
                "generate_image requires the mount-index database".to_string(),
            ));
        };
        // Borrow order: the mount writer is a separate `Writer`, so both `main()`
        // and `mount_index()` can hand out read borrows for the gather.
        let mount = mount_w.connection();
        let main = writers.main().connection();
        Ok(gather_db_context(main, mount, &ctx, &input))
    })
    .await
}

// ===========================================================================
// The reroute + expand + generate helpers
// ===========================================================================

/// v4's AUTO_ROUTE image-provider reroute. Runs
/// `resolveImageProviderForDangerousContent`, then reloads the rerouted profile
/// with its API key. Returns `Some(profile)` for the rerouted profile, or `None`
/// when there is no reroute (or the reload failed) so the caller keeps the original.
fn reroute_image_profile<A: ApiKeyResolver>(
    db: &Db,
    api_keys: &A,
    image_profile: &LoadedProfile,
    danger_settings: &DangerousContentSettings,
    user_id: &str,
) -> Option<LoadedProfile> {
    let original = RouteProfile {
        id: image_profile.id.clone(),
        name: image_profile.name.clone(),
        provider: image_profile.provider.clone(),
        model_name: image_profile.model_name.clone(),
        base_url: None,
    };
    let uid = user_id.to_string();
    let mode = danger_settings.mode.clone();
    let uncensored = danger_settings.uncensored_image_profile_id.clone();
    let api_key = image_profile.api_key.clone();
    let route = db
        .read_main(move |conn| {
            Ok(resolve_image_provider_for_dangerous_content(
                conn,
                api_keys,
                &original,
                &api_key,
                &mode,
                uncensored.as_deref(),
                &uid,
            ))
        })
        .ok()?;
    if !route.rerouted {
        return None;
    }
    // Reload the full rerouted profile (with API key) — v4 `loadAndValidateProfile`.
    match load_and_validate_profile(db, api_keys, &route.image_profile.id, user_id) {
        Ok(Ok(p)) => Some(p),
        _ => None,
    }
}

/// v4 `expandPromptWithContext` (steps 6 + 6b). Expand the prompt with
/// descriptions (`{{placeholder}}` → craftImagePrompt), then classify the
/// expanded prompt (scanImageGeneration) and reroute if newly-dangerous.
#[allow(clippy::too_many_arguments)]
async fn expand_prompt_with_context<I, C, M, A, T, L>(
    db: &Db,
    deps: &ImageGenDeps<'_, I, C, M, A, T, L>,
    input: &ImageGenerationToolInput,
    image_profile: &LoadedProfile,
    effective_profile: LoadedProfile,
    danger_settings: &DangerousContentSettings,
    cheap_llm_selection: Option<&CheapLlmSelection>,
    image_prompt_dangerous: bool,
    resolved_appearances: Option<&[ResolvedCharacterAppearance]>,
    db_ctx: &DbContext,
    ctx: &ImageToolExecutionContext,
) -> (String, LoadedProfile)
where
    I: ImageProvider,
    C: CompletionProvider,
    M: ModerationProvider,
    A: ApiKeyResolver,
    T: ImageTranscoder,
    L: LanternNotificationSink,
{
    // 6. Expand.
    let expanded_prompt = expand_prompt_with_descriptions(
        db,
        deps,
        &input.prompt,
        &effective_profile.provider,
        image_prompt_dangerous,
        resolved_appearances,
        db_ctx,
        ctx,
    )
    .await;

    let mut effective_profile = effective_profile;

    // 6b. Classify the expanded prompt (only when it differs from the original).
    if danger_settings.mode != "OFF"
        && danger_settings.scan_image_generation
        && cheap_llm_selection.is_some()
        && expanded_prompt != input.prompt
    {
        if let Some(selection) = cheap_llm_selection {
            let classification = classify_content(
                db,
                deps.moderation,
                deps.completion,
                &expanded_prompt,
                selection,
                &ctx.user_id,
                danger_settings,
                ctx.chat_id.as_deref(),
            )
            .await;
            if classification.is_dangerous
                && !image_prompt_dangerous
                && danger_settings.mode == "AUTO_ROUTE"
                && effective_profile.id == image_profile.id
            {
                if let Some(rerouted) = reroute_image_profile(
                    db,
                    deps.api_keys,
                    image_profile,
                    danger_settings,
                    &ctx.user_id,
                ) {
                    effective_profile = rerouted;
                }
            }
        }
    }

    (expanded_prompt, effective_profile)
}

/// v4 `expandPromptWithDescriptions`. Parse + resolve placeholders, build the
/// expansion context (injecting resolved appearances), select the crafting
/// cheap-LLM (the uncensored image-prompt profile override on dangerous content),
/// craft, and fall back to tier-concatenation on a craft failure. On any error →
/// the original prompt (v4's outer catch).
#[allow(clippy::too_many_arguments)]
async fn expand_prompt_with_descriptions<I, C, M, A, T, L>(
    db: &Db,
    deps: &ImageGenDeps<'_, I, C, M, A, T, L>,
    original_prompt: &str,
    provider: &str,
    is_dangerous: bool,
    resolved_appearances: Option<&[ResolvedCharacterAppearance]>,
    db_ctx: &DbContext,
    ctx: &ImageToolExecutionContext,
) -> String
where
    I: ImageProvider,
    C: CompletionProvider,
    M: ModerationProvider,
    A: ApiKeyResolver,
    T: ImageTranscoder,
    L: LanternNotificationSink,
{
    let placeholders = crate::image_gen::parse_placeholders(original_prompt);
    if placeholders.is_empty() {
        return original_prompt.to_string();
    }

    // Resolve placeholders (both connections).
    let resolved: Vec<ResolvedPlaceholder> =
        match resolve_placeholders_via_db(db, &placeholders, db_ctx, ctx).await {
            Ok(r) => r,
            Err(_) => return original_prompt.to_string(),
        };

    let expansion_context =
        build_expansion_context(original_prompt, &resolved, provider, resolved_appearances);

    // Select the crafting cheap-LLM: the uncensored image-prompt profile on
    // dangerous content, else the standard cheap-LLM logic.
    let cheap_settings = db_ctx
        .chat_settings
        .as_ref()
        .and_then(|cs| cs.get("cheapLLMSettings"));
    let image_prompt_profile_id = cheap_settings
        .and_then(|c| c.get("imagePromptProfileId"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());

    let mut selection: Option<CheapLlmSelection> = None;
    if is_dangerous {
        if let Some(pid) = image_prompt_profile_id {
            if let Some(prof) = db_ctx
                .all_profiles
                .iter()
                .find(|p| p.get("id").and_then(Value::as_str) == Some(pid))
            {
                let provider_name = prof
                    .get("provider")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let is_local = provider_name == "OLLAMA";
                selection = Some(CheapLlmSelection {
                    provider: provider_name.to_string(),
                    model_name: prof
                        .get("modelName")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    connection_profile_id: prof
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    base_url: if is_local {
                        Some(
                            prof.get("baseUrl")
                                .and_then(Value::as_str)
                                .filter(|s| !s.is_empty())
                                .unwrap_or("http://localhost:11434")
                                .to_string(),
                        )
                    } else {
                        None
                    },
                    is_local,
                    profile_parameters: prof.get("parameters").cloned(),
                });
            }
        }
    }
    if selection.is_none() {
        // Standard cheap-LLM logic (returns None only when no profiles exist).
        if db_ctx.all_profiles.is_empty() {
            return original_prompt.to_string();
        }
        selection = build_cheap_llm_selection(&db_ctx.all_profiles, cheap_settings);
    }
    let Some(selection) = selection else {
        return original_prompt.to_string();
    };

    // (Aesthetics + Ariel Clause are the injected-None deferral — v4 error-swallows.)
    let craft = craft_image_prompt(
        deps.executor,
        deps.completion,
        &expansion_context,
        &selection,
        &ctx.user_id,
        ctx.chat_id.as_deref(),
    )
    .await;

    if craft.success {
        if let Some(result) = craft.result.filter(|s| !s.is_empty()) {
            return result;
        }
    }

    // Craft failed → tier-concatenation fallback (v4's `.replace` per placeholder).
    let mut fallback = original_prompt.to_string();
    for ph in &expansion_context.placeholders {
        let description = ph
            .tiers
            .complete
            .clone()
            .or_else(|| ph.tiers.long.clone())
            .or_else(|| ph.tiers.medium.clone())
            .or_else(|| ph.tiers.short.clone())
            .unwrap_or_else(|| ph.name.clone());
        // JS `String.replace(string, string)` replaces the FIRST occurrence.
        if let Some(pos) = fallback.find(&ph.placeholder) {
            fallback.replace_range(pos..pos + ph.placeholder.len(), &description);
        }
    }
    fallback
}

/// Resolve placeholders via a `Db` read (both connections).
async fn resolve_placeholders_via_db(
    db: &Db,
    placeholders: &[crate::image_gen::Placeholder],
    db_ctx: &DbContext,
    ctx: &ImageToolExecutionContext,
) -> Result<Vec<ResolvedPlaceholder>, DbError> {
    let placeholders = placeholders.to_vec();
    let user_id = ctx.user_id.clone();
    let chat = db_ctx.chat.clone();
    let cpid = ctx.calling_participant_id.clone();
    db.write(move |writers| {
        let Some(mount_w) = writers.mount_index() else {
            return Err(DbError::Key(
                "generate_image requires the mount-index database".to_string(),
            ));
        };
        let mount = mount_w.connection();
        let main = writers.main().connection();
        Ok(resolve_placeholders(
            main,
            mount,
            &placeholders,
            &user_id,
            chat.as_ref(),
            cpid.as_deref(),
        ))
    })
    .await
}

/// v4 `ImageGenerationError`'s `{ code, message }` for a provider/storage failure.
struct GenError {
    code: String,
    message: String,
}

/// v4 `logLLMCall` (image-generation-handler.ts) — one `IMAGE_GENERATION` row per
/// provider attempt (success/failure/reroute-success/reroute-failure),
/// fire-and-forget. Awaited here (the writer never throws — the watermark
/// precedent); `LogContext::none()` on the request path. `durationMs` is v4's
/// `Date.now()`-delta around the provider call — a REAL wall-clock span (the
/// P4.D49 cheap-path pattern; the differentials normalize non-NULL durations,
/// so a measured value stays oracle-neutral). The request is a single `user`
/// message carrying the tool prompt; no usage/cache/hashes on this path.
#[allow(clippy::too_many_arguments)]
async fn log_image_generation(
    db: &Db,
    ctx: &ImageToolExecutionContext,
    provider: &str,
    model_name: &str,
    // v4 `0cde7fbc`: `imageProfile.id` on the primary arms,
    // `reroute.profile.id` on the Concierge-reroute arms.
    image_profile_id: &str,
    prompt: &str,
    content: String,
    error: Option<String>,
    duration_ms: f64,
) {
    let params = LogLlmCallParams {
        user_id: ctx.user_id.clone(),
        log_type: log_type::IMAGE_GENERATION.to_string(),
        message_id: None,
        chat_id: ctx.chat_id.clone(),
        character_id: None,
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
/// content (an empty first-image `revisedPrompt` falls back to the count text).
fn image_gen_success_content(
    response: &crate::model::image::ImageGenResponse,
    suffix: &str,
) -> String {
    response
        .images
        .first()
        .and_then(|i| i.revised_prompt.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Generated {} image(s){suffix}", response.images.len()))
}

/// v4 `generateImagesWithProvider` — merge params + orientation, call the provider
/// (with the post-hoc moderation reroute), then save each image.
#[allow(clippy::too_many_arguments)]
async fn generate_images_with_provider<I, C, M, A, T, L>(
    db: &Db,
    deps: &ImageGenDeps<'_, I, C, M, A, T, L>,
    tool_input: &ImageGenerationToolInput,
    image_profile: &LoadedProfile,
    original_input: &ImageGenerationToolInput,
    danger_settings: &DangerousContentSettings,
    db_ctx: &DbContext,
    ctx: &ImageToolExecutionContext,
) -> Result<Vec<GeneratedImageResult>, GenError>
where
    I: ImageProvider,
    C: CompletionProvider,
    M: ModerationProvider,
    A: ApiKeyResolver,
    T: ImageTranscoder,
    L: LanternNotificationSink,
{
    // Merge params + resolve orientation (orientation wins over raw size/aspect).
    let mut merged = merge_parameters(
        tool_input,
        &image_profile.parameters,
        Some(&image_profile.model_name),
    );
    let (models, support) = (deps.orientation_data_for)(&image_profile.provider);
    apply_orientation(
        &mut merged,
        orientation_of(original_input),
        &models,
        support.as_ref(),
    );

    let mut active_provider = image_profile.provider.clone();
    let mut active_model = image_profile.model_name.clone();

    // Call the provider. v4 stamps `Date.now()` around the call for `durationMs`
    // — a real wall-clock read, NOT the pinned `deps.now_ms` (same-field
    // subtraction made every duration structurally 0).
    let gen_start = crate::clock::now_unix_ms();
    let response = match deps
        .image_provider
        .generate_image(&image_profile.provider, &image_profile.api_key, &merged)
        .await
    {
        Ok(r) => {
            log_image_generation(
                db,
                ctx,
                &image_profile.provider,
                &image_profile.model_name,
                &image_profile.id,
                &tool_input.prompt,
                image_gen_success_content(&r, ""),
                None,
                (crate::clock::now_unix_ms() - gen_start) as f64,
            )
            .await;
            r
        }
        Err(error) => {
            log_image_generation(
                db,
                ctx,
                &image_profile.provider,
                &image_profile.model_name,
                &image_profile.id,
                &tool_input.prompt,
                String::new(),
                Some(error.message.clone()),
                (crate::clock::now_unix_ms() - gen_start) as f64,
            )
            .await;
            // Post-hoc Concierge reroute on a moderation rejection.
            let reroute = if is_image_moderation_error(&error.message) {
                let uid = ctx.user_id.clone();
                let mode = danger_settings.mode.clone();
                let uncensored = danger_settings.uncensored_image_profile_id.clone();
                let current_id = image_profile.id.clone();
                let api_keys = deps.api_keys;
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
                return Err(GenError {
                    code: "PROVIDER_ERROR".to_string(),
                    message: format!("Image generation failed: {}", error.message),
                });
            };

            // Re-issue on the uncensored profile. The reroute profile's
            // `parameters` are not carried on the `RouteProfile`; v4 reads
            // `reroute.profile.parameters`, so load them from the DB.
            let reroute_params = load_profile_parameters(db, &reroute.profile.id);
            let mut reroute_merged = merge_parameters(
                tool_input,
                &reroute_params,
                Some(&reroute.profile.model_name),
            );
            let (reroute_models, reroute_support) =
                (deps.orientation_data_for)(&reroute.profile.provider);
            apply_orientation(
                &mut reroute_merged,
                orientation_of(original_input),
                &reroute_models,
                reroute_support.as_ref(),
            );
            let reroute_start = crate::clock::now_unix_ms();
            match deps
                .image_provider
                .generate_image(&reroute.profile.provider, &reroute.api_key, &reroute_merged)
                .await
            {
                Ok(r) => {
                    log_image_generation(
                        db,
                        ctx,
                        &reroute.profile.provider,
                        &reroute.profile.model_name,
                        &reroute.profile.id,
                        &tool_input.prompt,
                        image_gen_success_content(&r, " (Concierge reroute)"),
                        None,
                        (crate::clock::now_unix_ms() - reroute_start) as f64,
                    )
                    .await;
                    active_provider = reroute.profile.provider.clone();
                    active_model = reroute.profile.model_name.clone();
                    r
                }
                Err(reroute_error) => {
                    log_image_generation(
                        db,
                        ctx,
                        &reroute.profile.provider,
                        &reroute.profile.model_name,
                        &reroute.profile.id,
                        &tool_input.prompt,
                        String::new(),
                        Some(reroute_error.message.clone()),
                        (crate::clock::now_unix_ms() - reroute_start) as f64,
                    )
                    .await;
                    return Err(GenError {
                        code: "PROVIDER_ERROR".to_string(),
                        message: format!(
                            "Image generation failed after Concierge reroute: {}",
                            reroute_error.message
                        ),
                    });
                }
            }
        }
    };

    // Save each image.
    let requester_name =
        resolve_requester_name(db_ctx.chat.as_ref(), ctx.calling_participant_id.as_deref());
    let lantern_mount = db
        .read_main(crate::db::instance_settings::get_lantern_backgrounds_mount_point_id)
        .ok()
        .flatten();
    // Verify the store exists + is a database mount (v4 `getLanternBackgroundsStore`).
    let lantern_mount = match &lantern_mount {
        Some(id) => {
            let id2 = id.clone();
            let ok = db
                .read_mount_index(move |conn| {
                    let repo = crate::db::doc_mount_points::DocMountPointsRepository::new(conn);
                    Ok(repo
                        .find_by_id_for_docedit(&id2)?
                        .map(|row| row.mount_type == "database")
                        .unwrap_or(false))
                })
                .unwrap_or(false);
            if ok {
                lantern_mount
            } else {
                None
            }
        }
        None => None,
    };

    let mut saved = Vec::new();
    for img in &response.images {
        let metadata = SaveMetadata {
            prompt: tool_input.prompt.clone(),
            revised_prompt: img.revised_prompt.clone(),
            model: active_model.clone(),
            provider: active_provider.clone(),
        };
        let mime = img
            .mime_type
            .clone()
            .unwrap_or_else(|| "image/png".to_string());

        // Transcode at the async layer (the seams never cross into the writer):
        // base64-decode → WebP (the injected [`ImageTranscoder`] seam).
        let raw_buffer = decode_base64_node(img.data.as_deref().unwrap_or_default());
        let provider_ext = mime.split('/').nth(1).unwrap_or("png");
        let converted = deps.transcoder.transcode(&TranscodeInput {
            bytes: raw_buffer,
            mime_type: mime.clone(),
            filename: format!("generated_{}.{provider_ext}", deps.now_ms),
        });

        let uid = ctx.user_id.clone();
        let chat_id = ctx.chat_id.clone();
        let lantern_mount = lantern_mount.clone();
        let result = db
            .write(move |writers| {
                let Some(mount_w) = writers.mount_index() else {
                    return Err(DbError::Key(
                        "generate_image requires the mount-index database".to_string(),
                    ));
                };
                let mount = mount_w.connection();
                let main = writers.main().connection();
                Ok(save_generated_image(
                    main,
                    mount,
                    lantern_mount.as_deref(),
                    &converted,
                    &mime,
                    &uid,
                    chat_id.as_deref(),
                    &metadata,
                ))
            })
            .await
            // v4 `saveGeneratedImage`'s catch wraps ANY internal failure as
            // `ImageGenerationError('STORAGE_ERROR', 'Failed to save generated image',
            // <detail>)` — the output MESSAGE is the static string; the detail is
            // `details` (dropped from the tool output). Match that here (the `_e`
            // detail is the mount-not-provisioned / DB reason).
            .map_err(|_e| GenError {
                code: "STORAGE_ERROR".to_string(),
                message: "Failed to save generated image".to_string(),
            })?;
        match result {
            Ok(r) => {
                // Post the Lantern notification (off the writer thread).
                if let Some(chat_id) = ctx.chat_id.as_deref() {
                    deps.lantern
                        .post_character_image(chat_id, &r.id, &requester_name)
                        .await;
                }
                saved.push(r);
            }
            Err(_msg) => {
                return Err(GenError {
                    code: "STORAGE_ERROR".to_string(),
                    message: "Failed to save generated image".to_string(),
                })
            }
        }
    }

    Ok(saved)
}

/// Load a profile's `parameters` JSON (for the post-hoc reroute merge).
fn load_profile_parameters(db: &Db, profile_id: &str) -> Value {
    let pid = profile_id.to_string();
    db.read_main(move |conn| crate::db::image_profiles::find_by_id(conn, &pid))
        .ok()
        .flatten()
        .and_then(|p| p.get("parameters").cloned())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod duration_tests {
    use super::*;
    use crate::db::runtime::DbPaths;
    use crate::db::Writer;
    use crate::model::completion::CannedCompletionProvider;
    use crate::model::image::{ImageGenError, ImageGenResponse, PassthroughTranscoder};
    use crate::services::dangerous_content::gatekeeper::NoModerationProvider;
    use crate::services::dangerous_content::provider_routing::NoApiKeys;

    // === The f7f1a956-round §3 finding: `durationMs` must be a MEASURED span ===
    //
    // The `gen_start = deps.now_ms` shape made every tool-path duration
    // structurally 0 (same-field subtraction), and the differentials cannot see
    // it — `normalize_duration_ms` collapses every non-NULL duration. This pin
    // asserts what no oracle diff can: the failure-arm row's duration BRACKETS
    // the provider call (a provider that takes a real ~30 ms must produce a
    // duration in that ballpark, not 0). The failure arm is used because it
    // returns before the save path, needing no mount/main-DB scaffolding; the
    // success arm shares the same `gen_start` read.

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A provider that takes a real ~30 ms and then fails with a NON-moderation
    /// error (so no reroute is attempted and the arm returns immediately).
    struct SlowFailingProvider;

    impl ImageProvider for SlowFailingProvider {
        async fn generate_image(
            &self,
            _provider: &str,
            _api_key: &str,
            _params: &ImageGenParams,
        ) -> Result<ImageGenResponse, ImageGenError> {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            Err(ImageGenError {
                message: "provider exploded".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn failure_row_duration_brackets_the_provider_call() {
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.db");
        let ll_path = dir.path().join("llm-logs.db");
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
        let db = Db::open(
            DbPaths {
                main: main_path,
                mount_index: None,
                llm_logs: Some(ll_path),
            },
            PEPPER,
        )
        .unwrap();

        let orientation_data: Box<OrientationDataFn> = Box::new(|_p: &str| (Vec::new(), None));
        let deps = ImageGenDeps {
            image_provider: &SlowFailingProvider,
            completion: &CannedCompletionProvider::new(),
            moderation: &NoModerationProvider,
            api_keys: &NoApiKeys,
            transcoder: &PassthroughTranscoder,
            lantern: &NoLanternNotification,
            executor: &CheapLlmTaskExecutor::new(),
            now_ms: 1_700_000_000_000,
            orientation_data_for: &orientation_data,
        };
        let tool_input = ImageGenerationToolInput {
            prompt: "a prompt".to_string(),
            negative_prompt: None,
            orientation: None,
            size: None,
            style: None,
            quality: None,
            aspect_ratio: None,
            count: Some(1),
        };
        let image_profile = LoadedProfile {
            id: "profile-1".to_string(),
            name: "Test Profile".to_string(),
            provider: "OPENAI".to_string(),
            model_name: "gpt-image-1".to_string(),
            parameters: serde_json::json!({}),
            api_key: "sk-test".to_string(),
        };
        let danger_settings = DangerousContentSettings {
            mode: "OFF".to_string(),
            threshold: 0.7,
            scan_text_chat: false,
            scan_image_prompts: false,
            scan_image_generation: false,
            uncensored_text_profile_id: None,
            uncensored_image_profile_id: None,
            display_mode: "BLUR".to_string(),
            show_warning_badges: false,
            custom_classification_prompt: None,
        };
        let db_ctx = DbContext {
            chat: None,
            is_dangerous_chat: false,
            recent_chat_messages: Vec::new(),
            all_profiles: Vec::new(),
            chat_settings: None,
            appearance_inputs: Vec::new(),
        };
        let ctx = ImageToolExecutionContext {
            user_id: "user-1".to_string(),
            profile_id: "profile-1".to_string(),
            chat_id: None,
            calling_participant_id: None,
        };

        let out = generate_images_with_provider(
            &db,
            &deps,
            &tool_input,
            &image_profile,
            &tool_input,
            &danger_settings,
            &db_ctx,
            &ctx,
        )
        .await;
        assert!(
            out.is_err(),
            "the non-moderation failure arm must return Err"
        );

        let durations: Vec<Option<f64>> = db
            .read_llm_logs(|conn| {
                let mut stmt = conn.prepare("SELECT durationMs FROM llm_logs")?;
                let out = stmt
                    .query_map([], |row| row.get::<_, Option<f64>>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(out)
            })
            .unwrap();
        assert_eq!(durations.len(), 1, "exactly one llm_logs row written");
        let d = durations[0].expect("durationMs must be present");
        assert!(
            d >= 20.0,
            "durationMs must bracket the ~30 ms provider call, got {d}"
        );
    }
}
