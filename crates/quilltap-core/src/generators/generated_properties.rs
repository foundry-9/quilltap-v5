//! v4 `lib/characters/generated-properties.ts` — the LLM-generated character
//! properties (pronouns + aliases).
//!
//! "Summon From Lore (`lib/services/ai-import.service.ts`) and the AI Wizard
//! (`lib/services/character-wizard.service.ts`) both ask a model for the same
//! `{ pronouns, aliases }` object and narrate the same one-line result to the
//! progress stream. The parse and the narration live here so the two runners
//! cannot drift." (v4's header.)
//!
//! ## Ported edge behaviour
//!
//! * `parsed?.pronouns` is optional chaining over whatever the model answered:
//!   a top-level `null`, a number, a string or an array all yield `undefined`
//!   there, so the second arm (`sanitizePronouns(parsed)` — the pre-aliases
//!   bare-pronouns response shape) is what gets the chance, and it rejects them
//!   too. `Value::get` reproduces the chain exactly.
//! * `Array.isArray(parsed?.aliases)` — anything that is not an array is `[]`,
//!   including an object or a single string.
//! * Aliases keep only non-blank strings, TRIMMED, in order; the blank test is
//!   `a.trim().length > 0` on the untrimmed value, and the kept value is the
//!   trimmed one.
//! * The parse itself is [`crate::generators::llm_json::parse_llm_json`], so a
//!   non-JSON answer is an `Err` exactly where v4 throws.

use serde_json::Value;

use crate::generators::llm_json::{parse_llm_json, LlmJsonError};
use crate::generators::sanitize_pronouns::{sanitize_pronouns, Pronouns};
use crate::jsstr::js_trim;

/// v4 `GeneratedProperties`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GeneratedProperties {
    pub pronouns: Option<Pronouns>,
    pub aliases: Vec<String>,
}

/// v4 `parseGeneratedProperties` — "Parse a properties response. Pronouns are
/// sanitized (placeholders rejected) and a model that answers with the bare
/// pronouns object — the pre-aliases response shape — is tolerated. Aliases keep
/// only non-blank strings, trimmed. Throws when the response is not JSON at all,
/// like every other LLM parse."
pub fn parse_generated_properties(raw: &str) -> Result<GeneratedProperties, LlmJsonError> {
    let parsed = parse_llm_json(raw)?;
    let pronouns = parsed
        .get("pronouns")
        .and_then(sanitize_pronouns)
        .or_else(|| sanitize_pronouns(&parsed));
    let aliases = match parsed.get("aliases") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|a| a.as_str())
            .filter(|a| !js_trim(a).is_empty())
            .map(|a| js_trim(a).to_string())
            .collect(),
        _ => Vec::new(),
    };
    Ok(GeneratedProperties { pronouns, aliases })
}

/// v4 `describeGeneratedProperties` — "One-line progress snippet:
/// `she/her/hers; aliases: Em, The Botanist`. `missingPronounsLabel` stands in
/// for the pronoun triple when none was derivable — each runner has its own
/// wording for that."
pub fn describe_generated_properties(
    props: &GeneratedProperties,
    missing_pronouns_label: &str,
) -> String {
    let pronoun_text = match &props.pronouns {
        Some(p) => format!("{}/{}/{}", p.subject, p.object, p.possessive),
        None => missing_pronouns_label.to_string(),
    };
    let alias_text = if !props.aliases.is_empty() {
        format!("aliases: {}", props.aliases.join(", "))
    } else {
        "no aliases".to_string()
    };
    format!("{pronoun_text}; {alias_text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_non_json_answer_is_an_error_where_v4_throws() {
        assert!(parse_generated_properties("I cannot answer that.").is_err());
    }

    #[test]
    fn the_bare_pronouns_shape_is_tolerated_by_the_second_arm() {
        let p = parse_generated_properties(
            r#"{"subject":"they","object":"them","possessive":"theirs"}"#,
        )
        .unwrap();
        assert_eq!(
            p.pronouns,
            Some(Pronouns {
                subject: "they".into(),
                object: "them".into(),
                possessive: "theirs".into(),
            })
        );
        assert!(p.aliases.is_empty());
    }

    #[test]
    fn a_non_array_aliases_value_is_empty() {
        let p = parse_generated_properties(r#"{"aliases":"Em"}"#).unwrap();
        assert!(p.aliases.is_empty());
        let p = parse_generated_properties(r#"{"aliases":{"0":"Em"}}"#).unwrap();
        assert!(p.aliases.is_empty());
    }

    #[test]
    fn the_narration_uses_the_callers_missing_label() {
        let props = GeneratedProperties {
            pronouns: None,
            aliases: vec![],
        };
        assert_eq!(
            describe_generated_properties(&props, "no pronouns found"),
            "no pronouns found; no aliases"
        );
    }
}
