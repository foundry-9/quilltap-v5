//! Image-generation pure leaves (v4 `lib/image-gen/`) — the decision-free helpers
//! the `generate_image` tool orchestration composes. Ported leaf-first (the
//! discipline of pure-to-stateful); the stateful `executeImageGenerationTool`
//! handler + `saveGeneratedImage` persistence remain a scoped deferral (see the
//! module note below).
//!
//! Ported so far:
//!   - `resolveOrientation` (`orientation.ts`): the pure `(provider, model,
//!     orientation)` → concrete-request-mutation mapping. The plugin-registry
//!     declarations (`getImageGenerationModels` / `getImageProviderConstraints`)
//!     are the external boundary — passed in as data (`ModelInfo` / provider-level
//!     `OrientationSupport`), so `matchModel` (exact + longest-prefix) + `realize`
//!     (strategy honouring + degrade-to-hint) are exercised directly.
//!   - `parsePlaceholders` (`prompt-expansion.ts`): the `{{name}}` scanner.
//!
//! ## Scoped deferral: the `generate_image` handler
//!
//! The full `executeImageGenerationTool` orchestration + `saveGeneratedImage`
//! persistence are NOT ported here. They compose a large set of unported /
//! host-side subsystems — the image-provider call + `convertToWebP` + the Lantern
//! store bridge + `postLanternImageNotification` (host seams), the entire W4.2
//! dangerous-content classify/route path (with a double image-profile reroute),
//! and three cheap-LLM tasks (`craftImagePrompt`, `resolveCharacterAppearances`,
//! `sanitizeAppearance`) — several of which are themselves large unported units.
//! The handler lands once those seams/subsystems exist; these leaves are banked
//! against it now.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::db::js_number_to_json;

pub mod huggingface_lookup;
pub mod huggingface_repo_id;
pub mod lora_support;
pub mod lora_validation;
pub mod params_builder;

pub use lora_support::{ImageLoraSpec, ImageLoraSupport, LoraScale};

// ===========================================================================
// prompt-expansion: parsePlaceholders
// ===========================================================================

/// One `{{name}}` placeholder found in a prompt (v4 `parsePlaceholders`'s element).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Placeholder {
    pub placeholder: String,
    pub name: String,
}

/// v4 `parsePlaceholders`: find all `{{name}}` patterns. Regex `/\{\{([^}]+)\}\}/g`
/// — `placeholder` is the full `{{…}}` match; `name` is the captured group,
/// `.trim()`-ed (JS whitespace ≈ Rust `str::trim` for the ASCII corpus).
pub fn parse_placeholders(prompt: &str) -> Vec<Placeholder> {
    let bytes = prompt.as_bytes();
    let n = bytes.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < n {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            // Scan the name: one-or-more chars that are not `}`.
            let name_start = i + 2;
            let mut j = name_start;
            while j < n && bytes[j] != b'}' {
                j += 1;
            }
            // Need a non-empty name AND a `}}` close.
            if j > name_start && j + 1 < n && bytes[j] == b'}' && bytes[j + 1] == b'}' {
                out.push(Placeholder {
                    placeholder: prompt[i..j + 2].to_string(),
                    name: prompt[name_start..j].trim().to_string(),
                });
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

// ===========================================================================
// orientation: resolveOrientation
// ===========================================================================

/// v4 `OrientationStrategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientationStrategy {
    Size,
    AspectRatio,
    Prompt,
}

/// v4 `OrientationMapping` — how a provider realises one orientation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OrientationMapping {
    pub size: Option<String>,
    pub aspect_ratio: Option<String>,
    pub prompt_hint: Option<String>,
    pub nominal_width: Option<f64>,
    pub nominal_height: Option<f64>,
}

/// v4 `ImageOrientationSupport` — strategy + per-orientation mappings.
#[derive(Debug, Clone, PartialEq)]
pub struct OrientationSupport {
    pub strategy: OrientationStrategy,
    pub portrait: OrientationMapping,
    pub landscape: OrientationMapping,
    pub square: Option<OrientationMapping>,
}

impl OrientationSupport {
    fn get(&self, orientation: Orientation) -> Option<&OrientationMapping> {
        match orientation {
            Orientation::Portrait => Some(&self.portrait),
            Orientation::Landscape => Some(&self.landscape),
            Orientation::Square => self.square.as_ref(),
        }
    }
}

/// The subset of v4 `ImageGenerationModelInfo` the host resolvers read.
///
/// `84f33ce94` added `loraSupport` beside `orientationSupport` — one matcher,
/// two capabilities: [`match_model`] answers both questions, so a family prefix
/// that resolves an orientation resolves a LoRA capability the same way.
#[derive(Debug, Clone, Default)]
pub struct ModelInfo {
    pub id: String,
    pub orientation_support: Option<OrientationSupport>,
    pub lora_support: Option<ImageLoraSupport>,
}

/// v4 `ImageOrientation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
    Square,
}

