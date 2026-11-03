//! Rendering-pattern auto-generation (P4.6p) — the pure port of v4's
//! `lib/chat/annotations.ts` `generateRenderingPatterns` (+ its private helpers
//! `delimiterToPrefixSuffix`, `addOnClassesFor`, `composeClassName`,
//! `buildDelimiterPattern`, `buildWrapPattern`).
//!
//! Given a template's `delimiters` (the discriminated-union entries) and its
//! `narrationDelimiters`, it derives the `RenderingPattern[]` a template uses when
//! it carries no explicit `renderingPatterns`. The roleplay-templates routes call
//! it on create/update (auto-regen when patterns are absent/empty) and at
//! read-time (GET-\[id\] materializes empty patterns on the fly, non-persisted).
//!
//! This is **pure string generation** — it only emits regex *source* strings
//! (never compiles a regex), so it is a tier-1 EXACT differential against v4.
//!
//! v4 `778b97b2b` (the Novel template, P4.D351) grew the generator a third
//! argument, [`TemplateRenderingOptions`]: `markdownAsterisks === false`
//! PREPENDS the `strip` / `literal` asterisk pre-parse pair, and an
//! `unmarkedStyle` with a non-empty `style` APPENDS the `unmarked` pattern; a
//! `null` narration (unmarked prose) emits no narration pattern; and a `wrap`
//! delimiter's `matchCurlyQuotes` / `closeOptional` take a new first arm in
//! `buildWrapPattern` (quote-tolerant open / close, an optional close, no
//! same-delimiter guard). [`generate_template_rendering_patterns`] is v4's
//! `generateTemplateRenderingPatterns` — the one call the routes and the
//! built-in table use, so a template's patterns are always derived the same way
//! from the same four fields.

use crate::db::roleplay_templates::{
    DelimiterAddOns, RenderingPattern, StringOrPair, TemplateDelimiter, UnmarkedStyle,
};
use std::collections::HashSet;

/// v4 `DEFAULT_TAG_TOKEN_PATTERN` (`lib/schemas/template.types.ts:26`).
const DEFAULT_TAG_TOKEN_PATTERN: &str = "[^\\p{Ll}]+";

