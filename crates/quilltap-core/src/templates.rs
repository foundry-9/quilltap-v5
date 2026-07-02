//! Port of v4's lib/templates/processor.ts — SillyTavern-compatible template
//! variable replacement for character fields (`{{char}}`, `{{user}}`, etc.).
//!
//! Two-pass replacement, matching v4 byte-for-byte:
//!
//! 1. `{{(\w+)}}` — every `{{word}}` placeholder (JS `\w` = ASCII
//!    `[A-Za-z0-9_]` only) is replaced by its context value. A missing /
//!    `undefined` / `null` value becomes the empty string (SillyTavern's
//!    "missing variables are excluded" behavior). `{{/trim}}` survives this
//!    pass because `/` is not a `\w` character.
//! 2. `{{trim}}...{{/trim}}` — strips leading/trailing newlines from the
//!    wrapped content. Runs AFTER the variable pass, so `{{trim}}` (a bare
//!    `\w+` word) has itself already been consumed by pass 1 unless it was
//!    part of the paired macro — the paired form is matched here on the raw
//!    literal text that survived (both `{{trim}}` and `{{/trim}}` tokens are
//!    still present because pass 1 replaced `{{trim}}` with `context.trim`
//!    which is `undefined` → `''`... see the note below).
//!
//! IMPORTANT quirk reproduced: pass 1's regex `\{\{(\w+)\}\}` matches the
//! opening `{{trim}}` token (`trim` is `\w+`) and replaces it with
//! `context["trim"]` — which is `undefined` for a normal context → `''`. So by
//! the time pass 2 runs, the literal `{{trim}}` opener is GONE and the paired
//! `{{trim}}...{{/trim}}` regex can never match (the closing `{{/trim}}`
//! survives pass 1 but has no opener). This is exactly v4's observable
//! behavior: `{{trim}}` is emptied and `{{/trim}}` is left as a literal. The
//! port reproduces this precisely by running the same two passes in the same
//! order over the same regexes.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

/// Context values for template replacement. Mirrors v4's `TemplateContext`
/// interface: every field is an optional string. Fields not present resolve to
/// the empty string in `processTemplate` (v4's `undefined` → `''`).
///
/// Because the ported `processTemplate` looks values up by the placeholder's
/// literal name, this is modeled as a name→value map. `build_template_context`
/// produces the exact keys v4 sets (all as owned `String`s, never absent — v4
/// materializes every documented key).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TemplateContext {
    map: HashMap<String, String>,
}

impl TemplateContext {
    /// Look up a placeholder value. Returns `None` when the key is absent
    /// (v4's `context[key] === undefined` → the empty-string branch).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    /// Set a key (used by `build_template_context`).
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.map.insert(key.to_string(), value.into());
    }
}

// Pass-1: every {{word}} placeholder. JS `\w` is ASCII [A-Za-z0-9_]; the `regex`
// crate's `\w` is Unicode by default, so we spell out the ASCII class to match
// V8 exactly.
static VARIABLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{([A-Za-z0-9_]+)\}\}").expect("variable regex compiles"));

// Pass-2: the {{trim}}...{{/trim}} paired macro. `[\s\S]*?` is a lazy
// match-anything (JS `[\s\S]` = any char incl. newlines); the `regex` crate's
// `(?s)` dot-all flag makes `.` do the same, but we keep `[\s\S]` verbatim for
// fidelity to v4's source.
static TRIM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{trim\}\}([\s\S]*?)\{\{/trim\}\}").expect("trim regex compiles")
});

/// Strip leading/trailing runs of `\n` from `s`, reproducing JS
/// `content.replace(/^\n+|\n+$/g, '')`.
///
/// JS `^`/`$` without the `m` flag anchor at the whole-string start/end only,
/// and `g` replaces every match. So this removes a leading run of newlines and
/// a trailing run of newlines (both anchored), nothing interior. We implement
/// it directly rather than via the multiline `regex` engine to match JS's
/// non-multiline anchoring exactly.
fn strip_edge_newlines(s: &str) -> String {
    let start = s.bytes().take_while(|&b| b == b'\n').count();
    if start == s.len() {
        // All newlines: the leading match consumes everything.
        return String::new();
    }
    let trailing = s[start..].bytes().rev().take_while(|&b| b == b'\n').count();
    s[start..s.len() - trailing].to_string()
}

