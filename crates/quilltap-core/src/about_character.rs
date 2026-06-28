//! Port of v4's lib/memory/about-character-resolution.ts — the name-set builders
//! and the word-boundary name matchers that consume them.
//!
//! ## Reproducing `buildNameRegex` without lookahead
//!
//! v4 matches a name with the case-insensitive Unicode regex
//! `(^|[^\p{L}\p{N}_])NAME(?=$|[^\p{L}\p{N}_])` (flags `iu` / `giu`): a consumed
//! leading boundary char and a *lookahead* trailing boundary, with Unicode
//! letter/number word-class. Rust's `regex` crate has no lookahead, but the
//! behaviour is exactly reproducible: find every case-insensitive occurrence of
//! the literal name with `find_iter` (non-overlapping, advancing past each
//! match — the same advancement JS `String.match(/g)` performs), then accept an
//! occurrence iff the char immediately before it is start-or-non-word and the
//! char immediately after is end-or-non-word.
//!
//! The trailing boundary is a *lookahead* in v4 specifically so a single
//! delimiter between two adjacent occurrences can serve as the first match's
//! (non-consumed) trailing boundary AND the second match's (consumed) leading
//! boundary — both count. `find_iter` doesn't consume either boundary, so the
//! post-check counts both as well; the two agree occurrence-for-occurrence
//! (verified by the tier-1 corpus, which includes the adjacency cases).
//!
//! Word-class membership uses the `regex` crate's `[\p{L}\p{N}_]`, giving the
//! same Unicode `\p{L}`/`\p{N}` set as JS's `u` flag; case-insensitive literal
//! matching uses the engine's `(?i)` Unicode simple case folding, matching JS
//! `iu`. The literal name is escaped with `regex::escape` (a different escape
//! spelling than v4's `escapeRegex`, but the matched literal is identical).

use regex::Regex;

/// Generic aliases that cheap-LLM extraction prompts use for the human user.
/// Included whenever the about-character is `controlledBy: 'user'`.
pub const USER_GENERIC_ALIASES: [&str; 2] = ["user", "the user"];

/// Build the names + aliases used to decide whether a memory is about the given
/// character. For user-controlled characters, augments the set with the generic
/// "user"/"the user" aliases extraction prompts emit. Entries whose trimmed form
/// is empty are dropped; surviving entries keep their original (untrimmed) form.
pub fn names_for_about_character(
    name: &str,
    aliases: &[String],
    controlled_by: &str,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::with_capacity(aliases.len() + 3);
    names.push(name.to_string());
    names.extend(aliases.iter().cloned());
    if controlled_by == "user" {
        names.extend(USER_GENERIC_ALIASES.iter().map(|s| s.to_string()));
    }
    names.retain(|n| !n.trim().is_empty());
    names
}

/// Build the names used to decide whether a memory is *about the holder*.
/// Excludes the generic "user"/"the user" aliases — those match only the
/// user-controlled about-target, not the holder's identity. Symmetric with
/// [`names_for_about_character`] aside from that gap.
pub fn names_for_holder(name: &str, aliases: &[String]) -> Vec<String> {
    let mut names: Vec<String> = Vec::with_capacity(aliases.len() + 1);
    names.push(name.to_string());
    names.extend(aliases.iter().cloned());
    names.retain(|n| !n.trim().is_empty());
    names
}

/// True when `c` is in the Unicode word class `[\p{L}\p{N}_]` that v4's
/// boundary regex uses (letters, numbers, underscore).
fn is_word_char(word_class: &Regex, c: char) -> bool {
    let mut buf = [0u8; 4];
    word_class.is_match(c.encode_utf8(&mut buf))
}

/// Does the literal `name` occur at `[start, end)` with non-word (or
/// string-edge) characters on both sides? Mirrors the leading-group +
/// trailing-lookahead boundary test of v4's `buildNameRegex`.
fn occurrence_is_bounded(word_class: &Regex, haystack: &str, start: usize, end: usize) -> bool {
    let before_ok = start == 0
        || haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(word_class, c));
    let after_ok = end == haystack.len()
        || haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(word_class, c));
    before_ok && after_ok
}

