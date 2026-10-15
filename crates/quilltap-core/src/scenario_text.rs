//! Port of v4's lib/chat/scenario-text.ts — combine a resolved preset scenario
//! body with the user's free-text scenario notes into the single string
//! persisted as `chat.scenarioText`, and (since `545e299bb`) recover the
//! preset body back out of a stored text.
//!
//! Rules (preserved from v4):
//!   * The preset body keeps its leading whitespace; only its trailing
//!     whitespace is trimmed (`trimEnd`). Free text is fully trimmed.
//!   * When only one side is non-empty, the result is just that side — no stray
//!     separator. When both are present, the free text is appended beneath the
//!     preset with a single blank line.
//!   * When both sides are empty, the result is `None` (the caller stores
//!     `scenarioText` as null).
//!
//! "Whitespace" is JavaScript's (`String.prototype.trim`), not Rust's: the two
//! disagree on U+FEFF (JS trims it) and U+0085 (Rust trims it). Pinned tier-1
//! by `scenario_pure_equivalence`.

use crate::jsstr::{js_trim, js_trim_end};

/// Combine a preset scenario body with free-text notes. Returns `None` when
/// nothing survives trimming on either side.
pub fn combine_scenario_text(preset_body: Option<&str>, free_text: Option<&str>) -> Option<String> {
    let base = preset_body.map(js_trim_end);
    let extra = free_text.map(js_trim);
    let parts: Vec<&str> = [base, extra]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

/// v4 `recoverPresetBody(scenarioText, notes)` (`scenario-text.ts:33-42`): the
/// preset body inside a stored `scenarioText`, given the notes that were
/// layered beneath it — the inverse of [`combine_scenario_text`]. `None` when
/// the text does not end in those notes the way `combine_scenario_text` writes
/// them, so the caller can fall back rather than guess.
///
/// JS truthiness: an empty `scenarioText` is falsy (`None`); empty or
/// whitespace-only notes mean there is nothing to strip, so the text comes back
/// whole.
pub fn recover_preset_body(scenario_text: Option<&str>, notes: Option<&str>) -> Option<String> {
    let text = scenario_text.filter(|t| !t.is_empty())?;
    let extra = notes.map(js_trim).filter(|e| !e.is_empty());
    let Some(extra) = extra else {
        return Some(text.to_string());
    };
    let suffix = format!("\n\n{extra}");
    text.strip_suffix(suffix.as_str()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rust twins of v4's `scenario-text.test.ts` `recoverPresetBody` cases.
    #[test]
    fn recover_preset_body_inverts_combine() {
        assert_eq!(recover_preset_body(None, Some("n")), None);
        assert_eq!(recover_preset_body(Some(""), Some("n")), None);
        assert_eq!(
            recover_preset_body(Some("body"), None).as_deref(),
            Some("body")
        );
        assert_eq!(
            recover_preset_body(Some("body"), Some("   ")).as_deref(),
            Some("body")
        );
        assert_eq!(
            recover_preset_body(Some("body\n\nnotes"), Some("  notes ")).as_deref(),
            Some("body")
        );
        assert_eq!(
            recover_preset_body(Some("body\nnotes"), Some("notes")),
            None
        );
        let combined = combine_scenario_text(Some("Body.  "), Some("  more  ")).unwrap();
        assert_eq!(
            recover_preset_body(Some(&combined), Some("more")).as_deref(),
            Some("Body.")
        );
    }

    #[test]
    fn combine_trims_javascript_whitespace() {
        // U+FEFF is JS whitespace; U+0085 is not.
        assert_eq!(
            combine_scenario_text(Some("a\u{FEFF}"), None).as_deref(),
            Some("a")
        );
        assert_eq!(
            combine_scenario_text(Some("a\u{0085}"), None).as_deref(),
            Some("a\u{0085}")
        );
    }
}
