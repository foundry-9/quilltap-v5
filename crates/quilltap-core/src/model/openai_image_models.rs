//! The OpenAI image-model capability table (v4 `d8d2890ee`,
//! `plugins/dist/qtap-plugin-openai/image-models.ts`) — the one source of what
//! each Images-API family accepts.
//!
//! v4's header names three consumers and says none of them re-derives any of
//! it; v5 has four, for the same reason:
//!
//!   - [`crate::model::image_dialects`]'s `build_openai` / the OPENAI response
//!     parse, to decide which parameters reach the wire and to validate a
//!     requested size;
//!   - [`crate::model::image_dialects::supported_image_models`] (v4
//!     `OpenAIImageProvider.supportedModels`);
//!   - [`crate::image_gen_data::orientation_data_for`] (v4 `index.ts`'s
//!     `getImageGenerationModels()`), which adds only the orientation mappings;
//!   - [`crate::model::openai_image_options`] (v4 `image-options-schema.ts`),
//!     the image-profile editor's fields.
//!
//! Families are matched by LONGEST PREFIX, so dated snapshots ride their
//! family's entry for free (`gpt-image-2.5-flare-2026-09-08` →
//! `gpt-image-2.5-flare`) and the overlapping ids resolve the way you would
//! hope: `gpt-image-2.5-sunburst` beats `gpt-image-2`, and `gpt-image-1-mini`
//! beats `gpt-image-1`.

use crate::jsstr::js_trim;

/// Wire formats the GPT Image families can return (v4
/// `OpenAIImageOutputFormat`).
pub const OUTPUT_FORMATS: [&str; 3] = ["png", "jpeg", "webp"];
/// Background treatments the GPT Image families accept (v4
/// `OpenAIImageBackground`).
pub const BACKGROUNDS: [&str; 3] = ["auto", "transparent", "opaque"];
/// v4 `MODERATIONS`.
pub const MODERATIONS: [&str; 2] = ["auto", "low"];
/// Formats that can carry an alpha channel, so a transparent background
/// survives (v4 `TRANSPARENCY_CAPABLE_FORMATS`).
pub const TRANSPARENCY_CAPABLE_FORMATS: [&str; 2] = ["png", "webp"];
/// Formats to which `output_compression` applies (v4 `COMPRESSIBLE_FORMATS`).
pub const COMPRESSIBLE_FORMATS: [&str; 2] = ["jpeg", "webp"];

/// One family's capabilities (v4 `OpenAIImageModelCapabilities`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OpenAIImageModel {
    /// Family id, and the prefix every dated snapshot of it starts with.
    pub id: &'static str,
    /// Display name for the editor and the host's model list.
    pub name: &'static str,
    /// True for the `gpt-image-*` families, which always return base64, accept
    /// `background` / `output_format` / `output_compression` / `moderation`,
    /// and reject `response_format` and `style`.
    pub gpt_image: bool,
    /// Quality tiers this family accepts, in editor order.
    pub qualities: &'static [&'static str],
    /// Standard sizes, offered in the editor and declared to the host.
    pub sizes: &'static [&'static str],
    /// True where the API takes any `WIDTHxHEIGHT` satisfying
    /// [`ARBITRARY_SIZE_RULES`] — GPT Image 2 and both GPT Image 2.5 models.
    pub arbitrary_sizes: bool,
    /// `style` is DALL·E 3's alone.
    pub supports_style: bool,
    /// Ceiling on `n` for one request.
    pub max_n: f64,
}

