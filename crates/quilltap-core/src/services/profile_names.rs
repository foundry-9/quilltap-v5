//! Connection-profile name helpers — v4 `lib/llm/connection-profile-names.ts`.
//!
//! Connection-profile names are unique per user, case-insensitively and
//! ignoring surrounding whitespace (the DB guarantee is v4's expression unique
//! index on `(userId, lower(trim(name)))`). This module is the single source of
//! truth for both the normalization rule and the "append a numeric suffix until
//! it's unique" rule, shared by the settings validators, the qtap importer, and
//! the backup-restore merge path (which each previously carried private copies).

use std::collections::HashSet;

/// v4 `normalizeProfileName`: `trim()` then `toLowerCase()`. `str::to_lowercase`
/// is byte-identical to JS `toLowerCase` (the closed case-mapping seam).
pub fn normalize_profile_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// v4 `makeUniqueProfileName` — `desired` trimmed, or `desired (2)`,
/// `desired (3)`, … until it stops colliding (case-insensitively) with
/// `taken`, whose entries are pre-normalized via [`normalize_profile_name`].
/// Callers minting several names in a row must add each returned name's
/// normalized form back into the set before the next call.
pub fn make_unique_profile_name(desired: &str, taken: &HashSet<String>) -> String {
    let base = desired.trim().to_string();
    if !taken.contains(&normalize_profile_name(&base)) {
        return base;
    }
    let mut i = 2u32;
    loop {
        let candidate = format!("{base} ({i})");
        if !taken.contains(&normalize_profile_name(&candidate)) {
            return candidate;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_trim_then_lowercase() {
        assert_eq!(normalize_profile_name("  My Profile "), "my profile");
    }

    #[test]
    fn unique_name_appends_from_two() {
        let mut taken: HashSet<String> = HashSet::new();
        assert_eq!(make_unique_profile_name(" Fresh ", &taken), "Fresh");

        taken.insert("fresh".to_string());
        assert_eq!(make_unique_profile_name(" Fresh ", &taken), "Fresh (2)");

        taken.insert("fresh (2)".to_string());
        assert_eq!(make_unique_profile_name("Fresh", &taken), "Fresh (3)");
    }
}
