//! v4 `lib/wardrobe/pronoun-guidance.ts` (`eb90d9ed8`): the one rule for "is
//! this item offered to this character?" and the one normaliser for the stored
//! value.
//!
//! An item may name the standard pronoun sets it is meant for
//! (`pronounGuidance`, values from [`crate::characters::pronoun_presets`]). A
//! pronoun-aware listing offers it only to characters whose pronouns are one of
//! those sets. Everything else is offered to everyone:
//!
//!  - an item with no guidance (empty or absent);
//!  - any item, to a character whose pronouns are unset or custom — there is no
//!    standard set to match against, so nothing is hidden from them.
//!
//! Guidance filters what is *offered*, never what may be *worn*: an item asked
//! for by id or title still goes on. Callers that list items for a character
//! (the `wardrobe_list` tool, the "Let character choose" pass) call
//! [`filter_by_pronoun_guidance`]; none re-derives the rule.

use std::collections::HashSet;

use serde_json::Value;

use crate::characters::pronoun_presets::{
    standard_pronoun_label, standard_pronoun_subject_of, STANDARD_PRONOUN_SUBJECTS,
};
use crate::jsstr::js_trim;

/// v4 `normalizePronounGuidance`: normalise a raw guidance value (frontmatter, a
/// tool argument, an imported bundle) to the stored shape — known subjects only,
/// lower-cased, de-duplicated, in PICKER order whatever the input order. A single
/// string is read as a one-item list; a full preset label (`He/Him/His`) is read
/// as its subject. Anything unreadable becomes `[]`.
pub fn normalize_pronoun_guidance(raw: Option<&Value>) -> Vec<String> {
    let values: Vec<&Value> = match raw {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(v @ Value::String(_)) => vec![v],
        _ => Vec::new(),
    };
    let mut found: HashSet<String> = HashSet::new();
    for value in values {
        let Some(s) = value.as_str() else { continue };
        let lowered = js_trim(s).to_lowercase();
        // `split('/')[0]` always exists for a string.
        let subject = js_trim(lowered.split('/').next().unwrap_or("")).to_string();
        if STANDARD_PRONOUN_SUBJECTS.contains(&subject.as_str()) {
            found.insert(subject);
        }
    }
    STANDARD_PRONOUN_SUBJECTS
        .iter()
        .filter(|s| found.contains(**s))
        .map(|s| s.to_string())
        .collect()
}

/// v4 `hasNoPronounGuidance`: true when the item names no pronoun guidance
/// (offered to everyone).
pub fn has_no_pronoun_guidance(guidance: &[String]) -> bool {
    guidance.is_empty()
}

/// v4 `fitsPronounGuidance`: is an item with `guidance` offered to a character
/// with `pronouns` (`(subject, object, possessive)`)? True for an item without
/// guidance, for a character without standard pronouns, and for an item whose
/// guidance names the character's set.
pub fn fits_pronoun_guidance(guidance: &[String], pronouns: Option<(&str, &str, &str)>) -> bool {
    if has_no_pronoun_guidance(guidance) {
        return true;
    }
    let Some(subject) = standard_pronoun_subject_of(pronouns) else {
        return true;
    };
    guidance.iter().any(|g| g == subject)
}

/// v4 `filterByPronounGuidance`: the items offered to a character with
/// `pronouns`, plus how many guidance held back. `keep_ids` are always kept
/// (what the character already wears, so a listing never hides the clothes on
/// their back). A character without standard pronouns gets every item and `0`,
/// with no per-item test.
pub fn filter_by_pronoun_guidance<'a, T>(
    items: &'a [T],
    pronouns: Option<(&str, &str, &str)>,
    keep_ids: Option<&HashSet<String>>,
    id_of: impl Fn(&T) -> &str,
    guidance_of: impl Fn(&T) -> &[String],
) -> (Vec<&'a T>, usize) {
    if standard_pronoun_subject_of(pronouns).is_none() {
        return (items.iter().collect(), 0);
    }
    let kept: Vec<&T> = items
        .iter()
        .filter(|item| {
            keep_ids.is_some_and(|k| k.contains(id_of(item)))
                || fits_pronoun_guidance(guidance_of(item), pronouns)
        })
        .collect();
    let hidden = items.len() - kept.len();
    (kept, hidden)
}

/// v4 `formatPronounGuidance`: a short human label for an item's guidance
/// (`He/Him/His, They/Them/Their`), or `""` for none.
pub fn format_pronoun_guidance(guidance: &[String]) -> String {
    let raw = Value::Array(guidance.iter().cloned().map(Value::String).collect());
    normalize_pronoun_guidance(Some(&raw))
        .iter()
        .map(|s| standard_pronoun_label(s).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn n(v: Value) -> Vec<String> {
        normalize_pronoun_guidance(Some(&v))
    }

    #[test]
    fn normalises_to_picker_order_and_drops_the_unknown() {
        assert_eq!(n(json!(["they", "she"])), ["she", "they"]);
        assert_eq!(n(json!(["She", "xe"])), ["she"]);
        assert_eq!(n(json!("He/Him/His")), ["he"]);
        assert_eq!(n(json!(["it", "he", "he"])), ["he", "it"]);
        assert!(n(json!([5, true])).is_empty());
        assert!(normalize_pronoun_guidance(None).is_empty());
    }

    #[test]
    fn random_values_never_panic_and_non_standard_input_is_empty() {
        let odd = [
            json!(null),
            json!(5),
            json!(1.5),
            json!(true),
            json!({}),
            json!({"he": true}),
            json!(""),
            json!("/"),
            json!("//he"),
            json!(" / "),
            json!("xe/xem"),
            json!([[], {}, null]),
            json!(["\u{feff}"]),
            json!(["İ", "ß"]),
        ];
        for v in &odd {
            assert!(n(v.clone()).is_empty(), "{v:?}");
        }
    }

    #[test]
    fn a_custom_character_sees_everything() {
        let items = vec![vec!["he".to_string()], vec![]];
        let (kept, hidden) = filter_by_pronoun_guidance(
            &items,
            Some(("ze", "zir", "zir")),
            None,
            |_| "",
            |g| g.as_slice(),
        );
        assert_eq!((kept.len(), hidden), (2, 0));
        assert_eq!(
            format_pronoun_guidance(&["they".into(), "he".into()]),
            "He/Him/His, They/Them/Their"
        );
    }
}
