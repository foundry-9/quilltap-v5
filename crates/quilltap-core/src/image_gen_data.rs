//! Image-generation orientation/constraint declarations (W4.7f) — the real
//! per-provider data v4's plugin registry exposes via `getImageGenerationModels`
//! (per-model `orientationSupport`) and `getImageProviderConstraints`
//! (provider-level `orientationSupport`), transcribed as a compiled-constant
//! module (the tool-catalog / `SAMPLING_PARAMS_REJECTED_MODELS` precedent — NOT a
//! W4.7a manifest-schema bump).
//!
//! Consumed by [`crate::image_gen::resolve_orientation`] via
//! [`orientation_data_for`], which the `generate_image` handler injects. Both
//! declaration LEVELS matter: the resolver checks the per-model list FIRST, then
//! falls back to the provider-level default. OPENAI/GOOGLE/OPENROUTER declare
//! per-model; GROK/Z_AI declare provider-level only.
//!
//! Source (v4 `6b6e39ad`): `plugins/dist/qtap-plugin-{openai,google,grok,
//! openrouter,z-ai}/index.ts`. The transcription is verified against v4's REAL
//! `resolveOrientation` in `image_dialects_equivalence`.

use crate::image_gen::lora_support::ImageLoraSupport;
use crate::image_gen::params_builder::ImageDeclarations;
use crate::image_gen::{ModelInfo, OrientationMapping, OrientationStrategy, OrientationSupport};
use crate::model::nanogpt_loras::{nanogpt_lora_families, NANOGPT_FLAGSHIP_IMAGE_MODEL_IDS};

/// A `{ size, nominalWidth, nominalHeight }` mapping (the OpenAI / Z-AI shape).
fn size_map(size: &str, w: f64, h: f64) -> OrientationMapping {
    OrientationMapping {
        size: Some(size.to_string()),
        nominal_width: Some(w),
        nominal_height: Some(h),
        ..Default::default()
    }
}

/// An `{ aspectRatio }` mapping (the Google / Grok / OpenRouter shape).
fn aspect_map(ratio: &str) -> OrientationMapping {
    OrientationMapping {
        aspect_ratio: Some(ratio.to_string()),
        ..Default::default()
    }
}

/// The shared aspect-ratio orientation (portrait 3:4 / landscape 16:9 / square
/// 1:1) — v4's `aspectOrientation` in google / grok / openrouter.
fn aspect_orientation() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::AspectRatio,
        portrait: aspect_map("3:4"),
        landscape: aspect_map("16:9"),
        square: Some(aspect_map("1:1")),
    }
}

/// v4 OpenAI `gptImageOrientation`.
fn gpt_image_orientation() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: size_map("1024x1536", 1024.0, 1536.0),
        landscape: size_map("1536x1024", 1536.0, 1024.0),
        square: Some(size_map("1024x1024", 1024.0, 1024.0)),
    }
}

/// v4 OpenAI `dalle3Orientation`.
fn dalle3_orientation() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: size_map("1024x1792", 1024.0, 1792.0),
        landscape: size_map("1792x1024", 1792.0, 1024.0),
        square: Some(size_map("1024x1024", 1024.0, 1024.0)),
    }
}

/// v4 OpenAI `dalle2Orientation` — square only; portrait/landscape are the
/// intentional EMPTY mappings that degrade to a prompt hint.
fn dalle2_orientation() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: OrientationMapping::default(),
        landscape: OrientationMapping::default(),
        square: Some(size_map("1024x1024", 1024.0, 1024.0)),
    }
}

/// v4 Z-AI `Z_AI_IMAGE_CONSTRAINTS.orientationSupport` (provider-level).
fn zai_orientation() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: size_map("1056x1568", 1056.0, 1568.0),
        landscape: size_map("1568x1056", 1568.0, 1056.0),
        square: Some(size_map("1024x1024", 1024.0, 1024.0)),
    }
}