/// Port of v4 `escapeRegex` (`lib/utils/regex.ts`):
/// `input.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')`.
fn escape_regex(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(
            ch,
            '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// The render scope of a built pattern (v4 `'inline' | 'line'`).
#[derive(PartialEq, Eq)]
enum Scope {
    Inline,
    Line,
}

/// A compiled-pattern descriptor: regex source, optional flags, and render scope
/// (v4's `BuiltPattern` interface).
struct BuiltPattern {
    regex: String,
    flags: Option<String>,
    scope: Scope,
}

/// v4 `delimiterToPrefixSuffix` — the prefix/suffix pair for a delimiter.
fn delimiter_to_prefix_suffix(delimiter: &TemplateDelimiter) -> (String, String) {
    match delimiter {
        TemplateDelimiter::LinePrefix { marker, .. } => (marker.clone(), String::new()),
        TemplateDelimiter::TagPrefix { open, close, .. } => (open.clone(), close.clone()),
        TemplateDelimiter::Wrap { delimiters, .. } => match delimiters {
            StringOrPair::Single(s) => (s.clone(), s.clone()),
            StringOrPair::Pair([open, close]) => (open.clone(), close.clone()),
        },
    }
}

/// v4 `addOnClassesFor` — map `DelimiterAddOns` to the `.qt-rp-*` utility classes.
fn add_on_classes_for(add_ons: Option<&DelimiterAddOns>) -> Vec<String> {
    let Some(a) = add_ons else {
        return Vec::new();
    };
    let mut classes: Vec<String> = Vec::new();
    if a.bold {
        classes.push("qt-rp-bold".to_string());
    }
    if a.italic {
        classes.push("qt-rp-italic".to_string());
    }
    if a.reverse {
        classes.push("qt-rp-reverse".to_string());
    }
    if a.underline == "single" {
        classes.push("qt-rp-underline".to_string());
    } else if a.underline == "double" {
        classes.push("qt-rp-underline-double".to_string());
    }
    if a.border == "solid" {
        classes.push("qt-rp-border".to_string());
    } else if a.border == "dashed" {
        classes.push("qt-rp-border-dashed".to_string());
    }
    if !a.font.is_empty() {
        classes.push(format!("qt-rp-font-{}", a.font));
    }
    classes
}

/// The shared fields on every delimiter variant: `style` + optional `addOns`.
fn delimiter_style(delimiter: &TemplateDelimiter) -> &str {
    match delimiter {
        TemplateDelimiter::Wrap { style, .. }
        | TemplateDelimiter::LinePrefix { style, .. }
        | TemplateDelimiter::TagPrefix { style, .. } => style,
    }
}

fn delimiter_add_ons(delimiter: &TemplateDelimiter) -> Option<&DelimiterAddOns> {
    match delimiter {
        TemplateDelimiter::Wrap { add_ons, .. }
        | TemplateDelimiter::LinePrefix { add_ons, .. }
        | TemplateDelimiter::TagPrefix { add_ons, .. } => add_ons.as_ref(),
    }
}

fn delimiter_hide(delimiter: &TemplateDelimiter) -> bool {
    match delimiter {
        TemplateDelimiter::Wrap { hide_delimiter, .. }
        | TemplateDelimiter::LinePrefix { hide_delimiter, .. }
        | TemplateDelimiter::TagPrefix { hide_delimiter, .. } => hide_delimiter.unwrap_or(false),
    }
}

/// The `kind` discriminant string (for the dedupe key `${kind}|${prefix}|${suffix}`).
fn delimiter_kind(delimiter: &TemplateDelimiter) -> &'static str {
    match delimiter {
        TemplateDelimiter::Wrap { .. } => "wrap",
        TemplateDelimiter::LinePrefix { .. } => "linePrefix",
        TemplateDelimiter::TagPrefix { .. } => "tagPrefix",
    }
}

/// v4 `composeClassName` — compose a delimiter's (or unmarked style's) base
/// class with its add-on classes: `[style, ...addOnClassesFor(addOns)].join(' ')`.
fn compose_class_name(style: &str, add_ons: Option<&DelimiterAddOns>) -> String {
    let mut parts = vec![style.to_string()];
    parts.extend(add_on_classes_for(add_ons));
    parts.join(" ")
}

/// v4 `TemplateRenderingOptions` — the template-wide rendering choices that sit
/// beside the delimiter list.
#[derive(Debug, Clone, Copy, Default)]
pub struct TemplateRenderingOptions<'a> {
    /// How text no delimiter claims is displayed (emits an `unmarked` pattern).
    pub unmarked_style: Option<&'a UnmarkedStyle>,
    /// `Some(false)` takes asterisks away from Markdown: doubled `**` runs are
    /// stripped and single `*` is made literal, so a `*` wrap delimiter can claim
    /// it. `None` / `Some(true)` ⇒ Markdown bold/italic as usual (v4 tests
    /// `=== false`).
    pub markdown_asterisks: Option<bool>,
}

/// v4 `LITERAL_ASTERISK_PATTERNS` — the pre-parse pair for a template with
/// `markdownAsterisks: false`. Order matters: the doubled runs are stripped
/// before the single ones are escaped.
fn literal_asterisk_patterns() -> [RenderingPattern; 2] {
    [
        RenderingPattern {
            pattern: "\\*{2,}".to_string(),
            class_name: String::new(),
            flags: None,
            scope: Some("strip".to_string()),
            hide_delimiters: None,
        },
        RenderingPattern {
            pattern: "\\*".to_string(),
            class_name: String::new(),
            flags: None,
            scope: Some("literal".to_string()),
            hide_delimiters: None,
        },
    ]
}

/// The first UTF-16 unit of `ch` — v4's `ch.charCodeAt(0)` (a non-BMP char
/// yields its HIGH surrogate, exactly as V8 does).
fn first_utf16_unit(ch: char) -> u16 {
    let mut buf = [0u16; 2];
    ch.encode_utf16(&mut buf)[0]
}

/// v4 `uEscape` — the regex-source `\uXXXX` escape (four lower-case hex digits
/// of `charCodeAt(0)`).
fn u_escape(ch: char) -> String {
    format!("\\u{:04x}", first_utf16_unit(ch))
}

/// v4 `CURLY_QUOTE_FORMS` — the curly forms a straight quote may also be written
/// as, by side (`(open, close)`).
fn curly_quote_forms(straight: char) -> (char, char) {
    if straight == '"' {
        ('\u{201c}', '\u{201d}')
    } else {
        ('\u{2018}', '\u{2019}')
    }
}

