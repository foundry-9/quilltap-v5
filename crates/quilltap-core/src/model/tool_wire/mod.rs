//! Tool wire (W4.7c) — the provider tool-format reshape (`formatTools`), the
//! native tool-call parse (`parseToolCalls` / `detectToolCalls`), and the
//! spontaneous text-marker detect/parse/strip that v4 hangs off each provider
//! plugin. Ported from v4 `packages/plugin-utils/src/tools/*` + the per-plugin
//! `index.ts` glue, dispatched here by the provider's manifest [`ToolFormat`]
//! (the manifest registry replaces `getProvider(...)`).
//!
//! **Sans-IO / pure.** These are the primitives; the services layer wires them
//! into the seams: [`crate::services::native_tool_loop`]'s `ToolCallDetector`
//! (the native parse), [`crate::services::text_tool_loop`]'s
//! `ProviderTextMarkersStrategy` (the text markers), and
//! [`crate::services::tool_build`]'s provider reshape (`formatTools`). This module
//! lives under [`crate::model`] so it may NOT depend on `services` (that would
//! cycle) — hence the trait impls live in those services, calling in here.
//!
//! **Dispatch note.** Only the built-in `GOOGLE` provider has
//! [`ToolFormat::Google`], so gating the two Google-specific behaviors (the
//! `parseToolCalls` `functionCalls` fast path, and the tool_use-only text-marker
//! set) on `tool_format == Google` reproduces v4's per-plugin dispatch exactly.

mod converters;
mod parsers;
mod text_parsers;

use serde_json::Value;

use crate::provider_manifest::{Registry, ToolFormat};

pub use converters::{convert_to_anthropic_format, convert_to_google_format};
pub use parsers::{
    normalize_js_numbers, parse_anthropic_tool_calls, parse_google_plugin_tool_calls,
    parse_google_tool_calls, parse_openai_tool_calls, ToolCallRequest,
};
pub use text_parsers::{
    convert_to_tool_call_request, has_any_xml_tool_markers, has_tool_use_markers,
    normalize_tool_name, parse_all_xml_as_tool_calls, parse_all_xml_formats,
    parse_function_call_format, parse_function_calls_format, parse_invoke_format,
    parse_tool_call_format, parse_tool_use_format, strip_all_xml_tool_markers,
    strip_tool_use_markers, strip_tool_use_markers_with_cleanup, ParsedTextTool, TextToolFormat,
};

// ============================================================================
// formatTools reshape (v4 buildToolsForProvider step 3-5)
// ============================================================================

/// v4 `buildToolsForProvider`'s reshape step: hand the canonicalized universal
/// tools to the provider plugin's `formatTools`. Reproduced here by dispatching
/// on the manifest [`ToolFormat`]:
///
/// - provider not in the registry (`getProvider === null`) → the canonical tools
///   pass through UNCHANGED (v4's backward-compat fallback — no `function` filter);
/// - otherwise every provider's `formatTools` first drops any tool lacking a
///   `function` property, then reshapes: [`ToolFormat::Openai`] passes through,
///   [`ToolFormat::Anthropic`] → `input_schema` shape, [`ToolFormat::Google`] →
///   `parameters` shape.
pub fn format_tools_for_provider(
    registry: &Registry,
    provider: &str,
    canonical: &[Value],
) -> Vec<Value> {
    let Some(manifest) = registry.get_provider(provider) else {
        return canonical.to_vec();
    };
    let has_function = |t: &Value| t.get("function").is_some();
    match manifest.tool_format {
        ToolFormat::Openai => canonical
            .iter()
            .filter(|t| has_function(t))
            .cloned()
            .collect(),
        ToolFormat::Anthropic => canonical
            .iter()
            .filter(|t| has_function(t))
            .map(convert_to_anthropic_format)
            .collect(),
        ToolFormat::Google => canonical
            .iter()
            .filter(|t| has_function(t))
            .map(convert_to_google_format)
            .collect(),
    }
}

// ============================================================================
// Native tool-call detection (v4 detectToolCalls → plugin.parseToolCalls)
// ============================================================================

/// v4 `detectToolCalls(response, provider)`: dispatch to the provider plugin's
/// `parseToolCalls`. An unknown provider (`getProvider === null`) yields `[]` (v4
/// warns + returns empty).
pub fn detect_native_tool_calls(
    registry: &Registry,
    response: &Value,
    provider: &str,
) -> Vec<ToolCallRequest> {
    let Some(manifest) = registry.get_provider(provider) else {
        return Vec::new();
    };
    match manifest.tool_format {
        ToolFormat::Anthropic => parse_anthropic_tool_calls(response),
        ToolFormat::Google => parse_google_plugin_tool_calls(response),
        ToolFormat::Openai => parse_openai_tool_calls(response),
    }
}

// ============================================================================
// Provider text-markers (v4 plugin.hasTextToolMarkers / parseTextToolCalls /
// stripTextToolMarkers)
// ============================================================================

/// Whether the provider plugin implements the spontaneous-text-marker trio (v4's
/// `providerPlugin?.hasTextToolMarkers && parseTextToolCalls && stripTextToolMarkers`
/// gate). Every built-in provider does; an unknown provider does not (the pass is
/// skipped).
pub fn provider_has_text_markers(registry: &Registry, provider: &str) -> bool {
    registry.has_provider(provider)
}

/// v4 `plugin.hasTextToolMarkers(text)`. Google uses the tool_use-only check;
/// every other provider uses the composite XML check.
pub fn provider_has_text_tool_markers(registry: &Registry, provider: &str, text: &str) -> bool {
    match registry.get_provider(provider).map(|m| m.tool_format) {
        Some(ToolFormat::Google) => has_tool_use_markers(text),
        _ => has_any_xml_tool_markers(text),
    }
}

/// v4 `plugin.parseTextToolCalls(text)`. Google parses only the tool_use format;
/// every other provider parses all XML formats.
pub fn provider_parse_text_tool_calls(
    registry: &Registry,
    provider: &str,
    text: &str,
) -> Vec<ToolCallRequest> {
    match registry.get_provider(provider).map(|m| m.tool_format) {
        Some(ToolFormat::Google) => parse_tool_use_format(text)
            .iter()
            .map(convert_to_tool_call_request)
            .collect(),
        _ => parse_all_xml_as_tool_calls(text),
    }
}

/// v4 `plugin.stripTextToolMarkers(text)`. Google strips only tool_use (with the
/// whitespace cleanup); every other provider strips all XML formats.
pub fn provider_strip_text_tool_markers(registry: &Registry, provider: &str, text: &str) -> String {
    match registry.get_provider(provider).map(|m| m.tool_format) {
        Some(ToolFormat::Google) => strip_tool_use_markers_with_cleanup(text),
        _ => strip_all_xml_tool_markers(text),
    }
}
