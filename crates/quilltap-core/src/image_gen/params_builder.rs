//! The single builder that turns an image profile (plus whatever the caller
//! wants to override) into the [`ImageGenParams`] handed to a provider
//! (v4 `lib/image-gen/params-builder.ts`, `84f33ce94`).
//!
//! Before this existed, five call sites assembled those params independently —
//! the `generate_image` tool and its Concierge reroute, the avatar job, the
//! story-background job, `POST /api/v1/images`, and the wardrobe preview — and
//! three of them read exactly one key off the profile (`quality`). Anything new
//! on a profile therefore worked in chat and vanished everywhere else. LoRAs
//! would have been the fourth such casualty, so they arrive with the drift
//! fixed instead. **v5 inherited the same drift** (`services::image_job_common`
//! `build_job_gen_params` hard-coded it, `quality_from_parameters` IS the
//! "reads only quality" bug), so this fixes it here too.
//!
//! What the builder owns:
//!   - merging caller overrides over the profile's stored defaults, with the
//!     original `mergeParameters` semantics preserved key for key;
//!   - resolving the requested orientation onto the provider's own mechanism;
//!   - resolving, validating and capping `parameters.loras` against the
//!     provider/model's declared `loraSupport`, naming anything it drops;
//!   - handing the residual `parameters` bag to the provider as
//!     `profileParameters`, so per-model options travel without the host
//!     enumerating them.

use serde_json::{Map, Value};

use crate::image_gen::lora_support::{
    cap_loras, join_lora_trigger_phrases, lora_trigger_phrases, read_loras_from_parameters,
    resolve_lora_support, ImageLoraSpec, ImageLoraSupport, LoraLogContext,
};
use crate::image_gen::ResolvedOrientation;
use crate::image_gen::{resolve_orientation, ModelInfo, Orientation, OrientationSupport};
use crate::model::image::ImageGenParams;

/// Parameter keys the host owns outright: each maps onto a named
/// [`ImageGenParams`] field, so forwarding it a second time in
/// `profileParameters` would just be the same value under two names.
/// Everything else on the profile is provider business and rides the residual
/// bag untouched.
///
/// ⚠ `hf_api_token` and `lora_preset` are **deliberately absent**. They ride the
/// residual bag, and the decision to attach them belongs to the provider's own
/// dialect (NanoGPT's `apply_loras`), where the family is known. A host-side
/// filter here would break both keys.
pub const HOST_OWNED_PARAMETER_KEYS: [&str; 14] = [
    "prompt",
    "negativePrompt",
    "model",
    "size",
    "aspectRatio",
    "orientation",
    "quality",
    "style",
    "n",
    "responseFormat",
    "seed",
    "guidanceScale",
    "steps",
    "loras",
];

/// The slice of an image profile the builder actually reads
/// (v4 `ImageProfileLike`).
#[derive(Debug, Clone, Copy)]
pub struct ImageProfileLike<'a> {
    pub provider: &'a str,
    pub model_name: Option<&'a str>,
    pub parameters: Option<&'a Value>,
}

/// Caller-supplied values that outrank the profile's stored defaults: tool
/// input, a route body, or a job handler's fixed choices (`n: 1`,
/// `style: 'natural'`) — v4's
/// `Partial<Omit<ImageGenParams, 'prompt' | 'loras' | 'profileParameters'>>`.
#[derive(Debug, Clone, Default)]
pub struct ImageGenOverrides {
    pub negative_prompt: Option<String>,
    pub model: Option<String>,
    pub n: Option<f64>,
    pub size: Option<String>,
    pub aspect_ratio: Option<String>,
    pub quality: Option<String>,
    pub style: Option<String>,
    pub response_format: Option<String>,
    pub seed: Option<f64>,
    pub guidance_scale: Option<f64>,
    pub steps: Option<f64>,
}

/// The declarations the builder resolves against — v4's ONE
/// `getImageGenerationModels(provider)` list (each entry carrying either or
/// both of `orientationSupport` / `loraSupport`) plus the two provider-level
/// defaults from `getImageProviderConstraints(provider)`.
///
/// v5 keeps the compiled data in two tables (`image_gen_data`'s
/// `orientation_data_for` and `lora_data_for`) because they were transcribed in
/// different rounds; [`crate::image_gen_data::image_declarations_for`] merges
/// them back into v4's single-list shape, which is what the injected seam
/// carries.
#[derive(Debug, Clone, Default)]
pub struct ImageDeclarations {
    pub models: Vec<ModelInfo>,
    pub orientation_provider: Option<OrientationSupport>,
    pub lora_provider: Option<ImageLoraSupport>,
}

/// The injected declaration seam: `provider -> ImageDeclarations`. Production
/// passes [`crate::image_gen_data::image_declarations_for`]; the differentials
/// pass canned tables (the `orientation_data_for` seam this replaces).
pub type ImageDeclarationsFn = dyn Fn(&str) -> ImageDeclarations + Send + Sync;