/// P4.D101 — NanoGPT's `NANOGPT_IMAGE_CONSTRAINTS.orientationSupport`
/// (`index.ts`): hidream's native ≈2:3 pair for portrait/landscape and a square
/// 1024. Provider-level only, like Z.AI and Grok — NanoGPT declares no
/// per-model orientation.
fn nanogpt_orientation() -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: size_map("832x1248", 832.0, 1248.0),
        landscape: size_map("1248x832", 1248.0, 832.0),
        square: Some(size_map("1024x1024", 1024.0, 1024.0)),
    }
}

fn model(id: &str, support: OrientationSupport) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        orientation_support: Some(support),
        lora_support: None,
    }
}

/// The injected orientation data for a provider: `(per-model info,
/// provider-level default support)`. Mirrors v4's `getImageGenerationModels` +
/// `getImageProviderConstraints(...).orientationSupport`.
pub fn orientation_data_for(provider: &str) -> (Vec<ModelInfo>, Option<OrientationSupport>) {
    match provider {
        "OPENAI" => (
            vec![
                model("gpt-image-2", gpt_image_orientation()),
                model("gpt-image-1.5", gpt_image_orientation()),
                model("gpt-image-1", gpt_image_orientation()),
                model("gpt-image-1-mini", gpt_image_orientation()),
                model("dall-e-3", dalle3_orientation()),
                model("dall-e-2", dalle2_orientation()),
            ],
            None,
        ),
        "GOOGLE" => (
            vec![
                model("gemini-2.5-flash-image", aspect_orientation()),
                model("gemini-3-pro-image-preview", aspect_orientation()),
                model("imagen-4", aspect_orientation()),
                model("imagen-4-fast", aspect_orientation()),
            ],
            None,
        ),
        "OPENROUTER" => (
            vec![
                model("google/gemini-2.0-flash-exp:free", aspect_orientation()),
                model(
                    "google/gemini-2.5-flash-preview-05-20",
                    aspect_orientation(),
                ),
                model(
                    "google/gemini-2.5-flash-preview-native-image",
                    aspect_orientation(),
                ),
                model("google/gemini-3-pro-image-preview", aspect_orientation()),
            ],
            None,
        ),
        // Provider-level only.
        "GROK" => (Vec::new(), Some(aspect_orientation())),
        "Z_AI" => (Vec::new(), Some(zai_orientation())),
        // NanoGPT declares no per-model ORIENTATION, but since `84f33ce94` it
        // does declare `getImageGenerationModels` (for the LoRA half), and this
        // function transcribes that list — so the ids appear here carrying
        // `orientation_support: None`, exactly as v4's entries do. Behaviour is
        // unchanged: `match_model` finds an entry with no orientation support
        // and `resolve_orientation` falls through to the provider-level
        // constraint, which is where NanoGPT's shape has always lived.
        "NANOGPT" => (nanogpt_model_ids(), Some(nanogpt_orientation())),
        _ => (Vec::new(), None),
    }
}

/// The injected **LoRA** declarations for a provider: `(per-model info,
/// provider-level default support)` — v4 `getImageGenerationModels(provider)`'s
/// `loraSupport` plus `getImageProviderConstraints(provider)?.loraSupport`
/// (`84f33ce94`). Read by
/// [`crate::image_gen::lora_support::resolve_lora_support`] through the SAME
/// [`crate::image_gen::match_model`] the orientation resolver walks.
///
/// NanoGPT is the only declaring provider: its curated image list carries one
/// entry per LoRA family (generated from the dialect table, exactly as v4's
/// `models.ts` generates `STATIC_IMAGE_MODELS` from `NANOGPT_LORA_FAMILIES`),
/// and the flagships declare nothing. `NANOGPT_IMAGE_CONSTRAINTS` declares no
/// provider-level `loraSupport`, so an unknown NanoGPT model resolves `None`.
///
/// ⚠ **Narrower than v4 by one arm, deliberately.** v4's
/// `getNanoGPTImageModels()` also augments this list from the module-level
/// detailed-catalog cache, giving a live-catalog `lora`-tagged model outside
/// the table `{ maxLoras: 1, sourceKinds: ['url','hf-repo'] }`. v5 carries that
/// arm too, but the cache is runtime state rather than compiled data (the
/// `model::nanogpt_catalog` module).
pub fn lora_data_for(provider: &str) -> (Vec<ModelInfo>, Option<ImageLoraSupport>) {
    match provider {
        "NANOGPT" => {
            let mut models = nanogpt_model_ids();
            for family in nanogpt_lora_families() {
                if let Some(entry) = models.iter_mut().find(|m| m.id == family.prefix) {
                    entry.lora_support = Some(family.support);
                }
            }
            (models, None)
        }
        _ => (Vec::new(), None),
    }
}

