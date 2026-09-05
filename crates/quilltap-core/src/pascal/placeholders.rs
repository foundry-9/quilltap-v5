//! The `{{placeholder}}` families — one classifier for every reader
//! (v4 `lib/pascal/placeholders.ts`, NEW at `0506517d3`).
//!
//! v4's own header rides verbatim, because it records why the module exists:
//!
//! > Seven sites used to re-derive "which family is this key" from a chain of
//! > `===` and `startsWith` tests: the template renderer, the effect-expression
//! > resolver, the grammar's reference check, the vocabulary scanner, and three
//! > Workbench audits. This module is that chain, once. Everything that reads a
//! > placeholder classifies it here and then decides what its own family means.
//! >
//! > CLIENT-SAFE and dependency-free: the Workbench runs it in the browser, and
//! > `expressions.ts` (which must stay import-light) leans on it from below the
//! > schema module.
//!
//! Three of v4's seven readers are v5's: [`super::expressions`]'s `is_known_ref`,
//! [`super::tool_vocabulary`]'s scanner, and [`super::custom_tools`]'s renderer
//! and effect resolver. The three Workbench draft audits live only in the SPA
//! (v5 has no Rust `tool-draft`), so their twin of this module is
//! `apps/web/src/app/pascal/placeholders.ts`.
//!
//! **The bare-prefix rule is the correction.** `{{params.}}` names nothing and
//! is `Unknown`, not a params reference with an empty name. In Rust that
//! distinction is easy to lose: `"params.".strip_prefix("params.")` is
//! `Some("")`, not `None`, so every site that stripped a prefix had to remember
//! to check the remainder — and the four that did agree with v4's new rule only
//! by coincidence of what they then did with an empty name. Deciding it once
//! here is what makes it a rule instead of a coincidence.

use std::sync::LazyLock;

use regex::Regex;

use crate::jsstr::js_trim;

/// v4 `PLACEHOLDER_PATTERN` — matches one `{{key}}` occurrence; group 1 is the
/// raw key, untrimmed.
///
/// v4 declares it `g`-flagged and its readers share `lastIndex` by hand;
/// [`scan_placeholders`] is the shape that removes that hazard, and a Rust
/// `captures_iter` is stateless anyway. Only the MATCHING semantics carry over:
/// `[^}]+`, so `{{}}` never matches and a `}` cannot appear inside a
/// placeholder.
pub static PLACEHOLDER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{([^}]+)\}\}").expect("placeholder pattern compiles"));

const PARAMS_PREFIX: &str = "params.";
const METADATA_PREFIX: &str = "metadata.";
const STATE_PREFIX: &str = "state.";

/// v4 `PlaceholderRef` — a classified placeholder key. `Unknown` keeps the key
/// for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceholderRef {
    Value,
    Roll,
    Dice,
    Llm,
    Params { name: String },
    Metadata { key: String },
    State { path: String },
    Unknown { key: String },
}

/// v4 `classifyPlaceholder(key)` — classify one ALREADY-TRIMMED placeholder key.
///
/// A family prefix with nothing after it (`params.`) names nothing and is
/// `Unknown`, exactly as the effect grammar has always ruled.
pub fn classify_placeholder(key: &str) -> PlaceholderRef {
    match key {
        "value" => return PlaceholderRef::Value,
        "roll" => return PlaceholderRef::Roll,
        "dice" => return PlaceholderRef::Dice,
        "llm" => return PlaceholderRef::Llm,
        _ => {}
    }
    if let Some(name) = key.strip_prefix(PARAMS_PREFIX) {
        if !name.is_empty() {
            return PlaceholderRef::Params {
                name: name.to_string(),
            };
        }
    }
    if let Some(k) = key.strip_prefix(METADATA_PREFIX) {
        if !k.is_empty() {
            return PlaceholderRef::Metadata { key: k.to_string() };
        }
    }
    if let Some(path) = key.strip_prefix(STATE_PREFIX) {
        if !path.is_empty() {
            return PlaceholderRef::State {
                path: path.to_string(),
            };
        }
    }
    PlaceholderRef::Unknown {
        key: key.to_string(),
    }
}

/// v4 `ScannedPlaceholder` — one placeholder as found in a string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedPlaceholder {
    /// The occurrence verbatim, braces included — what "renders as written" means.
    pub whole: String,
    /// The trimmed key. JS-trimmed: `String.prototype.trim` and Rust's `trim`
    /// disagree on U+FEFF and U+0085, and v4 trims here.
    pub key: String,
    pub place_ref: PlaceholderRef,
}

/// v4 `scanPlaceholders(text)` — every `{{…}}` in `text`, in order, classified.
pub fn scan_placeholders(text: &str) -> Vec<ScannedPlaceholder> {
    PLACEHOLDER_PATTERN
        .captures_iter(text)
        .map(|caps| {
            let whole = caps.get(0).map_or("", |m| m.as_str()).to_string();
            let key = js_trim(caps.get(1).map_or("", |m| m.as_str())).to_string();
            let place_ref = classify_placeholder(&key);
            ScannedPlaceholder {
                whole,
                key,
                place_ref,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_family_prefix_names_nothing() {
        // The correction, in both directions. Rust's `strip_prefix` answers
        // `Some("")` here, so without the guard these would classify as a
        // params/metadata/state reference with an empty name.
        for key in ["params.", "metadata.", "state."] {
            assert_eq!(
                classify_placeholder(key),
                PlaceholderRef::Unknown {
                    key: key.to_string()
                },
                "{key} names nothing"
            );
        }
        assert_eq!(
            classify_placeholder("params.bonus"),
            PlaceholderRef::Params {
                name: "bonus".into()
            }
        );
        assert_eq!(
            classify_placeholder("state.a.b[0]"),
            PlaceholderRef::State {
                path: "a.b[0]".into()
            }
        );
        assert_eq!(
            classify_placeholder("metadata.dots.are.fine"),
            PlaceholderRef::Metadata {
                key: "dots.are.fine".into()
            }
        );
    }

    #[test]
    fn the_four_bare_names_are_exact_not_prefixes() {
        assert_eq!(classify_placeholder("value"), PlaceholderRef::Value);
        assert_eq!(
            classify_placeholder("values"),
            PlaceholderRef::Unknown {
                key: "values".into()
            }
        );
        assert_eq!(
            classify_placeholder("params"),
            PlaceholderRef::Unknown {
                key: "params".into()
            }
        );
    }

    #[test]
    fn scanning_trims_js_whitespace_and_never_shares_state() {
        let found = scan_placeholders("a {{ value }} b {{\u{FEFF}params.x\u{FEFF}}} c {{}} d");
        assert_eq!(found.len(), 2, "`{{{{}}}}` never matches: {found:?}");
        assert_eq!(found[0].whole, "{{ value }}");
        assert_eq!(found[0].key, "value");
        assert_eq!(found[0].place_ref, PlaceholderRef::Value);
        // U+FEFF is JS whitespace and is NOT Rust `char::is_whitespace` — a
        // `.trim()` here would leave it in the key and classify `Unknown`.
        assert_eq!(found[1].key, "params.x");
        assert_eq!(
            found[1].place_ref,
            PlaceholderRef::Params { name: "x".into() }
        );

        // Two calls over different texts cannot influence each other (v4's
        // readers used to share one `g` regex's `lastIndex`).
        let a = scan_placeholders("{{value}}{{roll}}");
        let b = scan_placeholders("{{dice}}");
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].place_ref, PlaceholderRef::Dice);
    }
}