/// The `logContext` v4 spreads into this build's log lines. `context` is the
/// call-site label (`'tools.generate_image'`,
/// `'background-jobs.character-avatar'`, …) — nine literals across the five
/// consolidated sites, and the tracing anchors the capture tests key on.
#[derive(Debug, Clone, Default)]
pub struct ImageParamsLogContext {
    pub context: &'static str,
    pub chat_id: Option<String>,
    pub job_id: Option<String>,
    pub profile_id: Option<String>,
}

/// v4 `BuiltImageGenParams`.
pub struct BuiltImageGenParams {
    /// Ready to hand to `provider.generate_image(...)`.
    pub params: ImageGenParams,
    /// What the provider/model declared, or `None` when it declared nothing.
    pub lora_support: Option<ImageLoraSupport>,
    /// The adapters that survived validation and capping.
    pub loras: Vec<ImageLoraSpec>,
    /// Those adapters' trigger phrases, joined; empty when none asks for one.
    pub lora_trigger_phrase: String,
    /// The phrases this build actually appended to the prompt — empty when the
    /// prompt already carried them (the crafter having done the honours) or
    /// when no adapter asks for one.
    pub appended_trigger_phrases: Vec<String>,
    /// The resolved orientation, or `None` when the caller asked for none.
    pub orientation: Option<ResolvedOrientation>,
}

/// `a || b` for the fields whose original merge used `||` — an empty string
/// from the caller means "unset", falling through to the profile's default
/// (v4 `firstNonEmptyString`).
fn first_non_empty_string(values: [Option<&str>; 3]) -> Option<String> {
    for v in values.into_iter().flatten() {
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    None
}

/// v4 `asNumber`: a stored default counts only when it is a real, finite JSON
/// number. A numeric STRING does not — the old `mergeParameters` cast without
/// checking, this does not.
fn as_number(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|f| f.is_finite())
}

fn str_default<'a>(defaults: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a str> {
    defaults.and_then(|d| d.get(key)).and_then(Value::as_str)
}

/// Resolve the LoRA story for a profile without building the whole request
/// (v4 `resolveProfileLoras`).
///
/// The `generate_image` path needs the trigger phrases *before* it expands the
/// prompt (the crafter has to see them), which is earlier than it can build
/// the params. Same resolution, same capping, same logs — the builder calls
/// this itself, so the two can never disagree.
pub fn resolve_profile_loras(
    profile: ImageProfileLike<'_>,
    declarations: &ImageDeclarations,
    log_context: &ImageParamsLogContext,
) -> (Option<ImageLoraSupport>, Vec<ImageLoraSpec>, String) {
    // v4 `{ provider, model, ...logContext }` (params-builder.ts:150) — the
    // caller's own fields ride into every `[Image LoRA]` line.
    let context = LoraLogContext {
        provider: profile.provider.to_string(),
        model: profile.model_name.map(str::to_string),
        context: log_context.context,
        chat_id: log_context.chat_id.clone(),
        job_id: log_context.job_id.clone(),
        profile_id: log_context.profile_id.clone(),
    };
    let support = resolve_lora_support(
        Some(&declarations.models),
        profile.model_name,
        declarations.lora_provider.as_ref(),
    );
    let stored = read_loras_from_parameters(profile.parameters, &context);
    let loras = cap_loras(stored, support.as_ref(), &context);
    let phrase = join_lora_trigger_phrases(&loras);
    (support, loras, phrase)
}

