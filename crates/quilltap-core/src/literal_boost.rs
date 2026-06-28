//! Port of v4's lib/embedding/literal-boost.ts — the literal-phrase boost
//! helpers for hybrid (literal + embedding) search. They lift items that
//! contain the trimmed query as a verbatim case-insensitive substring so a
//! literal match isn't outranked by a slightly-stronger pure-vector neighbour.

/// Minimum length (in characters, after trim+lowercase) for a query to qualify
/// for the literal pass. Below this, short tokens hit too many false positives.
/// Length is counted in UTF-16 code units to match JS `String.length`.
pub const LITERAL_BOOST_MIN_PHRASE_LENGTH: usize = 8;

/// Literal-hit boost fraction for the responding character's own vault.
pub const LITERAL_BOOST_CHARACTER: f64 = 0.5;
/// Literal-hit boost fraction for a responding-character group store.
pub const LITERAL_BOOST_GROUP: f64 = 0.45;
/// Literal-hit boost fraction for a chat-project-linked mount.
pub const LITERAL_BOOST_PROJECT: f64 = 0.4;
/// Literal-hit boost fraction for the Quilltap General mount.
pub const LITERAL_BOOST_GLOBAL: f64 = 0.25;

/// Returns the trimmed lowercase phrase when the query qualifies for the
/// literal-boost pass, or `None`. Centralises the trim/lower/length gate so
/// every call site behaves identically. A `None` or empty query is falsy in v4
/// and yields `None`. The length check counts UTF-16 code units, matching JS
/// `phrase.length`.
pub fn get_literal_phrase(query: Option<&str>) -> Option<String> {
    let q = query?;
    if q.is_empty() {
        return None;
    }
    let phrase = q.trim().to_lowercase();
    if phrase.encode_utf16().count() >= LITERAL_BOOST_MIN_PHRASE_LENGTH {
        Some(phrase)
    } else {
        None
    }
}

/// Case-insensitive substring match against a candidate text field.
/// `lower_phrase` MUST already be lowercased ([`get_literal_phrase`] does that).
/// A `None` or empty text is falsy in v4 and yields `false`.
pub fn contains_literal_phrase(text: Option<&str>, lower_phrase: &str) -> bool {
    match text {
        Some(t) if !t.is_empty() => t.to_lowercase().contains(lower_phrase),
        _ => false,
    }
}

/// Fraction-of-distance-to-1 boost. With the default fraction 0.5: 0.0 → 0.5,
/// 0.5 → 0.75, 0.8 → 0.9. Applied to the cosine similarity of items that also
/// scored a literal-phrase hit.
pub fn apply_literal_boost(score: f64, fraction: f64) -> f64 {
    score + (1.0 - score) * fraction
}
