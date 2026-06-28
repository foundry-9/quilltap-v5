//! Port of v4's lib/tags/styles.ts — merge a partial tag visual style with the
//! defaults. Note the deliberate `||` vs `??` split: colours use `||` (an empty
//! string falls back to the default) while the booleans and emoji-only use `??`
//! (an explicit `false` is kept).

/// A fully-resolved tag visual style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagVisualStyle {
    pub emoji: Option<String>,
    pub foreground_color: String,
    pub background_color: String,
    pub emoji_only: bool,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
}

/// A partial style as supplied by a caller — every field optional. `emoji`
/// being `Some("")` is distinct from `None` (both resolve to no emoji, but for
/// different reasons in v4's check).
#[derive(Clone, Debug, Default)]
pub struct PartialTagVisualStyle {
    pub emoji: Option<String>,
    pub foreground_color: Option<String>,
    pub background_color: Option<String>,
    pub emoji_only: Option<bool>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strikethrough: Option<bool>,
}

const DEFAULT_FOREGROUND: &str = "#1f2937";
const DEFAULT_BACKGROUND: &str = "#e5e7eb";

/// The default tag style (emoji none, neutral colours, all flags off).
pub fn default_tag_style() -> TagVisualStyle {
    TagVisualStyle {
        emoji: None,
        foreground_color: DEFAULT_FOREGROUND.to_string(),
        background_color: DEFAULT_BACKGROUND.to_string(),
        emoji_only: false,
        bold: false,
        italic: false,
        strikethrough: false,
    }
}

/// Merge a partial style over the defaults. `None` (the whole style absent)
/// yields the defaults verbatim.
pub fn merge_with_default_tag_style(style: Option<&PartialTagVisualStyle>) -> TagVisualStyle {
    let Some(s) = style else {
        return default_tag_style();
    };
    TagVisualStyle {
        // `typeof emoji === 'string' && emoji.length > 0` — non-empty string only.
        emoji: s
            .emoji
            .as_deref()
            .filter(|e| !e.is_empty())
            .map(str::to_string),
        // `||` → an empty string falls through to the default.
        foreground_color: non_empty_or(&s.foreground_color, DEFAULT_FOREGROUND),
        background_color: non_empty_or(&s.background_color, DEFAULT_BACKGROUND),
        // `??` → an explicit value (incl. false) is kept; only absence defaults.
        emoji_only: s.emoji_only.unwrap_or(false),
        bold: s.bold.unwrap_or(false),
        italic: s.italic.unwrap_or(false),
        strikethrough: s.strikethrough.unwrap_or(false),
    }
}

fn non_empty_or(value: &Option<String>, default: &str) -> String {
    match value {
        Some(v) if !v.is_empty() => v.clone(),
        _ => default.to_string(),
    }
}