/// Word-boundary, case-insensitive presence check for any of the supplied names
/// in the haystack. Empty/whitespace names are ignored. Equivalent to v4's
/// `nameAppears`.
pub fn name_appears(names: &[String], haystack: &str) -> bool {
    if haystack.is_empty() {
        return false;
    }
    let word_class = Regex::new(r"[\p{L}\p{N}_]").unwrap();
    for raw in names {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        let re = Regex::new(&format!("(?i){}", regex::escape(name))).unwrap();
        for m in re.find_iter(haystack) {
            if occurrence_is_bounded(&word_class, haystack, m.start(), m.end()) {
                return true;
            }
        }
    }
    false
}

/// Word-boundary, case-insensitive occurrence count summed across the supplied
/// names. Duplicate names in the list double-count (v4 sums per name). Empty and
/// whitespace-only names are ignored. Equivalent to v4's `countNameOccurrences`.
pub fn count_name_occurrences(names: &[String], haystack: &str) -> usize {
    if haystack.is_empty() {
        return 0;
    }
    let word_class = Regex::new(r"[\p{L}\p{N}_]").unwrap();
    let mut total = 0;
    for raw in names {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        let re = Regex::new(&format!("(?i){}", regex::escape(name))).unwrap();
        for m in re.find_iter(haystack) {
            if occurrence_is_bounded(&word_class, haystack, m.start(), m.end()) {
                total += 1;
            }
        }
    }
    total
}

/// Why a memory's about-character attribution was flipped to the holder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AboutFlipReason {
    /// The holder is named strictly more often than the about-character.
    HolderDominates,
}

/// Result of [`resolve_about_character_id`]: the resolved attribution, whether
/// it was flipped from the proposal, and (when flipped on the tiebreaker) why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AboutResolution {
    pub about_character_id: Option<String>,
    pub flipped: bool,
    pub reason: Option<AboutFlipReason>,
}

/// Decide what `aboutCharacterId` a memory should carry. Mirrors v4's
/// `resolveAboutCharacterId` rule order:
///   1. null proposal → unchanged (null);
///   2. self-reference (proposal == holder) → keep;
///   3. about-character data unavailable → keep;
///   4. about-character's names absent from text → flip to holder;
///   5. tiebreaker — when the holder is supplied and named strictly more often
///      than the about-character, flip to holder (`holder-dominates`);
///   6. otherwise keep the about-character.
///
/// `holder_character` / `proposed_about_character` carry `(name, aliases[, …])`.
pub fn resolve_about_character_id(
    holder_character_id: &str,
    holder_character: Option<(&str, &[String])>,
    proposed_about_character_id: Option<&str>,
    proposed_about_character: Option<(&str, &[String], &str)>,
    text: &str,
) -> AboutResolution {
    let proposed_id = match proposed_about_character_id {
        None => {
            return AboutResolution {
                about_character_id: None,
                flipped: false,
                reason: None,
            };
        }
        Some(id) => id,
    };
    if proposed_id == holder_character_id {
        return AboutResolution {
            about_character_id: Some(holder_character_id.to_string()),
            flipped: false,
            reason: None,
        };
    }
    let (about_name, about_aliases, about_controlled_by) = match proposed_about_character {
        None => {
            return AboutResolution {
                about_character_id: Some(proposed_id.to_string()),
                flipped: false,
                reason: None,
            };
        }
        Some(c) => c,
    };

    let about_names = names_for_about_character(about_name, about_aliases, about_controlled_by);
    let about_count = count_name_occurrences(&about_names, text);
    if about_count == 0 {
        return AboutResolution {
            about_character_id: Some(holder_character_id.to_string()),
            flipped: true,
            reason: None,
        };
    }

    if let Some((holder_name, holder_aliases)) = holder_character {
        let holder_names = names_for_holder(holder_name, holder_aliases);
        let holder_count = count_name_occurrences(&holder_names, text);
        if holder_count > about_count {
            return AboutResolution {
                about_character_id: Some(holder_character_id.to_string()),
                flipped: true,
                reason: Some(AboutFlipReason::HolderDominates),
            };
        }
    }

    AboutResolution {
        about_character_id: Some(proposed_id.to_string()),
        flipped: false,
        reason: None,
    }
}
