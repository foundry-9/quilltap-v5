//! Fitting a requested image shape onto what one NanoGPT model accepts — v4
//! `plugins/dist/qtap-plugin-nanogpt/image-sizes.ts` (`4b0d1cb04`, bug 269).
//!
//! v4's why, carried: the host asks for a shape in one provider-wide
//! vocabulary — a portrait avatar arrives as `size: '832x1248'` — but NanoGPT
//! fronts two hundred models with three different ideas of a size, and its
//! detailed catalog says which one each model speaks:
//!
//!   - concrete sizes, `WxH` (`1024x1536`) or `W*H` (`1024*1536`);
//!   - resolution tiers (`1k`, `2k`) with the shape carried separately in an
//!     `aspect_ratio` field (`2:3`), the Qwen Image 3 family among them.
//!
//! A size the model does not list is a hard 400 ("supports 1K and 2K
//! output"), so the request is fitted here, at the wire, from the model's own
//! catalog entry: the nearest listed concrete size by aspect ratio, or the
//! nearest tier by long edge plus the nearest listed aspect ratio. With no
//! catalog entry, or no shape to fit, the request goes out exactly as asked.
//!
//! Pure: no network, no logging — the provider fetches the catalog and
//! records what changed.
//!
//! **JS regex fidelity.** v4's three patterns are hand-rolled here rather
//! than compiled, because the `regex` crate's semantics differ on exactly the
//! inputs a catalog can carry: JS `\d` is ASCII-only, JS `\s` is the JS
//! whitespace set (U+FEFF yes, U+0085 no — [`crate::jsstr::is_js_ws`]), and a
//! JS `/i` without the `u` flag never folds a non-ASCII character onto an
//! ASCII one (so `1\u{212A}` — a KELVIN SIGN — is not the tier `1k`, which a
//! Unicode `(?i)` would accept). Each is measured both ways in the tier-1
//! family `nanogpt_image_sizes_equivalence`.

use serde::{Deserialize, Serialize};

use crate::jsstr::is_js_ws;

/// v4 `NanoGPTSizeCatalog` — the catalog fields this module reads from a
/// model's entry (`supported_parameters`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct NanoGptSizeCatalog {
    #[serde(default)]
    pub resolutions: Option<Vec<String>>,
    #[serde(default)]
    pub aspect_ratio: Option<Vec<String>>,
}

/// v4 `FittedImageShape`. Serialized in v4's `toEqual` shape: an `undefined`
/// key is ABSENT.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FittedImageShape {
    /// The `size` to post, in the model's own spelling; `None` to omit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// The `aspect_ratio` to post, or `None` to omit.
    #[serde(rename = "aspectRatio", skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    /// True when anything differs from what was asked.
    pub adjusted: bool,
}

/// v4 `DEFAULT_AREA` — the area a request with no pixel dimensions is
/// measured against.
const DEFAULT_AREA: f64 = 1024.0 * 1024.0;