/// The documented limits on an arbitrary `WIDTHxHEIGHT` request (v4
/// `ARBITRARY_SIZE_RULES`). OpenAI reserves the right to apply tighter
/// per-model pixel and edge limits on top of these, so a size that clears them
/// all is forwarded rather than guaranteed.
pub mod arbitrary_size_rules {
    /// Both edges must be divisible by this.
    pub const EDGE_MULTIPLE: f64 = 16.0;
    /// Longest edge over shortest may not exceed this (1:3 … 3:1).
    pub const MAX_ASPECT: f64 = 3.0;
    /// No edge may exceed the long side of the maximum resolution.
    pub const MAX_EDGE: f64 = 3840.0;
    /// Total pixels may not exceed the maximum resolution's budget.
    pub const MAX_PIXELS: f64 = 3840.0 * 2160.0;
    /// Above this many pixels the docs call the resolution experimental.
    pub const EXPERIMENTAL_ABOVE_PIXELS: f64 = 2560.0 * 1440.0;
}

const GPT_IMAGE_STANDARD_SIZES: &[&str] = &["auto", "1024x1024", "1536x1024", "1024x1536"];

/// Sizes offered in the editor for the arbitrary-resolution families. The API
/// takes far more than these — this is a picker, not the limit.
const GPT_IMAGE_WIDE_SIZES: &[&str] = &[
    "auto",
    "1024x1024",
    "1536x1024",
    "1024x1536",
    "1792x1024",
    "1024x1792",
    "1920x1088",
    "1088x1920",
    "2048x2048",
    "2560x1440",
    "1440x2560",
    "3840x2160",
    "2160x3840",
];

const GPT_IMAGE_QUALITIES: &[&str] = &["auto", "low", "medium", "high"];
const GPT_IMAGE_25_QUALITIES: &[&str] = &["auto", "low", "medium", "high", "xhigh", "max"];

/// Every family the plugin knows, newest first (v4 `OPENAI_IMAGE_MODELS`).
/// `supported_image_models("OPENAI")` and the host's model list are both
/// derived from this, so adding a family here is the whole job.
pub const OPENAI_IMAGE_MODELS: &[OpenAIImageModel] = &[
    OpenAIImageModel {
        id: "gpt-image-2.5-sunburst",
        name: "GPT Image 2.5 Sunburst",
        gpt_image: true,
        qualities: GPT_IMAGE_25_QUALITIES,
        sizes: GPT_IMAGE_WIDE_SIZES,
        arbitrary_sizes: true,
        supports_style: false,
        max_n: 10.0,
    },
    OpenAIImageModel {
        id: "gpt-image-2.5-flare",
        name: "GPT Image 2.5 Flare",
        gpt_image: true,
        qualities: GPT_IMAGE_25_QUALITIES,
        sizes: GPT_IMAGE_WIDE_SIZES,
        arbitrary_sizes: true,
        supports_style: false,
        max_n: 10.0,
    },
    OpenAIImageModel {
        id: "gpt-image-2",
        name: "GPT Image 2",
        gpt_image: true,
        qualities: GPT_IMAGE_QUALITIES,
        sizes: GPT_IMAGE_WIDE_SIZES,
        arbitrary_sizes: true,
        supports_style: false,
        max_n: 10.0,
    },
    OpenAIImageModel {
        id: "gpt-image-1.5",
        name: "GPT Image 1.5",
        gpt_image: true,
        qualities: GPT_IMAGE_QUALITIES,
        sizes: GPT_IMAGE_STANDARD_SIZES,
        arbitrary_sizes: false,
        supports_style: false,
        max_n: 10.0,
    },
    OpenAIImageModel {
        id: "gpt-image-1-mini",
        name: "GPT Image 1 Mini",
        gpt_image: true,
        qualities: GPT_IMAGE_QUALITIES,
        sizes: GPT_IMAGE_STANDARD_SIZES,
        arbitrary_sizes: false,
        supports_style: false,
        max_n: 10.0,
    },
    OpenAIImageModel {
        id: "gpt-image-1",
        name: "GPT Image 1",
        gpt_image: true,
        qualities: GPT_IMAGE_QUALITIES,
        sizes: GPT_IMAGE_STANDARD_SIZES,
        arbitrary_sizes: false,
        supports_style: false,
        max_n: 10.0,
    },
    OpenAIImageModel {
        id: "dall-e-3",
        name: "DALL·E 3",
        gpt_image: false,
        qualities: &["standard", "hd"],
        sizes: &["1024x1024", "1792x1024", "1024x1792"],
        arbitrary_sizes: false,
        supports_style: true,
        max_n: 1.0,
    },
    OpenAIImageModel {
        id: "dall-e-2",
        name: "DALL·E 2",
        gpt_image: false,
        qualities: &["standard"],
        sizes: &["256x256", "512x512", "1024x1024"],
        arbitrary_sizes: false,
        supports_style: false,
        max_n: 10.0,
    },
];

