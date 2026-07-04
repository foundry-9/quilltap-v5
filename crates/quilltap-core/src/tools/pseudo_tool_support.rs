//! Tool-mode resolution — port of the pure helpers in v4
//! `lib/tools/pseudo-tool-support.ts`.
//!
//! `checkModelSupportsTools` (the async OpenRouter/fallback pricing lookup) is
//! NOT ported here: it consumes the pricing-fetcher cache, a host-side boundary
//! outside the pure-leaf scope. Its result (a `supports_native_tools` boolean) is
//! the injected input to [`resolve_tool_mode`] / [`should_use_text_block_tools`].

/// Tool-mode override on a connection profile. v4's `ToolMode` string union.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    /// Pick by model capability.
    Auto,
    /// Force native function calling (falls back to simple-json on a
    /// non-native model).
    Native,
    /// `<tool_call>{...}</tool_call>` JSON-in-XML pseudo-tool format.
    SimpleJson,
    /// Legacy `[[TOOL_NAME ...]]content[[/TOOL_NAME]]` pseudo-tool format.
    TextBlock,
}

impl ToolMode {
    /// Parse the profile-override string. Unknown values map to `None` (treated
    /// as absent, mirroring v4 where a non-matching `profileOverride` falls
    /// through to the `'auto'`/undefined default).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<ToolMode> {
        match s {
            "auto" => Some(ToolMode::Auto),
            "native" => Some(ToolMode::Native),
            "simple-json" => Some(ToolMode::SimpleJson),
            "text-block" => Some(ToolMode::TextBlock),
            _ => None,
        }
    }
}

/// The concrete strategy a request resolves to. v4's `ResolvedToolMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedToolMode {
    Native,
    SimpleJson,
    TextBlock,
}

impl ResolvedToolMode {
    /// The wire string (for the differential / logging).
    pub fn as_str(self) -> &'static str {
        match self {
            ResolvedToolMode::Native => "native",
            ResolvedToolMode::SimpleJson => "simple-json",
            ResolvedToolMode::TextBlock => "text-block",
        }
    }
}

/// Resolve the concrete strategy for a request (v4 `resolveToolMode`).
///
/// Precedence: an explicit `text-block`/`simple-json` override always wins; a
/// `native` override is honoured only when the model supports it (else
/// simple-json); `auto`/absent → native on a native model, simple-json
/// otherwise.
pub fn resolve_tool_mode(
    supports_native_tools: bool,
    profile_override: Option<ToolMode>,
) -> ResolvedToolMode {
    match profile_override {
        Some(ToolMode::TextBlock) => ResolvedToolMode::TextBlock,
        Some(ToolMode::SimpleJson) => ResolvedToolMode::SimpleJson,
        // `native` is honoured only if the model supports it (else simple-json);
        // `auto`/absent resolves to the same native-or-simple-json default — v4
        // writes these as two separate branches with identical bodies.
        _ => {
            if supports_native_tools {
                ResolvedToolMode::Native
            } else {
                ResolvedToolMode::SimpleJson
            }
        }
    }
}

/// Whether a pseudo-tool surface (simple-json OR text-block) applies — i.e. the
/// resolved mode is not native. v4 `shouldUseTextBlockTools` (the legacy boolean
/// name persists for back-compat).
pub fn should_use_text_block_tools(
    supports_native_tools: bool,
    profile_override: Option<ToolMode>,
) -> bool {
    resolve_tool_mode(supports_native_tools, profile_override) != ResolvedToolMode::Native
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_precedence() {
        assert_eq!(
            resolve_tool_mode(true, Some(ToolMode::TextBlock)),
            ResolvedToolMode::TextBlock
        );
        assert_eq!(
            resolve_tool_mode(true, Some(ToolMode::SimpleJson)),
            ResolvedToolMode::SimpleJson
        );
        assert_eq!(
            resolve_tool_mode(false, Some(ToolMode::Native)),
            ResolvedToolMode::SimpleJson
        );
        assert_eq!(
            resolve_tool_mode(true, Some(ToolMode::Native)),
            ResolvedToolMode::Native
        );
        assert_eq!(resolve_tool_mode(true, None), ResolvedToolMode::Native);
        assert_eq!(resolve_tool_mode(false, None), ResolvedToolMode::SimpleJson);
        assert_eq!(
            resolve_tool_mode(false, Some(ToolMode::Auto)),
            ResolvedToolMode::SimpleJson
        );
    }

    #[test]
    fn should_use_text_block() {
        assert!(!should_use_text_block_tools(true, None));
        assert!(should_use_text_block_tools(false, None));
        assert!(should_use_text_block_tools(true, Some(ToolMode::TextBlock)));
        assert!(should_use_text_block_tools(
            true,
            Some(ToolMode::SimpleJson)
        ));
    }
}
