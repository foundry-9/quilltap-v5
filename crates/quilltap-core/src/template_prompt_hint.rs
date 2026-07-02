//! Formatting prompt-hint generator — v4 `lib/chat/template-prompt-hint.ts`.
//!
//! Drafts a plain, kind-aware "how to format your replies" paragraph from a
//! template's delimiter list, for the template editor's "Draft formatting
//! instructions" button. The output is a STARTER the user edits — the template's
//! `systemPrompt` stays the source of truth sent to the model — so the text is
//! deliberately clear and literal (instructions to an LLM), not in the app's
//! usual flowery voice.
//!
//! Pure: no clock, no IO. Reproduces [`generate_formatting_prompt_hint`] over
//! the three delimiter kinds plus the template-wide narration default.

/// A template delimiter — v4's discriminated union `TemplateDelimiter` (the
/// three `kind` variants). Only the fields this generator reads are carried:
/// `name` (shown in each bullet) plus the kind-specific markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateDelimiter {
    /// `wrap` — `open…close` around an inline span. `delimiters` is a single
    /// string (same open/close) or an `[open, close]` tuple.
    Wrap {
        name: String,
        delimiters: WrapDelimiters,
    },
    /// `linePrefix` — a marker at the START of a line styles the whole line.
    LinePrefix { name: String, marker: String },
    /// `tagPrefix` — a bracketed token at the START of a line styles the whole line.
    TagPrefix {
        name: String,
        open: String,
        close: String,
    },
}

/// A `wrap` delimiter's open/close — either one shared string or an `[open, close]`
/// pair (v4's `z.union([z.string(), z.tuple([z.string(), z.string()])])`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapDelimiters {
    Same(String),
    Pair(String, String),
}

/// Narration delimiters — v4 `NarrationDelimiters`: a single string (same
/// open/close) or an `[open, close]` tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NarrationDelimiters {
    Same(String),
    Pair(String, String),
}

/// v4 `delimiterToPrefixSuffix`: a delimiter's prefix/suffix pair.
/// - `wrap` → its open/close (string ⇒ same both sides; tuple ⇒ [open, close]).
/// - `linePrefix` → `{ prefix: marker, suffix: '' }`.
/// - `tagPrefix` → `{ prefix: open, suffix: close }`.
fn delimiter_to_prefix_suffix(delimiter: &TemplateDelimiter) -> (String, String) {
    match delimiter {
        TemplateDelimiter::LinePrefix { marker, .. } => (marker.clone(), String::new()),
        TemplateDelimiter::TagPrefix { open, close, .. } => (open.clone(), close.clone()),
        TemplateDelimiter::Wrap { delimiters, .. } => match delimiters {
            WrapDelimiters::Same(s) => (s.clone(), s.clone()),
            WrapDelimiters::Pair(a, b) => (a.clone(), b.clone()),
        },
    }
}