/// Process a template string by replacing `{{variable}}` placeholders with
/// their context values, then applying the `{{trim}}...{{/trim}}` macro.
///
/// Returns the empty string for a falsy (empty) `template`, matching v4's
/// `if (!template) return ''`.
pub fn process_template(template: &str, context: &TemplateContext) -> String {
    if template.is_empty() {
        return String::new();
    }

    // Pass 1: replace every {{word}} with its context value (or '' if absent).
    let result = VARIABLE_RE.replace_all(template, |caps: &regex::Captures| {
        let variable = &caps[1];
        match context.get(variable) {
            Some(value) => value.to_string(),
            None => String::new(),
        }
    });

    // Pass 2: {{trim}}...{{/trim}} — strip surrounding newlines from content.
    let result = TRIM_RE.replace_all(&result, |caps: &regex::Captures| {
        strip_edge_newlines(&caps[1])
    });

    result.into_owned()
}

/// A character as `build_template_context` / `process_character_templates`
/// read it. Optional string fields are `Option<String>`, matching v4's
/// `string | null | undefined` (a `None`/empty produces `''` via `|| ''`).
#[derive(Clone, Debug, Default)]
pub struct TemplateCharacter {
    pub name: String,
    pub description: Option<String>,
    pub manifesto: Option<String>,
    pub personality: Option<String>,
    pub first_message: Option<String>,
    pub example_dialogues: Option<String>,
}

/// A user character for the `{{user}}`/`{{persona}}` placeholders.
#[derive(Clone, Debug, Default)]
pub struct TemplateUserCharacter {
    pub name: String,
    pub description: Option<String>,
}

/// JS `value || ''` for an `Option<String>`: `None`/empty → `""`.
fn or_empty(v: &Option<String>) -> String {
    match v {
        Some(s) if !s.is_empty() => s.clone(),
        _ => String::new(),
    }
}

/// Build the template context from character and user-character data, exactly
/// as v4's `buildTemplateContext` does (every documented key materialized).
///
/// `scenario` and `system_prompt` are the caller-resolved overrides (`None` →
/// `''`). `user` defaults to `"User"` when no user character (v4:
/// `userCharacter?.name || 'User'`).
pub fn build_template_context(
    character: &TemplateCharacter,
    user_character: Option<&TemplateUserCharacter>,
    scenario: Option<&str>,
    system_prompt: Option<&str>,
) -> TemplateContext {
    let mut ctx = TemplateContext::default();

    // Character data.
    ctx.set("char", character.name.clone());
    ctx.set("description", or_empty(&character.description));
    ctx.set("manifesto", or_empty(&character.manifesto));
    ctx.set("personality", or_empty(&character.personality));
    ctx.set("scenario", scenario.filter(|s| !s.is_empty()).unwrap_or(""));

    // User character data. `userCharacter?.name || 'User'`: a missing user char
    // OR an empty name falls back to "User".
    let user = match user_character {
        Some(uc) if !uc.name.is_empty() => uc.name.clone(),
        _ => "User".to_string(),
    };
    ctx.set("user", user);
    let persona = user_character
        .and_then(|uc| uc.description.clone())
        .filter(|d| !d.is_empty())
        .unwrap_or_default();
    ctx.set("persona", persona);

    // System prompt (resolved by caller).
    ctx.set(
        "system",
        system_prompt.filter(|s| !s.is_empty()).unwrap_or(""),
    );

    // Example dialogues.
    ctx.set("mesExamplesRaw", or_empty(&character.example_dialogues));
    ctx.set("mesExamples", or_empty(&character.example_dialogues));

    // Future support — empty for now (v4 materializes these keys as '').
    ctx.set("wiBefore", "");
    ctx.set("wiAfter", "");
    ctx.set("loreBefore", "");
    ctx.set("loreAfter", "");
    ctx.set("anchorBefore", "");
    ctx.set("anchorAfter", "");

    ctx
}