/// Model ids in the order the editor and `supportedModels` list them (v4
/// `OPENAI_IMAGE_MODEL_IDS`).
pub fn openai_image_model_ids() -> Vec<&'static str> {
    OPENAI_IMAGE_MODELS.iter().map(|m| m.id).collect()
}

/// The capability entry governing `model`: exact id first, then the longest
/// family prefix, so dated snapshots resolve without their own entry (v4
/// `findImageModel`). `None` for a model this table does not recognise — a
/// live `/v1/models` listing can name one, and guessing its limits would be
/// worse than forwarding the request untouched.
///
/// The same rule [`crate::image_gen::match_model`] runs over the orientation
/// table, and it carries the tie note: v4 sorts by descending id length and
/// takes the first, so equal-length ids would resolve to the EARLIER table
/// entry. `max_by_key` keeps the LAST maximum, but the ids here are distinct
/// and no two share a length while both prefix the same model, so the two
/// agree. (`gpt-image-1-mini` and `gpt-image-1` differ in length; only one of
/// `gpt-image-2.5-sunburst` / `gpt-image-2.5-flare` can prefix any model.)
pub fn find_image_model(model: Option<&str>) -> Option<&'static OpenAIImageModel> {
    // v4 `if (!model)` — a JS falsy test, so an EMPTY model is "unknown".
    let model = model.filter(|m| !m.is_empty())?;
    if let Some(exact) = OPENAI_IMAGE_MODELS.iter().find(|m| m.id == model) {
        return Some(exact);
    }
    OPENAI_IMAGE_MODELS
        .iter()
        .filter(|m| model.starts_with(m.id))
        .max_by_key(|m| m.id.len())
}

/// Whether `model` belongs to one of the `gpt-image-*` families (v4
/// `isGptImageModel`): the table's verdict, or — for a model it does not know —
/// the bare id prefix.
pub fn is_gpt_image_model(model: Option<&str>) -> bool {
    match find_image_model(model) {
        Some(caps) => caps.gpt_image,
        // v4 `(model ?? '').startsWith('gpt-image-')`.
        None => model.unwrap_or("").starts_with("gpt-image-"),
    }
}

/// A parsed `WIDTHxHEIGHT`. JS `Number()` over a digit run, so the edges are
/// f64: v4 tests them with `Number.isInteger`, which a 21-digit run still
/// passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParsedSize {
    pub width: f64,
    pub height: f64,
}

/// Parse a `WIDTHxHEIGHT` string, or `None` when it is not one (v4
/// `parseSize`). The regex is `/^(\d+)x(\d+)$/` over `size.trim()` — JS `\d`
/// is ASCII-only and `trim` strips the JS whitespace set.
pub fn parse_size(size: &str) -> Option<ParsedSize> {
    let s = js_trim(size);
    let (w, h) = s.split_once('x')?;
    if w.is_empty() || h.is_empty() {
        return None;
    }
    if !w.bytes().all(|b| b.is_ascii_digit()) || !h.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // `Number("0012")` is 12; a run too long for f64 saturates to infinity,
    // which `Number.isInteger` then refuses exactly as v4 does.
    let width: f64 = w.parse().ok()?;
    let height: f64 = h.parse().ok()?;
    if !is_js_integer(width) || !is_js_integer(height) || width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(ParsedSize { width, height })
}

/// JS `Number.isInteger`.
fn is_js_integer(x: f64) -> bool {
    x.is_finite() && x.fract() == 0.0
}

