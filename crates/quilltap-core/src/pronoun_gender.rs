//! Port of v4's lib/characters/pronoun-gender.ts — the `he → male` / `she →
//! female` image-prompt gender hint. Only the unambiguous binary subjects yield
//! a gender; everything else (they, neopronouns, empty, unset) is neutral.
//!
//! The only field read is the subject pronoun; callers pass `Some(subject)` when
//! pronouns are set, `None` when absent (v4's `if (!pronouns) return null`).

/// Map a subject pronoun to a binary gender (`male`/`female`), or `None` when
/// neutral, custom, empty, or unset. The subject is trimmed and lowercased.
pub fn gender_from_pronouns(subject: Option<&str>) -> Option<&'static str> {
    let subject = subject?;
    match subject.trim().to_lowercase().as_str() {
        "he" => Some("male"),
        "she" => Some("female"),
        _ => None,
    }
}

/// The gendered noun (`man`/`woman`) for a subject pronoun, or `None` when
/// neutral/unknown.
pub fn gender_noun_from_pronouns(subject: Option<&str>) -> Option<&'static str> {
    match gender_from_pronouns(subject) {
        Some("male") => Some("man"),
        Some("female") => Some("woman"),
        _ => None,
    }
}

/// A short sentence prefix (`"A man. "` / `"A woman. "`) for image prompts, or
/// the empty string when neutral/unknown.
pub fn gender_prefix_from_pronouns(subject: Option<&str>) -> String {
    match gender_noun_from_pronouns(subject) {
        Some(noun) => format!("A {noun}. "),
        None => String::new(),
    }
}