/// v4 `STRAIGHT_FORM_OF` — every quote glyph (straight or curly) mapped back to
/// its straight form.
fn straight_form_of(ch: char) -> Option<char> {
    match ch {
        '"' | '\u{201c}' | '\u{201d}' => Some('"'),
        '\'' | '\u{2018}' | '\u{2019}' => Some('\''),
        _ => None,
    }
}

/// Which side of a wrap a delimiter string sits on (v4 `'open' | 'close'`).
#[derive(Clone, Copy)]
enum Side {
    Open,
    Close,
}

/// v4 `quoteTolerant` — the alternatives a delimiter string matches when
/// `matchCurlyQuotes` is on: each quote glyph becomes a character class of its
/// straight form plus the curly form for this side; everything else is matched
/// literally. Also returns the raw characters involved, for a body exclusion
/// class. Iterates CODE POINTS (`for (const ch of delim)`).
fn quote_tolerant(delim: &str, side: Side) -> (String, Vec<char>) {
    let mut source = String::new();
    let mut chars: Vec<char> = Vec::new();
    for ch in delim.chars() {
        if let Some(straight) = straight_form_of(ch) {
            let (open, close) = curly_quote_forms(straight);
            let curly = match side {
                Side::Open => open,
                Side::Close => close,
            };
            source.push('[');
            source.push_str(&escape_regex(&straight.to_string()));
            source.push_str(&u_escape(curly));
            source.push(']');
            chars.push(straight);
            chars.push(curly);
        } else {
            source.push_str(&escape_regex(&ch.to_string()));
            chars.push(ch);
        }
    }
    (source, chars)
}

/// The two `wrap` flags `buildWrapPattern` reads (v4 passes them as
/// `{ matchCurlyQuotes, closeOptional }` — JS truthiness, so absent = `false`).
#[derive(Clone, Copy, Default)]
struct WrapOpts {
    match_curly_quotes: bool,
    close_optional: bool,
}

/// v4 `buildWrapPattern` — an inline wrap regex for a prefix/suffix pair. `None`
/// when the pair is empty.
///
/// `match_curly_quotes` lets straight quotes in the pair match their curly
/// forms; `close_optional` lets an unclosed span run to the end of the text run.
fn build_wrap_pattern(prefix: &str, suffix: &str, opts: WrapOpts) -> Option<BuiltPattern> {
    if prefix.is_empty() && suffix.is_empty() {
        return None;
    }

    if !prefix.is_empty() && !suffix.is_empty() && (opts.match_curly_quotes || opts.close_optional)
    {
        let (open_source, _) = if opts.match_curly_quotes {
            quote_tolerant(prefix, Side::Open)
        } else {
            (escape_regex(prefix), prefix.chars().collect())
        };
        let (close_source, close_chars) = if opts.match_curly_quotes {
            quote_tolerant(suffix, Side::Close)
        } else {
            (escape_regex(suffix), suffix.chars().collect())
        };
        // The body may not contain any closing character (mirrors the plain
        // paths). `[...new Set(close.chars)]` — de-duplicated, insertion order.
        let mut unique: Vec<char> = Vec::new();
        for ch in close_chars {
            if !unique.contains(&ch) {
                unique.push(ch);
            }
        }
        let exclude: String = unique
            .into_iter()
            .map(|ch| {
                if first_utf16_unit(ch) > 0x7f {
                    u_escape(ch)
                } else if ch == '-' {
                    "\\-".to_string()
                } else {
                    escape_regex(&ch.to_string())
                }
            })
            .collect();
        let closing = if opts.close_optional {
            format!("(?:{close_source}|$)")
        } else {
            close_source
        };
        let link_exclusion = if suffix == "]" && !opts.close_optional {
            "(?!\\()"
        } else {
            ""
        };
        return Some(BuiltPattern {
            regex: format!("{open_source}(?<rpBody>[^{exclude}]+){closing}{link_exclusion}"),
            flags: None,
            scope: Scope::Inline,
        });
    }

    // Defensive: a wrap with an empty suffix degrades to a line-start prefix.
    if !prefix.is_empty() && suffix.is_empty() {
        return Some(BuiltPattern {
            regex: format!("^{}(?<rpBody>.+)$", escape_regex(prefix)),
            flags: Some("m".to_string()),
            scope: Scope::Inline,
        });
    }

    let escaped_prefix = escape_regex(prefix);
    let escaped_suffix = escape_regex(suffix);

    if prefix == suffix {
        // Same open/close delimiter — avoid matching doubled delimiters.
        return Some(BuiltPattern {
            regex: format!(
                "(?<!{ep}){ep}(?<rpBody>[^{ep}]+){es}(?!{es})",
                ep = escaped_prefix,
                es = escaped_suffix
            ),
            flags: None,
            scope: Scope::Inline,
        });
    }

    // Different open/close — for square brackets, exclude markdown links.
    let link_exclusion = if suffix == "]" { "(?!\\()" } else { "" };
    Some(BuiltPattern {
        regex: format!(
            "{escaped_prefix}(?<rpBody>[^{escaped_suffix}]+){escaped_suffix}{link_exclusion}"
        ),
        flags: None,
        scope: Scope::Inline,
    })
}