/// The verdict on an arbitrary `WIDTHxHEIGHT` (v4 `checkArbitrarySize`), which
/// names the rule it breaks so the caller can log something a user can act on.
#[derive(Clone, Debug, PartialEq)]
pub enum ArbitrarySizeCheck {
    Ok,
    Rejected(String),
}

/// Check an arbitrary `WIDTHxHEIGHT` against [`arbitrary_size_rules`] (v4
/// `checkArbitrarySize`). The five reason sentences are v4's template
/// interpolations, rendered.
pub fn check_arbitrary_size(size: &str) -> ArbitrarySizeCheck {
    use arbitrary_size_rules::*;
    let Some(ParsedSize { width, height }) = parse_size(size) else {
        return ArbitrarySizeCheck::Rejected("not a WIDTHxHEIGHT value".to_string());
    };
    if width % EDGE_MULTIPLE != 0.0 || height % EDGE_MULTIPLE != 0.0 {
        return ArbitrarySizeCheck::Rejected(format!(
            "both edges must be divisible by {}",
            js_num(EDGE_MULTIPLE)
        ));
    }
    if width.max(height) / width.min(height) > MAX_ASPECT {
        return ArbitrarySizeCheck::Rejected(format!(
            "aspect ratio must be between 1:{} and {}:1",
            js_num(MAX_ASPECT),
            js_num(MAX_ASPECT)
        ));
    }
    if width > MAX_EDGE || height > MAX_EDGE {
        return ArbitrarySizeCheck::Rejected(format!("no edge may exceed {}px", js_num(MAX_EDGE)));
    }
    if width * height > MAX_PIXELS {
        return ArbitrarySizeCheck::Rejected(format!(
            "total pixels may not exceed {}",
            js_num(MAX_PIXELS)
        ));
    }
    ArbitrarySizeCheck::Ok
}

/// A rule constant as a JS template literal renders it (`16`, not `16.0`).
pub(crate) fn js_num(x: f64) -> String {
    crate::pascal::js_value::number_to_string(x)
}