impl Orientation {
    /// The generic host-fallback hint (v4 `HOST_FALLBACK_HINTS`).
    fn fallback_hint(self) -> &'static str {
        match self {
            Orientation::Portrait => "vertical portrait composition, taller than wide",
            Orientation::Landscape => "wide landscape composition, wider than tall",
            Orientation::Square => "square composition",
        }
    }
}

/// v4 `ResolvedOrientation`. Serialized in v4 field order: `params, promptHint,
/// dimensionsAuthoritative, nominalWidth?, nominalHeight?` (undefined nominal dims
/// dropped). `params` is `{size?}` / `{aspectRatio?}` / `{}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedOrientation {
    pub size: Option<String>,
    pub aspect_ratio: Option<String>,
    pub prompt_hint: String,
    pub dimensions_authoritative: bool,
    pub nominal_width: Option<f64>,
    pub nominal_height: Option<f64>,
}

impl Serialize for ResolvedOrientation {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut len = 2; // params, promptHint, dimensionsAuthoritative
        len += 1;
        if self.nominal_width.is_some() {
            len += 1;
        }
        if self.nominal_height.is_some() {
            len += 1;
        }
        let mut st = s.serialize_struct("ResolvedOrientation", len)?;
        st.serialize_field(
            "params",
            &Params {
                size: &self.size,
                aspect_ratio: &self.aspect_ratio,
            },
        )?;
        st.serialize_field("promptHint", &self.prompt_hint)?;
        st.serialize_field("dimensionsAuthoritative", &self.dimensions_authoritative)?;
        if let Some(w) = self.nominal_width {
            st.serialize_field("nominalWidth", &js_number_to_json(w))?;
        }
        if let Some(h) = self.nominal_height {
            st.serialize_field("nominalHeight", &js_number_to_json(h))?;
        }
        st.end()
    }
}

/// The `params` sub-object (`{size?}` / `{aspectRatio?}` / `{}`).
struct Params<'a> {
    size: &'a Option<String>,
    aspect_ratio: &'a Option<String>,
}
impl Serialize for Params<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut len = 0;
        if self.size.is_some() {
            len += 1;
        }
        if self.aspect_ratio.is_some() {
            len += 1;
        }
        let mut st = s.serialize_struct("Params", len)?;
        if let Some(sz) = self.size {
            st.serialize_field("size", sz)?;
        }
        if let Some(ar) = self.aspect_ratio {
            st.serialize_field("aspectRatio", ar)?;
        }
        st.end()
    }
}

/// v4 `matchModel`: exact id match wins; else the longest-prefix family match.
///
/// `pub` since `84f33ce94`: `resolveLoraSupport` walks the SAME matcher
/// `resolveOrientation` does, and v4's comment for that is explicit — one
/// matcher, two capabilities.
pub fn match_model<'a>(
    models: Option<&'a [ModelInfo]>,
    model: Option<&str>,
) -> Option<&'a ModelInfo> {
    let (models, model) = (models?, model?);
    if let Some(exact) = models.iter().find(|m| m.id == model) {
        return Some(exact);
    }
    models
        .iter()
        .filter(|m| model.starts_with(&m.id))
        // longest id first (stable — `max_by_key` keeps the LAST max; v4's
        // sort-desc keeps the FIRST of equal lengths, but ids are distinct here).
        .max_by_key(|m| m.id.len())
}