/// v4 `models.ts` `STATIC_IMAGE_MODEL_IDS` in ORDER — the six documented
/// flagships followed by one entry per LoRA family, which is exactly what
/// `STATIC_IMAGE_MODELS.map(m => m.id)` yields since `84f33ce94`.
fn nanogpt_model_ids() -> Vec<ModelInfo> {
    NANOGPT_FLAGSHIP_IMAGE_MODEL_IDS
        .iter()
        .map(|id| (*id).to_string())
        .chain(
            nanogpt_lora_families()
                .into_iter()
                .map(|f| f.prefix.to_string()),
        )
        .map(|id| ModelInfo {
            id,
            ..Default::default()
        })
        .collect()
}

/// v4's ONE `getImageGenerationModels(provider)` list plus BOTH provider-level
/// defaults, merged from the two compiled tables above (`84f33ce94`).
///
/// v4 keeps a single declaration list whose entries may carry either or both of
/// `orientationSupport` / `loraSupport`; v5 transcribed the two capabilities in
/// different rounds, so this merges them back by model id — an id declared in
/// both tables becomes one [`ModelInfo`] carrying both supports, which is what
/// [`crate::image_gen::match_model`] must see for "one matcher, two
/// capabilities" to hold.
///
/// This is the production value of the injected
/// [`ImageDeclarationsFn`](crate::image_gen::params_builder::ImageDeclarationsFn)
/// seam.
pub fn image_declarations_for(provider: &str) -> ImageDeclarations {
    let (orientation_models, orientation_provider) = orientation_data_for(provider);
    let (lora_models, lora_provider) = lora_data_for(provider);

    let mut models = orientation_models;
    for lora in lora_models {
        match models.iter_mut().find(|m| m.id == lora.id) {
            Some(existing) => existing.lora_support = lora.lora_support,
            None => models.push(lora),
        }
    }

    ImageDeclarations {
        models,
        orientation_provider,
        lora_provider,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_gen::{resolve_orientation, Orientation};

    #[test]
    fn dalle2_portrait_degrades_to_hint() {
        let (models, prov) = orientation_data_for("OPENAI");
        let r = resolve_orientation(
            Some(&models),
            Some("dall-e-2"),
            prov.as_ref(),
            Orientation::Portrait,
        );
        // Empty portrait mapping → not authoritative → generic host hint.
        assert!(!r.dimensions_authoritative);
        assert_eq!(
            r.prompt_hint,
            "vertical portrait composition, taller than wide"
        );
    }

    #[test]
    fn grok_provider_level_aspect() {
        let (models, prov) = orientation_data_for("GROK");
        assert!(models.is_empty());
        let r = resolve_orientation(
            Some(&models),
            Some("grok-imagine-image"),
            prov.as_ref(),
            Orientation::Landscape,
        );
        assert_eq!(r.aspect_ratio.as_deref(), Some("16:9"));
        assert!(r.dimensions_authoritative);
    }

    #[test]
    fn zai_provider_level_size() {
        let (models, prov) = orientation_data_for("Z_AI");
        let r = resolve_orientation(
            Some(&models),
            Some("glm-image"),
            prov.as_ref(),
            Orientation::Portrait,
        );
        assert_eq!(r.size.as_deref(), Some("1056x1568"));
    }

    #[test]
    fn openai_dalle3_size() {
        let (models, prov) = orientation_data_for("OPENAI");
        let r = resolve_orientation(
            Some(&models),
            Some("dall-e-3"),
            prov.as_ref(),
            Orientation::Landscape,
        );
        assert_eq!(r.size.as_deref(), Some("1792x1024"));
        assert_eq!(r.nominal_width, Some(1792.0));
    }
}
