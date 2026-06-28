//! Port of the pure name-set builders in v4's lib/memory/about-character-resolution.ts.
//!
//! These assemble the name/alias sets used to decide whether a memory is about
//! a given character. The word-boundary regex matchers that consume these sets
//! (`nameAppears`, `countNameOccurrences`, `resolveAboutCharacterId`) are
//! regex-bearing (Unicode `\p{L}\p{N}` boundaries, lookahead) and are deferred
//! to the regex-fidelity wave; the set builders themselves are pure.

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
