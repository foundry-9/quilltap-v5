//! v4 `lib/characters/sanitize-pronouns.ts` — pronoun sanitization for
//! LLM-generated character data.
//!
//! "Shared by Summon From Lore (`lib/services/ai-import.service.ts`) and the AI
//! Wizard's properties generation (`lib/services/character-wizard.service.ts`),
//! which both ask an LLM for a `{ subject, object, possessive }` pronouns object
//! and must reject placeholders before character create." (v4's header.)
//!
//! ## Ported edge behaviour (measured against v4's source, not guessed)
//!
//! * v4's guard is `!raw || typeof raw !== 'object'`. `typeof null` is
//!   `'object'` but `!null` is true, so `null` is rejected by the first half;
//!   an ARRAY passes the guard (`typeof [] === 'object'`) and then fails on the
//!   first field lookup, since `[]['subject']` is `undefined`. Reading the
//!   value as a serde_json OBJECT reproduces both outcomes exactly — an array
//!   has no `subject` either way — and [`tests`] pins the array arm.
//! * A field that is present but not a string is a hard reject (v4 returns
//!   `undefined` from inside the loop; it does not fall through to the next
//!   field).
//! * The length bound is `trimmed.length > 20` — JS `String.length`, i.e. UTF-16
//!   code UNITS, so a 20-astral-character pronoun is 40 units and rejects.
//!   [`crate::jsstr::utf16_len`] is that function.
//! * The placeholder set is matched on `trimmed.toLowerCase()`; v5's
//!   `str::to_lowercase` is byte-identical to JS's (the ICU cluster, Phase 1).

use serde_json::Value;

use crate::jsstr::{js_trim, utf16_len};

/// v4 `PRONOUN_PLACEHOLDERS` (byte-exact, in v4's order).
pub const PRONOUN_PLACEHOLDERS: &[&str] = &[
    "",
    "unknown",
    "n/a",
    "na",
    "none",
    "null",
    "undefined",
    "not specified",
    "not given",
    "tbd",
];

/// v4's three-field pronouns object.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Pronouns {
    pub subject: String,
    pub object: String,
    pub possessive: String,
}

/// v4 `sanitizePronouns` — "Validate a parsed pronouns response. Returns the
/// trimmed object if all three fields are usable strings; otherwise returns
/// undefined so the assembler stores null. PronounsSchema requires non-empty
/// strings <= 20 chars on subject/object/possessive, so anything else would
/// explode at character-create time."
pub fn sanitize_pronouns(raw: &Value) -> Option<Pronouns> {
    let obj = raw.as_object()?;
    let mut cleaned: Vec<String> = Vec::with_capacity(3);
    for field in ["subject", "object", "possessive"] {
        let v = obj.get(field)?;
        let s = v.as_str()?;
        let trimmed = js_trim(s);
        if trimmed.is_empty() || utf16_len(trimmed) > 20 {
            return None;
        }
        if PRONOUN_PLACEHOLDERS.contains(&trimmed.to_lowercase().as_str()) {
            return None;
        }
        cleaned.push(trimmed.to_string());
    }
    Some(Pronouns {
        subject: cleaned[0].clone(),
        object: cleaned[1].clone(),
        possessive: cleaned[2].clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_array_reaches_the_field_lookup_and_fails_there() {
        // v4: `typeof [] === 'object'` so the guard passes; `[]['subject']` is
        // undefined, so the first field rejects. Same answer either way — the
        // pin exists so a future "reject arrays up front" edit stays honest.
        assert_eq!(sanitize_pronouns(&json!([])), None);
        assert_eq!(sanitize_pronouns(&json!(["she", "her", "hers"])), None);
    }

    #[test]
    fn falsy_and_non_object_inputs_reject() {
        for v in [
            json!(null),
            json!(false),
            json!(0),
            json!(""),
            json!("she/her"),
        ] {
            assert_eq!(sanitize_pronouns(&v), None, "{v}");
        }
    }

    #[test]
    fn a_non_string_field_is_a_hard_reject() {
        assert_eq!(
            sanitize_pronouns(&json!({"subject": "she", "object": 3, "possessive": "hers"})),
            None
        );
    }

    #[test]
    fn the_length_bound_counts_utf16_units() {
        // 20 ASCII units: accepted.
        let twenty = "a".repeat(20);
        assert!(sanitize_pronouns(
            &json!({"subject": twenty, "object": "her", "possessive": "hers"})
        )
        .is_some());
        // 20 astral CHARACTERS are 40 UTF-16 units: rejected, as v4's
        // `String.length` rejects them.
        let astral = "\u{1f600}".repeat(20);
        assert!(sanitize_pronouns(
            &json!({"subject": astral, "object": "her", "possessive": "hers"})
        )
        .is_none());
    }

    #[test]
    fn placeholders_reject_case_insensitively_after_trimming() {
        for p in ["Unknown", "  N/A  ", "TBD", "not specified", "NULL"] {
            assert_eq!(
                sanitize_pronouns(&json!({"subject": p, "object": "her", "possessive": "hers"})),
                None,
                "{p}"
            );
        }
    }

    #[test]
    fn a_usable_triple_comes_back_trimmed() {
        assert_eq!(
            sanitize_pronouns(&json!({"subject": " she ", "object": "her", "possessive": "hers"})),
            Some(Pronouns {
                subject: "she".into(),
                object: "her".into(),
                possessive: "hers".into(),
            })
        );
    }
}