/// A JS `Number(<digits>)` / `Number(<digits>.<digits>)`: the decimal string
/// is already validated, so Rust's correctly-rounded parse is JS's (a run past
/// the range is `Infinity` on both sides).
fn js_number(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// The length of the leading ASCII-digit run (JS `\d+`).
fn digit_run(s: &str) -> usize {
    s.bytes().take_while(u8::is_ascii_digit).count()
}

/// JS `\d+(?:\.\d+)?` at the start of `s` → the matched length, or `None`.
fn decimal_run(s: &str) -> Option<usize> {
    let int = digit_run(s);
    if int == 0 {
        return None;
    }
    let rest = &s[int..];
    if let Some(after_dot) = rest.strip_prefix('.') {
        let frac = digit_run(after_dot);
        if frac > 0 {
            return Some(int + 1 + frac);
        }
    }
    Some(int)
}

/// Strip JS `^\s*` … `\s*$`.
fn js_ws_trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

/// The `(a) \s* <sep> \s* (b)` core shared by the dims and ratio patterns,
/// on an already-trimmed string. `num` measures one operand.
fn split_pair(
    s: &str,
    num: impl Fn(&str) -> Option<usize>,
    is_sep: impl Fn(char) -> bool,
) -> Option<(&str, &str)> {
    let a_len = num(s)?;
    let (a, rest) = s.split_at(a_len);
    let rest = rest.trim_start_matches(is_js_ws);
    let sep = rest.chars().next().filter(|c| is_sep(*c))?;
    let rest = rest[sep.len_utf8()..].trim_start_matches(is_js_ws);
    let b_len = num(rest)?;
    // Anchored at `$` — after trimming the outer whitespace, nothing remains.
    (b_len == rest.len()).then(|| (a, &rest[..b_len]))
}

/// The captured digit groups of v4's dims pattern
/// `/^\s*(\d+)\s*[x×*]\s*(\d+)\s*$/i` — `[x×*]` under `/i` is `x`, `X`,
/// U+00D7 or `*` (the multiplication sign has no case partner).
fn dims_groups(value: &str) -> Option<(&str, &str)> {
    split_pair(
        js_ws_trim(value),
        |s| Some(digit_run(s)).filter(|n| *n > 0),
        |c| matches!(c, 'x' | 'X' | '\u{D7}' | '*'),
    )
}

/// v4 `parseDims` — `{width, height}` when both are `> 0`.
fn parse_dims(value: &str) -> Option<(f64, f64)> {
    let (a, b) = dims_groups(value)?;
    let (w, h) = (js_number(a), js_number(b));
    (w > 0.0 && h > 0.0).then_some((w, h))
}

/// v4 `parseRatio` — `/^\s*(\d+(?:\.\d+)?)\s*:\s*(\d+(?:\.\d+)?)\s*$/` →
/// `w / h` when both are `> 0`.
fn parse_ratio(value: &str) -> Option<f64> {
    let (a, b) = split_pair(js_ws_trim(value), decimal_run, |c| c == ':')?;
    let (w, h) = (js_number(a), js_number(b));
    (w > 0.0 && h > 0.0).then_some(w / h)
}

/// v4 `tierLongEdge` — `1k` → 1024, `1.5k` → 1536 (`/^\s*(\d+(?:\.\d+)?)\s*k\s*$/i`
/// — ASCII `k` / `K` only, see the module doc); anything else is not a tier
/// this can rank.
fn tier_long_edge(value: &str) -> Option<f64> {
    let s = js_ws_trim(value);
    let n = decimal_run(s)?;
    let rest = s[n..].trim_start_matches(is_js_ws);
    matches!(rest, "k" | "K").then(|| js_number(&s[..n]) * 1024.0)
}

/// v4 `ratioDistance` — distance between two ratios, symmetric for portrait
/// and landscape.
fn ratio_distance(a: f64, b: f64) -> f64 {
    (a.ln() - b.ln()).abs()
}

/// v4 `fitImageShape` — fit a requested `size` / `aspectRatio` onto a model's
/// advertised options. `size` wins over `aspectRatio` as the statement of
/// shape when both parse.
///
/// ⚠ The pass-through never carries the ASKED `aspectRatio` (v4's `asked` is
/// `{ size, aspectRatio: undefined, adjusted: false }`).
pub fn fit_image_shape(
    catalog: Option<&NanoGptSizeCatalog>,
    size: Option<&str>,
    aspect_ratio: Option<&str>,
) -> FittedImageShape {
    let asked = FittedImageShape {
        size: size.map(str::to_string),
        aspect_ratio: None,
        adjusted: false,
    };
    let resolutions: &[String] = catalog
        .and_then(|c| c.resolutions.as_deref())
        .unwrap_or(&[]);
    if resolutions.is_empty() {
        return asked;
    }

    // `size && …` / `aspectRatio ? …` are JS truthiness tests: an EMPTY string
    // is "not asked".
    let size = size.filter(|s| !s.is_empty());
    let aspect_ratio = aspect_ratio.filter(|s| !s.is_empty());

    // A size the model lists verbatim needs no fitting.
    if let Some(s) = size {
        if resolutions.iter().any(|r| r == s) {
            return asked;
        }
    }

    let dims = size.and_then(parse_dims);
    let target_ratio = match dims {
        Some((w, h)) => Some(w / h),
        None => aspect_ratio.and_then(parse_ratio),
    };
    let Some(target_ratio) = target_ratio else {
        return asked;
    };
    let target_area = dims.map_or(DEFAULT_AREA, |(w, h)| w * h);

    let concrete: Vec<(&String, (f64, f64))> = resolutions
        .iter()
        .filter_map(|v| parse_dims(v).map(|d| (v, d)))
        .collect();

    if let Some(first) = concrete.first() {
        // `reduce` with no seed: the first entry is the winner to beat; a
        // candidate replaces it only when STRICTLY nearer (ties keep the
        // earlier entry).
        let mut best = *first;
        for candidate in &concrete[1..] {
            let winner_ratio = ratio_distance(best.1 .0 / best.1 .1, target_ratio);
            let candidate_ratio = ratio_distance(candidate.1 .0 / candidate.1 .1, target_ratio);
            if candidate_ratio != winner_ratio {
                if candidate_ratio < winner_ratio {
                    best = *candidate;
                }
                continue;
            }
            let winner_area = (best.1 .0 * best.1 .1 - target_area).abs();
            let candidate_area = (candidate.1 .0 * candidate.1 .1 - target_area).abs();
            if candidate_area < winner_area {
                best = *candidate;
            }
        }
        return FittedImageShape {
            size: Some(best.0.clone()),
            aspect_ratio: None,
            // `best.value !== size` — an empty asked size compares as itself.
            adjusted: Some(best.0.as_str()) != asked.size.as_deref(),
        };
    }

    // Tier models: the size names a tier, the shape rides in `aspect_ratio`.
    let target_long_edge = dims.map_or(1024.0, |(w, h)| w.max(h));
    let tiers: Vec<(&String, f64)> = resolutions
        .iter()
        .filter_map(|v| tier_long_edge(v).map(|e| (v, e)))
        .collect();
    let Some(first_tier) = tiers.first() else {
        return asked;
    };
    let mut tier = *first_tier;
    for candidate in &tiers[1..] {
        if (candidate.1 - target_long_edge).abs() < (tier.1 - target_long_edge).abs() {
            tier = *candidate;
        }
    }

    let ratios: Vec<(&String, f64)> = catalog
        .and_then(|c| c.aspect_ratio.as_deref())
        .unwrap_or(&[])
        .iter()
        .filter_map(|v| parse_ratio(v).map(|r| (v, r)))
        .collect();
    let ratio = ratios.first().map(|first| {
        let mut winner = *first;
        for candidate in &ratios[1..] {
            if ratio_distance(candidate.1, target_ratio) < ratio_distance(winner.1, target_ratio) {
                winner = *candidate;
            }
        }
        winner.0.clone()
    });

    FittedImageShape {
        size: Some(tier.0.clone()),
        aspect_ratio: ratio,
        adjusted: true,
    }
}

/// v4 `isNonSquare(size, aspectRatio)` (`image-provider.ts:105-113`) — true
/// when a POSTED size or aspect ratio asks for something other than a square.
///
/// The ratio arm is v4's `aspectRatio.split(':').map(Number)` → both `> 0` →
/// a NUMERIC compare; the only `aspect_ratio` that can reach the body is a
/// catalog entry [`fit_image_shape`] accepted (no passthrough key spells it —
/// `NANOGPT_PASSTHROUGH_KEYS`), so `parse_ratio`'s operands are exactly what
/// `Number` reads. The size arm compares the captured digit GROUPS as STRINGS
/// (`dims[1] !== dims[2]`), so `0100x100` is non-square to v4 — ported as is.
pub fn is_non_square(size: Option<&str>, aspect_ratio: Option<&str>) -> bool {
    if let Some(ar) = aspect_ratio {
        if let Some((a, b)) = split_pair(js_ws_trim(ar), decimal_run, |c| c == ':') {
            let (w, h) = (js_number(a), js_number(b));
            if w > 0.0 && h > 0.0 {
                return w != h;
            }
        }
    }
    match size.filter(|s| !s.is_empty()).and_then(dims_groups) {
        Some((a, b)) => a != b,
        None => false,
    }
}

/// v4 `isShapeFailure(error)` (`image-provider.ts:119-125`) — whether a failed
/// generate call is one a square might get past: an upstream 503, or a 400
/// about the output's size, resolution or aspect ratio. A failure with no
/// status (a transport throw) is not one.
///
/// v4's `/\b(size|resolution|aspect|dimension|output)\b/i` is matched with
/// ASCII word boundaries and ASCII-only case folding (JS `\b` / `/i` without
/// the `u` flag) — `(?i-u)`.
pub fn is_shape_failure(status: Option<u16>, message: &str) -> bool {
    match status {
        Some(503) => true,
        Some(400) => shape_failure_regex().is_match(message.as_bytes()),
        _ => false,
    }
}

fn shape_failure_regex() -> &'static regex::bytes::Regex {
    static RE: std::sync::LazyLock<regex::bytes::Regex> = std::sync::LazyLock::new(|| {
        regex::bytes::Regex::new(r"(?i-u)\b(size|resolution|aspect|dimension|output)\b")
            .expect("static pattern")
    });
    &RE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qwen3() -> NanoGptSizeCatalog {
        NanoGptSizeCatalog {
            resolutions: Some(vec!["1k".into(), "2k".into()]),
            aspect_ratio: Some(
                ["1:1", "3:2", "16:9", "21:9", "9:16", "2:3", "3:4", "4:5"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
        }
    }

    #[test]
    fn v4s_first_vector_fits_a_portrait_onto_a_tier() {
        let fitted = fit_image_shape(Some(&qwen3()), Some("832x1248"), None);
        assert_eq!(
            fitted,
            FittedImageShape {
                size: Some("1k".into()),
                aspect_ratio: Some("2:3".into()),
                adjusted: true
            }
        );
    }

    #[test]
    fn is_non_square_compares_the_dims_groups_as_strings() {
        assert!(is_non_square(Some("0100x100"), None));
        assert!(!is_non_square(Some("1024x1024"), None));
        assert!(!is_non_square(Some("1k"), Some("1:1")));
        assert!(is_non_square(Some("1k"), Some("2:3")));
        assert!(!is_non_square(None, None));
    }

    #[test]
    fn is_shape_failure_reads_v4s_status_and_words() {
        assert!(is_shape_failure(Some(503), "anything"));
        assert!(is_shape_failure(
            Some(400),
            "400 Model X supports 1K and 2K output."
        ));
        assert!(!is_shape_failure(
            Some(400),
            "400 try a different prompt or image"
        ));
        assert!(!is_shape_failure(Some(400), "400 outputs"));
        // A LONG S is not an `s` to a non-unicode JS `/i`.
        assert!(!is_shape_failure(Some(400), "400 \u{17F}ize"));
        assert!(!is_shape_failure(None, "size"));
        assert!(!is_shape_failure(Some(500), "size"));
    }
}
