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

use crate::db::roleplay_templates::{
    DelimiterAddOns, RenderingPattern, StringOrPair, TemplateDelimiter,
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

/// v4 `composeClassName` — `[style, ...addOnClassesFor(addOns)].join(' ')`.
fn compose_class_name(delimiter: &TemplateDelimiter) -> String {
    let mut parts = vec![delimiter_style(delimiter).to_string()];
    parts.extend(add_on_classes_for(delimiter_add_ons(delimiter)));
    parts.join(" ")
}

/// v4 `buildWrapPattern` — an inline wrap regex for a prefix/suffix pair. `None`
/// when the pair is empty.
fn build_wrap_pattern(prefix: &str, suffix: &str) -> Option<BuiltPattern> {
    if prefix.is_empty() && suffix.is_empty() {
        return None;
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
        TemplateDelimiter::Wrap { .. } => {
            let (prefix, suffix) = delimiter_to_prefix_suffix(delimiter);
            build_wrap_pattern(&prefix, &suffix)
        }
    }
}

/// v4 `generateRenderingPatterns` — auto-generate the rendering patterns from
/// template delimiters and narration delimiters (the default when a template
/// carries no explicit `renderingPatterns`).
pub fn generate_rendering_patterns(
    delimiters: &[TemplateDelimiter],
    narration_delimiters: Option<&StringOrPair>,
) -> Vec<RenderingPattern> {
    let mut patterns: Vec<RenderingPattern> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

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
                class_name: compose_class_name(d),
                flags: pattern.flags.filter(|f| !f.is_empty()),
                scope: (pattern.scope == Scope::Line).then(|| "line".to_string()),
                hide_delimiters: delimiter_hide(d).then_some(true),
            });
        }
    }

    // Add narration delimiters if not already covered. Narration is always a
    // wrap-style (inline) rule.
    if let Some(nd) = narration_delimiters {
        let (nar_prefix, nar_suffix) = match nd {
            StringOrPair::Single(s) => (s.clone(), s.clone()),
            StringOrPair::Pair([open, close]) => (open.clone(), close.clone()),
        };
        let key = format!("wrap|{nar_prefix}|{nar_suffix}");
        if !seen.contains(&key) {
            if let Some(pattern) = build_wrap_pattern(&nar_prefix, &nar_suffix) {
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

    patterns
}