/// v4 `realize`: turn a concrete mapping into a `ResolvedOrientation`, honouring
/// the strategy; a size/aspectRatio strategy whose mapping lacks the concrete
/// field degrades to a prompt hint.
fn realize(
    strategy: OrientationStrategy,
    mapping: &OrientationMapping,
    fallback_hint: &str,
) -> ResolvedOrientation {
    let nw = mapping.nominal_width;
    let nh = mapping.nominal_height;
    match strategy {
        OrientationStrategy::Size if mapping.size.is_some() => ResolvedOrientation {
            size: mapping.size.clone(),
            aspect_ratio: None,
            prompt_hint: String::new(),
            dimensions_authoritative: true,
            nominal_width: nw,
            nominal_height: nh,
        },
        OrientationStrategy::AspectRatio if mapping.aspect_ratio.is_some() => ResolvedOrientation {
            size: None,
            aspect_ratio: mapping.aspect_ratio.clone(),
            prompt_hint: String::new(),
            dimensions_authoritative: true,
            nominal_width: nw,
            nominal_height: nh,
        },
        // 'prompt' strategy, or a size/aspectRatio mapping carrying only a hint.
        _ => ResolvedOrientation {
            size: None,
            aspect_ratio: None,
            // `mapping.promptHint ?? fallbackHint`.
            prompt_hint: mapping
                .prompt_hint
                .clone()
                .unwrap_or_else(|| fallback_hint.to_string()),
            dimensions_authoritative: false,
            nominal_width: nw,
            nominal_height: nh,
        },
    }
}

/// v4 `resolveOrientation`. `models` / `provider_support` are the injected
/// plugin-registry declarations (`getImageGenerationModels(provider)` /
/// `getImageProviderConstraints(provider)?.orientationSupport`). Always returns a
/// usable result (never fails) thanks to the host fallback.
pub fn resolve_orientation(
    models: Option<&[ModelInfo]>,
    model: Option<&str>,
    provider_support: Option<&OrientationSupport>,
    orientation: Orientation,
) -> ResolvedOrientation {
    let fallback_hint = orientation.fallback_hint();

    // 1. Per-model override, then 2. provider-level default.
    let support: Option<&OrientationSupport> = match_model(models, model)
        .and_then(|m| m.orientation_support.as_ref())
        .or(provider_support);

    if let Some(support) = support {
        if let Some(mapping) = support.get(orientation) {
            return realize(support.strategy, mapping, fallback_hint);
        }
        // Orientation declared-but-absent → fall through to the generic hint.
    }

    // 3. Host fallback — generic prompt hints, never authoritative, no nominal.
    ResolvedOrientation {
        size: None,
        aspect_ratio: None,
        prompt_hint: fallback_hint.to_string(),
        dimensions_authoritative: false,
        nominal_width: None,
        nominal_height: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_placeholders_scans() {
        assert_eq!(
            parse_placeholders("A scene with {{me}} and {{Aurora}}."),
            vec![
                Placeholder {
                    placeholder: "{{me}}".into(),
                    name: "me".into()
                },
                Placeholder {
                    placeholder: "{{Aurora}}".into(),
                    name: "Aurora".into()
                },
            ]
        );
        assert_eq!(parse_placeholders("no placeholders here"), vec![]);
        // The name is trimmed (v4 `match[1].trim()`); the placeholder is the full match.
        assert_eq!(
            parse_placeholders("{{ me }}"),
            vec![Placeholder {
                placeholder: "{{ me }}".into(),
                name: "me".into()
            }]
        );
        // Adjacent.
        assert_eq!(parse_placeholders("{{a}}{{b}}").len(), 2);
        // Empty name `{{}}` does not match (`[^}]+` requires ≥1).
        assert_eq!(parse_placeholders("{{}}"), vec![]);
    }

    #[test]
    fn orientation_host_fallback() {
        let r = resolve_orientation(None, None, None, Orientation::Portrait);
        assert_eq!(
            r.prompt_hint,
            "vertical portrait composition, taller than wide"
        );
        assert!(!r.dimensions_authoritative);
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"params":{},"promptHint":"vertical portrait composition, taller than wide","dimensionsAuthoritative":false}"#
        );
    }

    #[test]
    fn orientation_size_strategy_and_prefix_match() {
        let models = vec![ModelInfo {
            id: "dall-e-3".into(),
            lora_support: None,
            orientation_support: Some(OrientationSupport {
                strategy: OrientationStrategy::Size,
                portrait: OrientationMapping {
                    size: Some("1024x1792".into()),
                    nominal_width: Some(1024.0),
                    nominal_height: Some(1792.0),
                    ..Default::default()
                },
                landscape: OrientationMapping {
                    size: Some("1792x1024".into()),
                    ..Default::default()
                },
                square: Some(OrientationMapping {
                    size: Some("1024x1024".into()),
                    ..Default::default()
                }),
            }),
        }];
        // Longest-prefix family match: "dall-e-3-mini" → "dall-e-3".
        let r = resolve_orientation(
            Some(&models),
            Some("dall-e-3-mini"),
            None,
            Orientation::Portrait,
        );
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"params":{"size":"1024x1792"},"promptHint":"","dimensionsAuthoritative":true,"nominalWidth":1024,"nominalHeight":1792}"#
        );
    }
}
