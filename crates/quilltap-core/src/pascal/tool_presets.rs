//! Custom-tool run presets — the naming contract (v4 `lib/pascal/tool-presets.ts`,
//! `fd165fce0`).
//!
//! A preset is an ordinary JSON document in a store's `Tools/` folder,
//! `Tools/{toolname}.{preset}.settings.json`: a flat object of parameter name →
//! value, saved from and loaded back into the run dialog's form. This module is
//! the server's single source of truth for the filename shape — the preset walk
//! ([`super::tool_preset_pool`]) parses paths through it, never by hand.
//!
//! The preset segment is deliberately stricter than a general filename: tool
//! names (`IDENTIFIER_PATTERN`) never contain a dot, so keeping dots out of the
//! preset name too makes `{toolname}.{preset}.settings.json` parse
//! unambiguously — and keeps the name safe on every filesystem shape a vault can
//! take.
//!
//! The client half is the SPA's own twin, `apps/web/src/app/pascal/
//! tool-presets.ts`. v4's two client-only helpers — `sanitizePresetNameInput`
//! (the dialog's keystroke filter) and `presetSettingsPath` (the save path) —
//! are NOT ported here (ruling P-2): no server code calls either, and the SPA
//! keeps its own, proven by its own spec.

use std::sync::LazyLock;

use regex::Regex;

use super::custom_tool_types::TOOLS_FOLDER;

/// What a preset name must look like: lowercase, filename-safe, dot-free (v4
/// `PRESET_NAME_PATTERN`, no flags). ASCII-only, so `{0,63}` counts the same in
/// UTF-16 units and in chars.
pub static PRESET_NAME_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-z0-9][a-z0-9_-]{0,63}$").expect("preset name pattern compiles")
});

/// The suffix every preset file wears (v4 `PRESET_FILE_SUFFIX`).
pub const PRESET_FILE_SUFFIX: &str = ".settings.json";

/// The preset name a store path holds for `tool_name`, or `None` when the path
/// is anything else: another tool's preset, a nested path, a `.tool.json`
/// definition, or a preset segment that would not have been accepted as input
/// (v4 `parsePresetFromPath`).
///
/// Prefix and suffix are compared CASE-SENSITIVELY, as v4's `startsWith` /
/// `endsWith` do — unlike the definition filter, which lowercases. The tool name
/// itself is not validated here (the route validates it first).
pub fn parse_preset_from_path(tool_name: &str, relative_path: &str) -> Option<String> {
    let prefix = format!("{TOOLS_FOLDER}/{tool_name}.");
    if !relative_path.starts_with(&prefix) || !relative_path.ends_with(PRESET_FILE_SUFFIX) {
        return None;
    }
    // v4 `relativePath.slice(prefix.length, -PRESET_FILE_SUFFIX.length)`. When
    // the prefix and the suffix overlap (`Tools/coin.settings.json` for tool
    // `coin`), the start lands past the end and JS's `slice` answers `""`,
    // which the pattern refuses. A Rust byte slice would PANIC there instead,
    // so the overlap is guarded explicitly. Both bounds sit on char boundaries:
    // the prefix ends on one and the ASCII suffix starts on one.
    let start = prefix.len();
    let end = relative_path.len() - PRESET_FILE_SUFFIX.len();
    let preset = if start > end {
        ""
    } else {
        &relative_path[start..end]
    };
    PRESET_NAME_PATTERN
        .is_match(preset)
        .then(|| preset.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The overlap rows answer `None` rather than panicking (mutation proof:
    /// drop the `start > end` guard and this test panics on the slice).
    #[test]
    fn an_overlapping_prefix_and_suffix_is_no_preset_and_no_panic() {
        assert_eq!(
            parse_preset_from_path("coin", "Tools/coin.settings.json"),
            None
        );
        assert_eq!(
            parse_preset_from_path("a.settings", "Tools/a.settings.json"),
            None
        );
    }

    #[test]
    fn a_well_formed_path_round_trips() {
        assert_eq!(
            parse_preset_from_path("coin_toss", "Tools/coin_toss.hard-mode.settings.json"),
            Some("hard-mode".to_string())
        );
    }
}
