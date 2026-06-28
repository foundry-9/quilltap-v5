//! Port of v4's lib/chat/scenario-text.ts — combine a resolved preset scenario
//! body with the user's free-text scenario notes into the single string
//! persisted as `chat.scenarioText`.
//!
//! Rules (preserved from v4):
//!   * The preset body keeps its leading whitespace; only its trailing
//!     whitespace is trimmed (`trimEnd`). Free text is fully trimmed.
//!   * When only one side is non-empty, the result is just that side — no stray
//!     separator. When both are present, the free text is appended beneath the
//!     preset with a single blank line.
//!   * When both sides are empty, the result is `None` (the caller stores
//!     `scenarioText` as null).

/// Combine a preset scenario body with free-text notes. Returns `None` when
/// nothing survives trimming on either side.
pub fn combine_scenario_text(preset_body: Option<&str>, free_text: Option<&str>) -> Option<String> {
    let base = preset_body.map(|s| s.trim_end().to_string());
    let extra = free_text.map(|s| s.trim().to_string());
    let parts: Vec<String> = [base, extra]
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