/// Build a starter formatting-instructions paragraph from delimiters + narration
/// — v4 `generateFormattingPromptHint`. Returns `""` when there is nothing to
/// describe.
pub fn generate_formatting_prompt_hint(
    delimiters: &[TemplateDelimiter],
    narration_delimiters: Option<&NarrationDelimiters>,
) -> String {
    let mut bullets: Vec<String> = Vec::new();

    // Narration first (it's the template-wide default action style).
    let mut narration_prefix: Option<String> = None;
    let mut narration_suffix: Option<String> = None;
    if let Some(nd) = narration_delimiters {
        let (prefix, suffix) = match nd {
            NarrationDelimiters::Same(s) => (s.clone(), s.clone()),
            NarrationDelimiters::Pair(a, b) => (a.clone(), b.clone()),
        };
        bullets.push(format!(
            "- Narration and action: wrap in {prefix}…{suffix} (e.g. {prefix}she stepped closer{suffix})."
        ));
        narration_prefix = Some(prefix);
        narration_suffix = Some(suffix);
    }

    for d in delimiters {
        match d {
            TemplateDelimiter::LinePrefix { name, marker } => {
                bullets.push(format!(
                    "- {name}: begin the line with \"{marker}\" (e.g. {marker}an aside)."
                ));
            }
            TemplateDelimiter::TagPrefix {
                name, open, close, ..
            } => {
                bullets.push(format!(
                    "- {name}: begin the line with a {open}TOKEN{close} tag, where TOKEN is uppercase (e.g. {open}CAPTAIN{close} All hands on deck!). The whole line is styled."
                ));
            }
            TemplateDelimiter::Wrap { name, .. } => {
                let (prefix, suffix) = delimiter_to_prefix_suffix(d);
                // Skip a wrap delimiter that simply restates the narration default.
                // v4 compares against `narrationPrefix`/`narrationSuffix`, which are
                // `undefined` when there is no narration — a wrap with an empty
                // prefix AND empty suffix would then match (both `undefined`-vs-`""`?
                // no: `"" === undefined` is false), so with no narration nothing is
                // skipped. We model that: only skip when narration is present and
                // both sides equal it.
                if narration_prefix.as_deref() == Some(prefix.as_str())
                    && narration_suffix.as_deref() == Some(suffix.as_str())
                {
                    continue;
                }
                bullets.push(format!(
                    "- {name}: wrap in {prefix}…{suffix} (e.g. {prefix}example{suffix})."
                ));
            }
        }
    }

    if bullets.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        "[FORMATTING GUIDE]".to_string(),
        "Please format your responses as follows:".to_string(),
    ];
    lines.extend(bullets);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_empty() {
        assert_eq!(generate_formatting_prompt_hint(&[], None), "");
    }

    #[test]
    fn narration_only() {
        let out =
            generate_formatting_prompt_hint(&[], Some(&NarrationDelimiters::Same("*".to_string())));
        assert_eq!(
            out,
            "[FORMATTING GUIDE]\nPlease format your responses as follows:\n- Narration and action: wrap in *…* (e.g. *she stepped closer*)."
        );
    }

    #[test]
    fn all_kinds() {
        let delims = vec![
            TemplateDelimiter::Wrap {
                name: "Thoughts".to_string(),
                delimiters: WrapDelimiters::Same("_".to_string()),
            },
            TemplateDelimiter::LinePrefix {
                name: "OOC".to_string(),
                marker: "// ".to_string(),
            },
            TemplateDelimiter::TagPrefix {
                name: "Rank".to_string(),
                open: "[".to_string(),
                close: "]".to_string(),
            },
            TemplateDelimiter::Wrap {
                name: "Aside".to_string(),
                delimiters: WrapDelimiters::Pair("{".to_string(), "}".to_string()),
            },
        ];
        let out = generate_formatting_prompt_hint(
            &delims,
            Some(&NarrationDelimiters::Same("*".to_string())),
        );
        let expected = [
            "[FORMATTING GUIDE]",
            "Please format your responses as follows:",
            "- Narration and action: wrap in *…* (e.g. *she stepped closer*).",
            "- Thoughts: wrap in _…_ (e.g. _example_).",
            "- OOC: begin the line with \"// \" (e.g. // an aside).",
            "- Rank: begin the line with a [TOKEN] tag, where TOKEN is uppercase (e.g. [CAPTAIN] All hands on deck!). The whole line is styled.",
            "- Aside: wrap in {…} (e.g. {example}).",
        ]
        .join("\n");
        assert_eq!(out, expected);
    }

    #[test]
    fn wrap_matching_narration_skipped() {
        // A wrap that restates the narration default is dropped.
        let delims = vec![TemplateDelimiter::Wrap {
            name: "Redundant".to_string(),
            delimiters: WrapDelimiters::Same("*".to_string()),
        }];
        let out = generate_formatting_prompt_hint(
            &delims,
            Some(&NarrationDelimiters::Same("*".to_string())),
        );
        assert_eq!(
            out,
            "[FORMATTING GUIDE]\nPlease format your responses as follows:\n- Narration and action: wrap in *…* (e.g. *she stepped closer*)."
        );
    }
}
