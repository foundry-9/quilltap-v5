//! Finding a character's name in text — v4 `lib/chat/name-mentions.ts`
//! (`9a88f862f`). The one matcher behind the referent ledger
//! ([`crate::services::referent_ledger`]) and the Salon's name chips for absent
//! characters ([`crate::services::absent_mentions`]), so the model and the
//! operator agree on what counts as a mention.
//!
//! Case-sensitive and letter-bounded: an alias like "Doc" or "Ember" is a common
//! word in lower case, and "Vergilian" is not Vergil. Longer tokens win, so
//! "Mrs. Sebold" is preferred over "Sebold". Pure.
//!
//! The pattern is v4's exactly — `(?<![\p{L}\p{N}_])(?:…)(?![\p{L}\p{N}_])` with
//! flags `gu` — run through `regress`, the workspace's JS-flavoured engine
//! (lookbehind, `\p{…}`, UTF-16 indices). Every [`NameMatch::index`] is a UTF-16
//! code-unit offset, as v4's `RegExpExecArray.index` is.
//!
//! It replaces `mentioned_characters.rs` (`\b…\b`, `gi`), a DIFFERENT contract
//! whose only consumer — the Host's per-turn "spoken of" scan — v4 deleted.

use crate::jsstr::{escape_regexp, js_trim, utf16_len};

/// A global matcher for any of a name and its aliases (v4's `RegExp`).
pub struct NameMatcher(regress::Regex);

/// One hit: where (UTF-16 units) and the token exactly as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameMatch {
    pub index: usize,
    pub text: String,
}

/// v4 `buildNameMatcher(names)` — a matcher for any usable token, or `None` when
/// none is. Tokens are trimmed (JS `trim`), empty ones dropped, de-duplicated in
/// first-seen order (a `Set`), then STABLE-sorted longest first by UTF-16 length
/// (`b.length - a.length`) so the longer alternative wins at a shared start.
pub fn build_name_matcher<'a>(
    names: impl IntoIterator<Item = Option<&'a str>>,
) -> Option<NameMatcher> {
    let mut tokens: Vec<&str> = Vec::new();
    for token in names.into_iter().flatten() {
        let trimmed = js_trim(token);
        if !trimmed.is_empty() && !tokens.contains(&trimmed) {
            tokens.push(trimmed);
        }
    }
    if tokens.is_empty() {
        return None;
    }
    // `Vec::sort_by` is stable, as V8's `Array.prototype.sort` is.
    tokens.sort_by_key(|t| std::cmp::Reverse(utf16_len(t)));
    let alternation = tokens
        .iter()
        .map(|t| escape_regexp(t))
        .collect::<Vec<_>>()
        .join("|");
    let pattern = format!(r"(?<![\p{{L}}\p{{N}}_])(?:{alternation})(?![\p{{L}}\p{{N}}_])");
    // The escaped alternation is always a valid pattern; a refusal would be an
    // engine gap, and v4's `new RegExp` would have thrown — no matcher either way.
    regress::Regex::with_flags(&pattern, "gu")
        .ok()
        .map(NameMatcher)
}

fn hits(text: &str, matcher: &NameMatcher) -> impl Iterator<Item = NameMatch> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let found: Vec<NameMatch> = matcher
        .0
        .find_from_utf16(&units, 0)
        .map(|m| NameMatch {
            index: m.start(),
            // `u` mode never splits a surrogate pair, so the slice decodes whole.
            text: String::from_utf16_lossy(&units[m.range()]),
        })
        .collect();
    found.into_iter()
}

/// v4 `firstNameMatch(text, matcher)` — the first match, or `None`.
pub fn first_name_match(text: &str, matcher: &NameMatcher) -> Option<NameMatch> {
    hits(text, matcher).next()
}

/// v4 `lastNameMatch(text, matcher)` — the last match, or `None`.
pub fn last_name_match(text: &str, matcher: &NameMatcher) -> Option<NameMatch> {
    hits(text, matcher).last()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(names: &[&str]) -> NameMatcher {
        build_name_matcher(names.iter().map(|n| Some(*n))).unwrap()
    }

    #[test]
    fn case_sensitive_and_letter_bounded() {
        let v = m(&["Vergil"]);
        assert!(first_name_match("not scolding vergil", &v).is_none());
        assert!(first_name_match("the Vergilian age", &v).is_none());
        assert!(first_name_match("Bob_Vergil", &v).is_none());
        assert_eq!(
            first_name_match("not scolding Vergil.", &v),
            Some(NameMatch {
                index: 13,
                text: "Vergil".into()
            })
        );
    }

    #[test]
    fn unicode_names_match_and_indices_are_utf16() {
        assert!(first_name_match("I met José today", &m(&["José"])).is_some());
        // An astral character before the name shifts the UTF-16 index by two.
        let hit = first_name_match("😀 Ada", &m(&["Ada"])).unwrap();
        assert_eq!(hit.index, 3);
    }

    #[test]
    fn longest_token_wins_and_metacharacters_are_literal() {
        let hit = first_name_match("Mrs. Sebold arrived", &m(&["Sebold", "Mrs. Sebold"])).unwrap();
        assert_eq!(hit.text, "Mrs. Sebold");
        // `.` is literal: "Mrs! Sebold" is not "Mrs. Sebold".
        assert!(first_name_match("Mrs! Sebold", &m(&["Mrs. Sebold"])).is_none());
    }

    #[test]
    fn empty_tokens_make_no_matcher() {
        assert!(build_name_matcher([Some("  "), None, Some("")]).is_none());
    }

    #[test]
    fn first_and_last() {
        let a = m(&["Ada"]);
        assert_eq!(first_name_match("Ada and Ada", &a).unwrap().index, 0);
        assert_eq!(last_name_match("Ada and Ada", &a).unwrap().index, 8);
    }
}