/// v4 `buildDelimiterPattern` — dispatch by kind. `None` when the delimiter can't
/// form a meaningful pattern.
fn build_delimiter_pattern(delimiter: &TemplateDelimiter) -> Option<BuiltPattern> {
    match delimiter {
        TemplateDelimiter::LinePrefix { marker, .. } => {
            if marker.is_empty() {
                return None;
            }
            Some(BuiltPattern {
                regex: format!("^{}(?<rpBody>.+)$", escape_regex(marker)),
                flags: Some("m".to_string()),
                scope: Scope::Line,
            })
        }
        TemplateDelimiter::TagPrefix {
            open,
            close,
            token_pattern,
            ..
        } => {
            if open.is_empty() || close.is_empty() {
                return None;
            }
            let token = match token_pattern {
                Some(tp) if !tp.trim().is_empty() => tp.as_str(),
                _ => DEFAULT_TAG_TOKEN_PATTERN,
            };
            Some(BuiltPattern {
                regex: format!(
                    "^{}(?:{token}){}(?<rpBody>.*)$",
                    escape_regex(open),
                    escape_regex(close)
                ),
                flags: Some("mu".to_string()),
                scope: Scope::Line,
            })
        }
        TemplateDelimiter::Wrap {
            match_curly_quotes,
            close_optional,
            ..
        } => {
            let (prefix, suffix) = delimiter_to_prefix_suffix(delimiter);
            build_wrap_pattern(
                &prefix,
                &suffix,
                WrapOpts {
                    match_curly_quotes: match_curly_quotes.unwrap_or(false),
                    close_optional: close_optional.unwrap_or(false),
                },
            )
        }
    }
}

