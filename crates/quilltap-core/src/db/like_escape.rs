//! LIKE-pattern escaping for user-supplied search text (v4
//! `lib/database/repositories/like-escape.ts`, ported whole at `b220999d`).
//!
//! SQLite's `LIKE` treats `%` and `_` as wildcards, so a raw user query of
//! `100%` or `a_b` would match far more than the user typed. Every substring
//! search built from user input goes through [`like_contains_pattern`], which
//! escapes those metacharacters (and the escape character itself) and wraps the
//! result in `%…%`. The matching SQL must declare the same escape character:
//! `... LIKE ? ESCAPE '\'`.
//!
//! The pattern is lower-cased **inside the helper** so callers can compare
//! against `LOWER(column)` — SQLite's built-in `LIKE` is only case-insensitive
//! for ASCII, and the mount-index path/name lookups already normalise with
//! `LOWER()`. (`str::to_lowercase` is byte-identical to JS `toLowerCase` — the
//! Phase-1 ICU/Unicode cluster settled that.)

/// The escape character declared by `ESCAPE '\'` in the accompanying SQL (v4
/// `LIKE_ESCAPE_CHAR`).
pub const LIKE_ESCAPE_CHAR: char = '\\';

/// Escape `%`, `_` and `\` in user text so LIKE matches it literally (v4
/// `escapeLikeLiteral` — `value.replace(/[\\%_]/g, ch => ESCAPE + ch)`).
pub fn escape_like_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == '\\' || ch == '%' || ch == '_' {
            out.push(LIKE_ESCAPE_CHAR);
        }
        out.push(ch);
    }
    out
}

/// Build a lower-cased `%contains%` LIKE pattern for a user-supplied query (v4
/// `likeContainsPattern`). Pair with `WHERE LOWER(col) LIKE ? ESCAPE '\'`.
pub fn like_contains_pattern(query: &str) -> String {
    format!("%{}%", escape_like_literal(&query.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // v4's five cases (`__tests__/unit/lib/database/repositories/
    // like-escape.test.ts`), ported one for one.

    #[test]
    fn leaves_ordinary_text_alone() {
        assert_eq!(escape_like_literal("manifesto"), "manifesto");
    }

    #[test]
    fn escapes_the_like_wildcards() {
        assert_eq!(escape_like_literal("100%"), "100\\%");
        assert_eq!(escape_like_literal("a_b"), "a\\_b");
    }

    #[test]
    fn escapes_the_escape_character_itself() {
        assert_eq!(escape_like_literal("c:\\notes"), "c:\\\\notes");
    }

    #[test]
    fn wraps_a_lowercased_escaped_needle() {
        assert_eq!(like_contains_pattern("Manifesto"), "%manifesto%");
    }

    #[test]
    fn keeps_user_wildcards_literal_inside_the_contains_pattern() {
        assert_eq!(like_contains_pattern("50%_off"), "%50\\%\\_off%");
    }

    /// The escape set is EXACTLY `\ % _` — nothing else is touched. A LIKE
    /// pattern that escaped, say, `[` would be a different query on a different
    /// engine; pinning the set keeps the port honest.
    #[test]
    fn escape_set_is_exactly_backslash_percent_underscore() {
        let punctuation = "!\"#$&'()*+,-./:;<=>?@[]^`{|}~";
        assert_eq!(escape_like_literal(punctuation), punctuation);
        assert_eq!(escape_like_literal("\\%_"), "\\\\\\%\\_");
    }
}
