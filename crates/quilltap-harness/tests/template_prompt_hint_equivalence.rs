//! Tier-1 differential test: formatting prompt-hint generator
//! (chat-orchestration wave 1.2, Part B).
//!
//! Covers generateFormattingPromptHint over every delimiter kind + the narration
//! default (incl. the restates-narration skip). Pure — exact string equality.
//!
//! P4.D351 (v4 `778b97b2b`): the row's `narration` is a THREE-state on the wire
//! (`JSON.stringify` omits an `undefined` and keeps a `null`), decoded through
//! `quilltap_core::db::serde_tristate::double_option`; a `wrap` row may carry
//! `matchCurlyQuotes` / `closeOptional`; a row may carry
//! `options: { markdownAsterisks? }`. Every mismatching row is named.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/template-prompt-hint.ts \
//!     > /tmp/oracle-template-prompt-hint.ndjson
//! Run:
//!   QT_ORACLE_TEMPLATE_PROMPT_HINT=/tmp/oracle-template-prompt-hint.ndjson \
//!     cargo test -p quilltap-harness --test template_prompt_hint_equivalence

use quilltap_core::db::serde_tristate::double_option;
use quilltap_core::template_prompt_hint::{
    generate_formatting_prompt_hint, HintOptions, NarrationDelimiters, NarrationHint,
    TemplateDelimiter, WrapDelimiters,
};
use serde::Deserialize;

/// A delimiter as emitted by the oracle. The runtime object carries extra fields
/// (buttonName, style) the generator never reads; `#[serde(default)]` ignores the
/// absent-vs-present distinction and unknown keys are dropped.
#[derive(Deserialize)]
#[serde(tag = "kind")]
enum WireDelimiter {
    #[serde(rename = "wrap")]
    Wrap {
        name: String,
        delimiters: WireWrap,
        #[serde(rename = "matchCurlyQuotes", default)]
        match_curly_quotes: Option<bool>,
        #[serde(rename = "closeOptional", default)]
        close_optional: Option<bool>,
    },
    #[serde(rename = "linePrefix")]
    LinePrefix { name: String, marker: String },
    #[serde(rename = "tagPrefix")]
    TagPrefix {
        name: String,
        open: String,
        close: String,
    },
}

/// A wrap delimiter's `delimiters`: JSON string (same) or a 2-tuple (pair).
#[derive(Deserialize)]
#[serde(untagged)]
enum WireWrap {
    Same(String),
    Pair([String; 2]),
}

/// Narration: JSON string (same) or a 2-tuple (pair).
#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum WireNarration {
    Same(String),
    Pair([String; 2]),
}

#[derive(Deserialize)]
struct WireOptions {
    #[serde(rename = "markdownAsterisks", default)]
    markdown_asterisks: Option<bool>,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    delimiters: Vec<WireDelimiter>,
    /// Absent → `None` (`undefined`), `null` → `Some(None)`, marks → `Some(Some)`.
    #[serde(default, deserialize_with = "double_option")]
    narration: Option<Option<WireNarration>>,
    #[serde(default)]
    options: Option<WireOptions>,
    out: String,
}

fn to_delimiter(w: WireDelimiter) -> TemplateDelimiter {
    match w {
        WireDelimiter::Wrap {
            name,
            delimiters,
            match_curly_quotes,
            close_optional,
        } => TemplateDelimiter::Wrap {
            name,
            delimiters: match delimiters {
                WireWrap::Same(s) => WrapDelimiters::Same(s),
                WireWrap::Pair([a, b]) => WrapDelimiters::Pair(a, b),
            },
            // v4 reads both as JS truthiness.
            match_curly_quotes: match_curly_quotes.unwrap_or(false),
            close_optional: close_optional.unwrap_or(false),
        },
        WireDelimiter::LinePrefix { name, marker } => {
            TemplateDelimiter::LinePrefix { name, marker }
        }
        WireDelimiter::TagPrefix { name, open, close } => {
            TemplateDelimiter::TagPrefix { name, open, close }
        }
    }
}

fn to_narration(w: WireNarration) -> NarrationDelimiters {
    match w {
        WireNarration::Same(s) => NarrationDelimiters::Same(s),
        WireNarration::Pair([a, b]) => NarrationDelimiters::Pair(a, b),
    }
}

#[test]
fn template_prompt_hint_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_TEMPLATE_PROMPT_HINT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TEMPLATE_PROMPT_HINT to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let delims: Vec<TemplateDelimiter> = row.delimiters.into_iter().map(to_delimiter).collect();
        let marks: Option<NarrationDelimiters> = row.narration.clone().flatten().map(to_narration);
        let narration = match (&row.narration, marks.as_ref()) {
            (None, _) => NarrationHint::Absent,
            (Some(_), Some(m)) => NarrationHint::Marks(m),
            (Some(_), None) => NarrationHint::Unmarked,
        };
        let options = HintOptions {
            markdown_asterisks: row.options.as_ref().and_then(|o| o.markdown_asterisks),
        };
        let got = generate_formatting_prompt_hint(&delims, narration, options);
        if got != row.out {
            failures.push(format!(
                "hint '{}'\n  v5: {got:?}\n  v4: {:?}",
                row.id, row.out
            ));
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    assert!(
        failures.is_empty(),
        "{} of {count} rows differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("OK: template-prompt-hint matched oracle ({count} rows).");
}
