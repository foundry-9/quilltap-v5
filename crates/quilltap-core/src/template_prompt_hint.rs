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
//!
//! v4 `778b97b2b` (the Novel template, P4.D351) grew three things: the narration
//! argument is a THREE-state ([`NarrationHint`] — absent, `null` = unmarked
//! prose, or marks), a `wrap` delimiter's `matchCurlyQuotes` / `closeOptional`
//! add capitalised notes to its bullet, and `markdownAsterisks === false` adds a
//! closing "never use double asterisks" bullet.

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
        /// v4 `matchCurlyQuotes` (read as JS truthiness — `false` is no note).
        match_curly_quotes: bool,
        /// v4 `closeOptional` (read as JS truthiness — `false` is no note).
        close_optional: bool,
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

/// The narration argument — v4's `narrationDelimiters?: NarrationDelimiters |
/// null`, whose three states the generator tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NarrationHint<'a> {
    /// `undefined` — no narration bullet (and nothing is skipped as restating it).
    Absent,
    /// `null` — narration is unmarked prose: the plain-prose bullet, and the
    /// narration prefix / suffix stay unset (so nothing is skipped).
    Unmarked,
    /// A string or `[open, close]` pair. v4 tests it for truthiness, so an empty
    /// string behaves as [`Self::Absent`].
    Marks(&'a NarrationDelimiters),
}

/// v4's `options: { markdownAsterisks?: boolean }`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HintOptions {
    /// Only `Some(false)` adds the double-asterisk bullet (v4 `=== false`).
    pub markdown_asterisks: Option<bool>,
}

/// v4's `n[0].toUpperCase() + n.slice(1)` — upper-case the first character. The
/// two notes are fixed ASCII-led strings, so the first UTF-16 unit and the first
/// `char` coincide.
fn capitalise_first(note: &str) -> String {
    let mut chars = note.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
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
    narration_delimiters: NarrationHint<'_>,
    options: HintOptions,
) -> String {
    let mut bullets: Vec<String> = Vec::new();

    // Narration first (it's the template-wide default action style). `null`
    // means narration is unmarked prose.
    let mut narration_prefix: Option<String> = None;
    let mut narration_suffix: Option<String> = None;
    match narration_delimiters {
        NarrationHint::Unmarked => bullets.push(
            "- Narration and action: plain prose with no marks around it (e.g. She stepped closer.). Never wrap narration in asterisks or brackets."
                .to_string(),
        ),
        // v4 `else if (narrationDelimiters)` — an empty string is falsy.
        NarrationHint::Marks(NarrationDelimiters::Same(s)) if s.is_empty() => {}
        NarrationHint::Marks(nd) => {
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
        NarrationHint::Absent => {}
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
            TemplateDelimiter::Wrap {
                name,
                match_curly_quotes,
                close_optional,
                ..
            } => {
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
                let mut notes: Vec<String> = Vec::new();
                if *match_curly_quotes {
                    notes.push("straight or curly quotation marks are both fine".to_string());
                }
                if *close_optional {
                    notes.push(format!(
                        "when it runs over several paragraphs, leave {suffix} off every paragraph but the last"
                    ));
                }
                // Each note is capitalised, then `; `-joined, `.`-terminated.
                let tail = if notes.is_empty() {
                    String::new()
                } else {
                    let capitalised: Vec<String> =
                        notes.iter().map(|n| capitalise_first(n)).collect();
                    format!(" {}.", capitalised.join("; "))
                };
                bullets.push(format!(
                    "- {name}: wrap in {prefix}…{suffix} (e.g. {prefix}example{suffix}).{tail}"
                ));
            }
        }
    }

    if options.markdown_asterisks == Some(false) {
        bullets.push(
            "- Never use double asterisks (**like this**); they have no meaning here and will not display as bold."
                .to_string(),
        );
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
        assert_eq!(
            generate_formatting_prompt_hint(&[], NarrationHint::Absent, HintOptions::default()),
            ""
        );
    }

    #[test]
    fn narration_only() {
        let star = NarrationDelimiters::Same("*".to_string());
        let out = generate_formatting_prompt_hint(
            &[],
            NarrationHint::Marks(&star),
            HintOptions::default(),
        );
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
                match_curly_quotes: false,
                close_optional: false,
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
                match_curly_quotes: false,
                close_optional: false,
            },
        ];
        let star = NarrationDelimiters::Same("*".to_string());
        let out = generate_formatting_prompt_hint(
            &delims,
            NarrationHint::Marks(&star),
            HintOptions::default(),
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
            match_curly_quotes: false,
            close_optional: false,
        }];
        let star = NarrationDelimiters::Same("*".to_string());
        let out = generate_formatting_prompt_hint(
            &delims,
            NarrationHint::Marks(&star),
            HintOptions::default(),
        );
        assert_eq!(
            out,
            "[FORMATTING GUIDE]\nPlease format your responses as follows:\n- Narration and action: wrap in *…* (e.g. *she stepped closer*)."
        );
    }

    #[test]
    fn unmarked_narration_with_both_notes_and_no_markdown_asterisks() {
        let delims = vec![TemplateDelimiter::Wrap {
            name: "Dialogue".to_string(),
            delimiters: WrapDelimiters::Same("\"".to_string()),
            match_curly_quotes: true,
            close_optional: true,
        }];
        let out = generate_formatting_prompt_hint(
            &delims,
            NarrationHint::Unmarked,
            HintOptions {
                markdown_asterisks: Some(false),
            },
        );
        let expected = [
            "[FORMATTING GUIDE]",
            "Please format your responses as follows:",
            "- Narration and action: plain prose with no marks around it (e.g. She stepped closer.). Never wrap narration in asterisks or brackets.",
            "- Dialogue: wrap in \"…\" (e.g. \"example\"). Straight or curly quotation marks are both fine; When it runs over several paragraphs, leave \" off every paragraph but the last.",
            "- Never use double asterisks (**like this**); they have no meaning here and will not display as bold.",
        ]
        .join("\n");
        assert_eq!(out, expected);
    }

    #[test]
    fn empty_string_narration_is_falsy() {
        let empty = NarrationDelimiters::Same(String::new());
        assert_eq!(
            generate_formatting_prompt_hint(
                &[],
                NarrationHint::Marks(&empty),
                HintOptions::default()
            ),
            ""
        );
    }
}
