//! Port of v4's lib/memory/format-utils.ts — shared formatting helpers used
//! across the memory-extraction modules.

/// A character's pronoun set (subject / object / possessive), e.g.
/// `she` / `her` / `her`.
#[derive(Clone, Debug)]
pub struct Pronouns {
    pub subject: String,
    pub object: String,
    pub possessive: String,
}

/// Format a name with pronouns appended, e.g. `"Friday (she/her/her)"`. Returns
/// just the name when pronouns are absent.
pub fn format_name_with_pronouns(name: &str, pronouns: Option<&Pronouns>) -> String {
    match pronouns {
        None => name.to_string(),
        Some(p) => format!("{name} ({}/{}/{})", p.subject, p.object, p.possessive),
    }
}
