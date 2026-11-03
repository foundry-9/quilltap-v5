//! v4 `lib/characters/pronoun-presets.ts` (`eb90d9ed8`): the built-in pronoun
//! sets — the one list behind Aurora's pronoun picker and the values a wardrobe
//! item's pronoun guidance may name.
//!
//! Each preset is keyed by its subject pronoun (`he`, `she`, `they`, `it`).
//! Custom pronouns live outside this list: a character who uses them has no
//! standard subject, and so is never filtered by wardrobe pronoun guidance.

use crate::jsstr::js_trim;

/// One standard pronoun set and its picker label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StandardPronounPreset {
    pub label: &'static str,
    pub subject: &'static str,
    pub object: &'static str,
    pub possessive: &'static str,
}

/// v4 `STANDARD_PRONOUNS` — the standard pronoun sets, in picker order. Append
/// new sets at the end.
pub const STANDARD_PRONOUNS: [StandardPronounPreset; 4] = [
    StandardPronounPreset {
        label: "He/Him/His",
        subject: "he",
        object: "him",
        possessive: "his",
    },
    StandardPronounPreset {
        label: "She/Her/Her",
        subject: "she",
        object: "her",
        possessive: "her",
    },
    StandardPronounPreset {
        label: "They/Them/Their",
        subject: "they",
        object: "them",
        possessive: "their",
    },
    StandardPronounPreset {
        label: "It/It/Its",
        subject: "it",
        object: "it",
        possessive: "its",
    },
];

/// v4 `STANDARD_PRONOUN_SUBJECTS` — every standard subject, in picker order.
pub const STANDARD_PRONOUN_SUBJECTS: [&str; 4] = ["he", "she", "they", "it"];

/// v4 `standardPronounLabel`: the picker label for a standard subject
/// (`"he"` → `"He/Him/His"`), or the subject itself when it is not one.
pub fn standard_pronoun_label(subject: &str) -> &str {
    STANDARD_PRONOUNS
        .iter()
        .find(|p| p.subject == subject)
        .map(|p| p.label)
        .unwrap_or(subject)
}

/// v4 `standardPronounSubjectOf`: the standard set a character's pronouns are,
/// or `None` when they are unset or custom. Matches on the WHOLE set (each field
/// trimmed and lower-cased), exactly as the Aurora picker does, so a character
/// the picker shows as "Custom" is never treated as standard.
///
/// `pronouns` is `(subject, object, possessive)`.
pub fn standard_pronoun_subject_of(pronouns: Option<(&str, &str, &str)>) -> Option<&'static str> {
    let (subject, object, possessive) = pronouns?;
    let subject = js_trim(subject).to_lowercase();
    let object = js_trim(object).to_lowercase();
    let possessive = js_trim(possessive).to_lowercase();
    STANDARD_PRONOUNS
        .iter()
        .find(|p| p.subject == subject && p.object == object && p.possessive == possessive)
        .map(|p| p.subject)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_are_the_presets_in_picker_order() {
        let derived: Vec<&str> = STANDARD_PRONOUNS.iter().map(|p| p.subject).collect();
        assert_eq!(derived, STANDARD_PRONOUN_SUBJECTS);
    }

    #[test]
    fn a_partial_match_is_custom() {
        assert_eq!(
            standard_pronoun_subject_of(Some((" He ", "HIM", "his"))),
            Some("he")
        );
        assert_eq!(
            standard_pronoun_subject_of(Some(("he", "her", "his"))),
            None
        );
        assert_eq!(standard_pronoun_subject_of(None), None);
        assert_eq!(standard_pronoun_label("xe"), "xe");
    }
}