/// Build the parameters for one image generation call
/// (v4 `buildImageGenParams`).
///
/// `orientation` is the semantic shape intent; `None` leaves `size`/
/// `aspectRatio` exactly as the merge produced them (the `POST /api/v1/images`
/// route, whose caller passes an explicit size and means it).
pub fn build_image_gen_params(
    profile: ImageProfileLike<'_>,
    prompt: &str,
    overrides: &ImageGenOverrides,
    orientation: Option<Orientation>,
    fallback_model: &str,
    declarations: &ImageDeclarations,
    log_context: &ImageParamsLogContext,
) -> BuiltImageGenParams {
    let defaults = profile.parameters.and_then(Value::as_object);

    // ---- 1. Merge, preserving the original mergeParameters semantics -------
    let model = first_non_empty_string([
        overrides.model.as_deref(),
        profile.model_name,
        str_default(defaults, "model"),
    ])
    .unwrap_or_else(|| fallback_model.to_string());

    let mut params = ImageGenParams {
        prompt: prompt.to_string(),
        model,
        // `overrides.n ?? asNumber(defaults.n) ?? 1`.
        n: Some(
            overrides
                .n
                .or_else(|| as_number(defaults.and_then(|d| d.get("n"))))
                .unwrap_or(1.0),
        ),
        ..Default::default()
    };

    params.negative_prompt = first_non_empty_string([
        overrides.negative_prompt.as_deref(),
        str_default(defaults, "negativePrompt"),
        None,
    ]);
    params.size = first_non_empty_string([
        overrides.size.as_deref(),
        str_default(defaults, "size"),
        None,
    ]);
    params.aspect_ratio = first_non_empty_string([
        overrides.aspect_ratio.as_deref(),
        str_default(defaults, "aspectRatio"),
        None,
    ]);
    params.quality = first_non_empty_string([
        overrides.quality.as_deref(),
        str_default(defaults, "quality"),
        None,
    ]);
    params.style = first_non_empty_string([
        overrides.style.as_deref(),
        str_default(defaults, "style"),
        None,
    ]);
    params.response_format = first_non_empty_string([
        overrides.response_format.as_deref(),
        str_default(defaults, "responseFormat"),
        None,
    ]);
    params.seed = overrides
        .seed
        .or_else(|| as_number(defaults.and_then(|d| d.get("seed"))));
    params.guidance_scale = overrides
        .guidance_scale
        .or_else(|| as_number(defaults.and_then(|d| d.get("guidanceScale"))));
    params.steps = overrides
        .steps
        .or_else(|| as_number(defaults.and_then(|d| d.get("steps"))));

    // ---- 2. Orientation ----------------------------------------------------
    let mut resolved_orientation: Option<ResolvedOrientation> = None;
    if let Some(orientation) = orientation {
        let resolved = resolve_orientation(
            Some(&declarations.models),
            Some(&params.model),
            declarations.orientation_provider.as_ref(),
            orientation,
        );
        // Orientation outranks any raw size/aspectRatio that arrived above: the
        // caller asked for a shape, not for a string.
        if let Some(size) = &resolved.size {
            // A size the merge never set is INSERTED here — after `steps` in
            // v4's object (see `ImageGenParams::size_inserted_by_orientation`).
            params.size_inserted_by_orientation = params.size.is_none();
            params.size = Some(size.clone());
        }
        if let Some(ar) = &resolved.aspect_ratio {
            params.aspect_ratio_inserted_by_orientation = params.aspect_ratio.is_none();
            params.aspect_ratio = Some(ar.clone());
        }
        if !resolved.prompt_hint.is_empty() {
            params.prompt = format!("{}\n\n{}", params.prompt, resolved.prompt_hint);
        }
        resolved_orientation = Some(resolved);
    }

    // ---- 3. LoRAs ----------------------------------------------------------
    let (support, loras, trigger_phrase) =
        resolve_profile_loras(profile, declarations, log_context);
    let mut appended_trigger_phrases: Vec<String> = Vec::new();
    if !loras.is_empty() {
        params.loras = loras.clone();

        // A LoRA's trigger phrase has to reach the prompt or the adapter fires
        // at half strength. The `generate_image` path already hands the phrases
        // to the prompt crafter through the same seam a style's trigger phrase
        // uses, so the crafted prompt usually carries them — but crafting is
        // skipped when there is nothing to expand, and it can fall back to
        // plain substitution when it fails. Rather than thread a "did the
        // crafter get them?" flag through five call sites and be wrong on the
        // fallback, look: append only the phrases the prompt does not already
        // say.
        let haystack = params.prompt.to_lowercase();
        for phrase in lora_trigger_phrases(&loras) {
            if !haystack.contains(&phrase.to_lowercase()) {
                appended_trigger_phrases.push(phrase);
            }
        }
        if !appended_trigger_phrases.is_empty() {
            params.prompt = format!(
                "{}\n\n{}",
                params.prompt,
                appended_trigger_phrases.join(", ")
            );
        }
    }

    // ---- 4. Residual bag ---------------------------------------------------
    let mut profile_parameters = Map::new();
    if let Some(defaults) = defaults {
        for (key, value) in defaults {
            if HOST_OWNED_PARAMETER_KEYS.contains(&key.as_str()) {
                continue;
            }
            // v4 `if (value === undefined) continue` — unreachable through
            // JSON.parse, but an explicit null IS carried (it is not
            // `undefined`), so no null filter here.
            profile_parameters.insert(key.clone(), value.clone());
        }
    }
    if !profile_parameters.is_empty() {
        params.profile_parameters = Some(Value::Object(profile_parameters.clone()));
    }

    tracing::debug!(
        context = %log_context.context,
        chat_id = ?log_context.chat_id,
        job_id = ?log_context.job_id,
        profile_id = ?log_context.profile_id,
        provider = %profile.provider,
        model = %params.model,
        orientation = ?orientation,
        size = ?params.size,
        aspect_ratio = ?params.aspect_ratio,
        n = ?params.n,
        lora_support = ?support.as_ref().map(|s| (s.max_loras, s.source_kinds.clone())),
        lora_count = loras.len(),
        lora_sources = ?loras.iter().map(|l| l.source.clone()).collect::<Vec<_>>(),
        lora_trigger_phrase = ?(if trigger_phrase.is_empty() { None } else { Some(&trigger_phrase) }),
        appended_trigger_phrases = ?appended_trigger_phrases,
        // Key NAMES only for the residual bag — the same discipline the
        // NanoGPT wire log keeps, so a `hf_api_token` never reaches a log line.
        profile_parameter_keys = ?profile_parameters.keys().collect::<Vec<_>>(),
        "[Image Params] Built image generation parameters"
    );

    BuiltImageGenParams {
        params,
        lora_support: support,
        loras,
        lora_trigger_phrase: trigger_phrase,
        appended_trigger_phrases,
        orientation: resolved_orientation,
    }
}