/// v4 `generateRenderingPatterns` — auto-generate the rendering patterns from
/// template delimiters and narration delimiters (the default when a template
/// carries no explicit `renderingPatterns`), plus the template-wide `strip` /
/// `literal` / `unmarked` patterns from [`TemplateRenderingOptions`].
///
/// `narration_delimiters` is `None` for BOTH v4's `undefined` and `null` (a
/// `null` narration — unmarked prose — generates no pattern); a
/// `Single("")` is falsy in v4 and generates none either.
pub fn generate_rendering_patterns(
    delimiters: &[TemplateDelimiter],
    narration_delimiters: Option<&StringOrPair>,
    options: TemplateRenderingOptions<'_>,
) -> Vec<RenderingPattern> {
    let mut patterns: Vec<RenderingPattern> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    if options.markdown_asterisks == Some(false) {
        patterns.extend(literal_asterisk_patterns());
    }

    for d in delimiters {
        let (prefix, suffix) = delimiter_to_prefix_suffix(d);
        // Include kind in the dedupe key: a `wrap` [ … ] and a `tagPrefix` [ … ]
        // are distinct rules that share the same prefix/suffix.
        let key = format!("{}|{}|{}", delimiter_kind(d), prefix, suffix);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);

        if let Some(pattern) = build_delimiter_pattern(d) {
            patterns.push(RenderingPattern {
                pattern: pattern.regex,
                class_name: compose_class_name(delimiter_style(d), delimiter_add_ons(d)),
                flags: pattern.flags.filter(|f| !f.is_empty()),
                scope: (pattern.scope == Scope::Line).then(|| "line".to_string()),
                hide_delimiters: delimiter_hide(d).then_some(true),
            });
        }
    }

    // Add narration delimiters if not already covered. Narration is always a
    // wrap-style (inline) rule.
    // v4 `if (narrationDelimiters)` — an empty string is falsy.
    let narration =
        narration_delimiters.filter(|nd| !matches!(nd, StringOrPair::Single(s) if s.is_empty()));
    if let Some(nd) = narration {
        let (nar_prefix, nar_suffix) = match nd {
            StringOrPair::Single(s) => (s.clone(), s.clone()),
            StringOrPair::Pair([open, close]) => (open.clone(), close.clone()),
        };
        let key = format!("wrap|{nar_prefix}|{nar_suffix}");
        if !seen.contains(&key) {
            if let Some(pattern) = build_wrap_pattern(&nar_prefix, &nar_suffix, WrapOpts::default())
            {
                patterns.push(RenderingPattern {
                    pattern: pattern.regex,
                    class_name: "qt-chat-narration".to_string(),
                    flags: pattern.flags.filter(|f| !f.is_empty()),
                    scope: None,
                    hide_delimiters: None,
                });
            }
        }
    }

    if let Some(unmarked) = options.unmarked_style {
        if !unmarked.style.is_empty() {
            // `pattern` is never matched for this scope; it only satisfies the
            // schema.
            patterns.push(RenderingPattern {
                pattern: "[\\s\\S]".to_string(),
                class_name: compose_class_name(&unmarked.style, unmarked.add_ons.as_ref()),
                flags: None,
                scope: Some("unmarked".to_string()),
                hide_delimiters: None,
            });
        }
    }

    patterns
}

/// v4 `generateTemplateRenderingPatterns` — [`generate_rendering_patterns`] over
/// a whole template's authoring fields (`template.delimiters ?? []`). The
/// `markdown_asterisks` tri-state passes through: only `Some(false)` prepends
/// the literal pair.
pub fn generate_template_rendering_patterns(
    delimiters: Option<&[TemplateDelimiter]>,
    narration_delimiters: Option<&StringOrPair>,
    unmarked_style: Option<&UnmarkedStyle>,
    markdown_asterisks: Option<bool>,
) -> Vec<RenderingPattern> {
    generate_rendering_patterns(
        delimiters.unwrap_or(&[]),
        narration_delimiters,
        TemplateRenderingOptions {
            unmarked_style,
            markdown_asterisks,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator reproduces the seed: the keystone's
    /// `services/builtin_templates.json` Novel row carries the patterns v4's own
    /// `generateTemplateRenderingPatterns` computed from Novel's authoring
    /// fields; generating them here from the same fields must be BYTE-equal
    /// (a `serde_json::Map` compare ignores key order — compare strings).
    #[test]
    fn generator_reproduces_the_seeded_novel_patterns() {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("builtin_templates.json")).unwrap();
        let novel = rows
            .iter()
            .find(|r| r["name"] == "Novel")
            .expect("the Novel built-in");
        let delimiters: Vec<TemplateDelimiter> =
            serde_json::from_value(novel["delimiters"].clone()).unwrap();
        let narration: Option<StringOrPair> =
            serde_json::from_value(novel["narrationDelimiters"].clone()).unwrap();
        let unmarked: Option<UnmarkedStyle> =
            serde_json::from_value(novel["unmarkedStyle"].clone()).unwrap();
        let markdown = novel["markdownAsterisks"].as_bool();
        assert!(narration.is_none(), "Novel's narration is unmarked");
        assert_eq!(markdown, Some(false));
        let got = generate_template_rendering_patterns(
            Some(&delimiters),
            narration.as_ref(),
            unmarked.as_ref(),
            markdown,
        );
        assert_eq!(
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&novel["renderingPatterns"]).unwrap()
        );
    }

    #[test]
    fn astral_close_escapes_its_high_surrogate() {
        let built = build_wrap_pattern(
            "\u{1f319}",
            "\u{1f31e}",
            WrapOpts {
                match_curly_quotes: false,
                close_optional: true,
            },
        )
        .unwrap();
        assert_eq!(
            built.regex,
            "\u{1f319}(?<rpBody>[^\\ud83c]+)(?:\u{1f31e}|$)"
        );
    }
}