/// The processed character fields returned by `process_character_templates`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessedCharacterTemplates {
    pub description: String,
    pub manifesto: String,
    pub personality: String,
    pub scenario: String,
    pub first_message: String,
    pub example_dialogues: String,
    pub system_prompt: String,
}

/// Process all character fields that may contain template variables, matching
/// v4's `processCharacterTemplates`. Builds the context once, then runs
/// `process_template` over each field (each with `|| ''` so a missing field
/// starts as the empty string).
pub fn process_character_templates(
    character: &TemplateCharacter,
    user_character: Option<&TemplateUserCharacter>,
    scenario: Option<&str>,
    system_prompt: Option<&str>,
) -> ProcessedCharacterTemplates {
    let context = build_template_context(character, user_character, scenario, system_prompt);

    let scenario_owned = scenario.filter(|s| !s.is_empty()).unwrap_or("");
    let system_owned = system_prompt.filter(|s| !s.is_empty()).unwrap_or("");

    ProcessedCharacterTemplates {
        description: process_template(&or_empty(&character.description), &context),
        manifesto: process_template(&or_empty(&character.manifesto), &context),
        personality: process_template(&or_empty(&character.personality), &context),
        scenario: process_template(scenario_owned, &context),
        first_message: process_template(&or_empty(&character.first_message), &context),
        example_dialogues: process_template(&or_empty(&character.example_dialogues), &context),
        system_prompt: process_template(system_owned, &context),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with(pairs: &[(&str, &str)]) -> TemplateContext {
        let mut c = TemplateContext::default();
        for (k, v) in pairs {
            c.set(k, *v);
        }
        c
    }

    #[test]
    fn empty_template_returns_empty() {
        assert_eq!(process_template("", &TemplateContext::default()), "");
    }

    #[test]
    fn simple_replacement() {
        let c = ctx_with(&[("char", "Ada"), ("user", "Bob")]);
        assert_eq!(
            process_template("{{char}} meets {{user}}", &c),
            "Ada meets Bob"
        );
    }

    #[test]
    fn missing_variable_becomes_empty() {
        let c = ctx_with(&[("char", "Ada")]);
        assert_eq!(process_template("[{{unknown}}]", &c), "[]");
    }

    #[test]
    fn non_word_placeholder_untouched() {
        // {{/trim}} is not \w+, so pass 1 leaves it; with no opener, pass 2 too.
        let c = TemplateContext::default();
        assert_eq!(process_template("a {{/trim}} b", &c), "a {{/trim}} b");
    }

    #[test]
    fn trim_opener_is_emptied_by_pass1() {
        // {{trim}} is \w+ → replaced by context["trim"] (absent → ''), so the
        // paired macro can never fire; the closer survives as a literal.
        let c = TemplateContext::default();
        assert_eq!(
            process_template("{{trim}}\nx\n{{/trim}}", &c),
            "\nx\n{{/trim}}"
        );
    }

    #[test]
    fn trim_macro_when_trim_key_present() {
        // If context has a `trim` key, pass 1 replaces {{trim}} with its value
        // (still not the macro). The paired macro only survives if {{trim}}
        // literal survives pass 1 — which it never does. Confirm the value sub.
        let c = ctx_with(&[("trim", "T")]);
        assert_eq!(process_template("{{trim}}x{{/trim}}", &c), "Tx{{/trim}}");
    }

    #[test]
    fn build_context_defaults_user() {
        let ch = TemplateCharacter {
            name: "Ada".into(),
            ..Default::default()
        };
        let ctx = build_template_context(&ch, None, None, None);
        assert_eq!(ctx.get("user"), Some("User"));
        assert_eq!(ctx.get("char"), Some("Ada"));
        assert_eq!(ctx.get("description"), Some(""));
    }
}