/// MIME type for a returned image, given the requested output format (v4
/// `mimeTypeForFormat`). Anything but `jpeg`/`webp` — including an absent
/// format — is `image/png`.
pub fn mime_type_for_format(format: Option<&str>) -> &'static str {
    match format {
        Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's `findImageModel` resolution rules, including the two overlaps its
    /// module header calls out by name.
    #[test]
    fn find_image_model_resolves_exact_then_longest_prefix() {
        assert_eq!(find_image_model(Some("dall-e-3")).unwrap().id, "dall-e-3");
        // A dated snapshot rides its family's entry.
        assert_eq!(
            find_image_model(Some("gpt-image-2.5-flare-2026-09-08"))
                .unwrap()
                .id,
            "gpt-image-2.5-flare"
        );
        // `gpt-image-2.5-sunburst` beats `gpt-image-2`…
        assert_eq!(
            find_image_model(Some("gpt-image-2.5-sunburst")).unwrap().id,
            "gpt-image-2.5-sunburst"
        );
        // …and `gpt-image-1-mini` beats `gpt-image-1`.
        assert_eq!(
            find_image_model(Some("gpt-image-1-mini-2026-01-01"))
                .unwrap()
                .id,
            "gpt-image-1-mini"
        );
        // A model the table does not know.
        assert!(find_image_model(Some("gpt-image-3-supernova")).is_none());
        // v4's falsy gate: absent AND empty are both "unknown".
        assert!(find_image_model(None).is_none());
        assert!(find_image_model(Some("")).is_none());
    }

    /// The bare-prefix fallback is what makes an unknown `gpt-image-*` still
    /// skip `response_format` and `style`.
    #[test]
    fn is_gpt_image_model_falls_back_to_the_id_prefix() {
        assert!(is_gpt_image_model(Some("gpt-image-1")));
        assert!(is_gpt_image_model(Some("gpt-image-3-supernova")));
        assert!(!is_gpt_image_model(Some("dall-e-3")));
        assert!(!is_gpt_image_model(Some("some-other-model")));
        assert!(!is_gpt_image_model(None));
    }

    /// The four failing shapes v4's `it.each` names, each with the rule it
    /// breaks — the reasons are the WARN's `reason` field.
    #[test]
    fn check_arbitrary_size_names_the_rule_it_breaks() {
        assert_eq!(check_arbitrary_size("1536x864"), ArbitrarySizeCheck::Ok);
        assert_eq!(check_arbitrary_size("3840x2160"), ArbitrarySizeCheck::Ok);
        assert_eq!(
            check_arbitrary_size("wide"),
            ArbitrarySizeCheck::Rejected("not a WIDTHxHEIGHT value".into())
        );
        assert_eq!(
            check_arbitrary_size("1000x1000"),
            ArbitrarySizeCheck::Rejected("both edges must be divisible by 16".into())
        );
        assert_eq!(
            check_arbitrary_size("3200x800"),
            ArbitrarySizeCheck::Rejected("aspect ratio must be between 1:3 and 3:1".into())
        );
        assert_eq!(
            check_arbitrary_size("3856x2160"),
            ArbitrarySizeCheck::Rejected("no edge may exceed 3840px".into())
        );
        assert_eq!(
            check_arbitrary_size("3840x3840"),
            ArbitrarySizeCheck::Rejected("total pixels may not exceed 8294400".into())
        );
    }

    /// The rules are checked in v4's ORDER, so a size breaking several is named
    /// by the first: `3856x3856` fails the aspect test never (1:1) but the edge
    /// test before the pixel test.
    #[test]
    fn check_arbitrary_size_reports_the_first_broken_rule() {
        assert_eq!(
            check_arbitrary_size("3856x3856"),
            ArbitrarySizeCheck::Rejected("no edge may exceed 3840px".into())
        );
        // Divisibility precedes the aspect test.
        assert_eq!(
            check_arbitrary_size("3201x800"),
            ArbitrarySizeCheck::Rejected("both edges must be divisible by 16".into())
        );
    }

    #[test]
    fn parse_size_follows_the_js_regex() {
        assert_eq!(
            parse_size(" 1024x768 "),
            Some(ParsedSize {
                width: 1024.0,
                height: 768.0
            })
        );
        assert_eq!(parse_size("1024X768"), None);
        assert_eq!(parse_size("1024x"), None);
        assert_eq!(parse_size("x768"), None);
        assert_eq!(parse_size("1024x768x2"), None);
        assert_eq!(parse_size("auto"), None);
        // Leading zeros are a digit run: `Number("0768")` is 768.
        assert_eq!(
            parse_size("01024x0768"),
            Some(ParsedSize {
                width: 1024.0,
                height: 768.0
            })
        );
        // Zero edges are refused by `width <= 0`.
        assert_eq!(parse_size("0x1024"), None);
    }

    #[test]
    fn mime_type_follows_the_requested_format() {
        assert_eq!(mime_type_for_format(Some("jpeg")), "image/jpeg");
        assert_eq!(mime_type_for_format(Some("webp")), "image/webp");
        assert_eq!(mime_type_for_format(Some("png")), "image/png");
        assert_eq!(mime_type_for_format(None), "image/png");
        assert_eq!(mime_type_for_format(Some("tiff")), "image/png");
    }

    /// The table's own shape: v4's order, and the two overlapping ids in the
    /// order that makes the longest-prefix rule resolve as its header says.
    #[test]
    fn table_is_v4s_order() {
        assert_eq!(
            openai_image_model_ids(),
            vec![
                "gpt-image-2.5-sunburst",
                "gpt-image-2.5-flare",
                "gpt-image-2",
                "gpt-image-1.5",
                "gpt-image-1-mini",
                "gpt-image-1",
                "dall-e-3",
                "dall-e-2",
            ]
        );
    }
}
