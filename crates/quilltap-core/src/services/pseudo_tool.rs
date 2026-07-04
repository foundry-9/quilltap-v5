//! Tool-mode support service — port of the pure exports in v4
//! `lib/services/chat-message/pseudo-tool.service.ts`.
//!
//! Thin wrappers over `tools::*`. The `log*ToolUsage` functions are logging-only
//! and are NOT ported (no observable output). `checkModelSupportsTools` (async
//! pricing lookup) is the host-side boundary and is out of scope; its boolean
//! result is the injected `supports_native_tools` input.

use serde_json::Value;

use crate::tools::native_tool_prompt::build_native_tool_instructions;
use crate::tools::pseudo_tool_support::{
    resolve_tool_mode, should_use_text_block_tools, ResolvedToolMode, ToolMode,
};
use crate::tools::simple_json_parser as sj;
use crate::tools::simple_json_prompt::{
    build_simple_json_tool_instructions, SimpleJsonPromptOptions,
};
use crate::tools::text_block_parser as tb;
use crate::tools::text_block_prompt::{build_text_block_instructions, TextBlockPromptOptions};

/// v4 `EnabledToolOptions`.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnabledToolOptions {
    pub image_generation: bool,
    pub search: bool,
    pub web_search: bool,
}

/// v4 `TextBlockEnabledToolOptions` (extends `EnabledToolOptions`).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBlockEnabledToolOptions {
    pub image_generation: bool,
    pub search: bool,
    pub web_search: bool,
    pub whisper: bool,
    pub state: bool,
    pub rng: bool,
    pub project_info: bool,
    pub help_search: bool,
    pub help_settings: bool,
    pub help_navigate: bool,
    pub create_note: bool,
    pub wardrobe_list: bool,
    pub wardrobe_read: bool,
    pub wardrobe_wear: bool,
    pub wardrobe_take_off: bool,
    pub wardrobe_create: bool,
    pub wardrobe_update: bool,
    pub wardrobe_archive: bool,
}

/// A converted tool-call request as the loops consume it.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallRequest {
    pub name: String,
    pub arguments: Value,
}

/// v4 `buildNativeToolSystemInstructions`.
pub fn build_native_tool_system_instructions() -> String {
    build_native_tool_instructions(true)
}

/// v4 `determineEnabledToolOptions`.
pub fn determine_enabled_tool_options(
    image_profile_id: Option<&str>,
    allow_web_search: bool,
) -> EnabledToolOptions {
    EnabledToolOptions {
        image_generation: image_profile_id.is_some(),
        search: true,
        web_search: allow_web_search,
    }
}

/// v4 `checkShouldUseTextBlockTools`.
pub fn check_should_use_text_block_tools(
    model_supports_native_tools: bool,
    profile_override: Option<ToolMode>,
) -> bool {
    should_use_text_block_tools(model_supports_native_tools, profile_override)
}

/// v4 `determineTextBlockToolOptions`. `canDressThemselves`/`canCreateOutfits`
/// default ON (`!== false`).
#[allow(clippy::too_many_arguments)]
pub fn determine_text_block_tool_options(
    image_profile_id: Option<&str>,
    allow_web_search: bool,
    is_multi_character: bool,
    has_project: bool,
    help_tools_enabled: Option<bool>,
    can_dress_themselves: Option<bool>,
    can_create_outfits: Option<bool>,
) -> TextBlockEnabledToolOptions {
    let can_dress = can_dress_themselves != Some(false);
    let can_create = can_create_outfits != Some(false);
    TextBlockEnabledToolOptions {
        image_generation: image_profile_id.is_some(),
        search: true,
        web_search: allow_web_search,
        whisper: is_multi_character,
        state: true,
        rng: true,
        project_info: has_project,
        help_search: help_tools_enabled.unwrap_or(false),
        help_settings: help_tools_enabled.unwrap_or(false),
        help_navigate: help_tools_enabled.unwrap_or(false),
        create_note: true,
        wardrobe_list: can_dress,
        wardrobe_read: can_dress,
        wardrobe_wear: can_dress,
        wardrobe_take_off: can_dress,
        wardrobe_create: can_create,
        wardrobe_update: can_create,
        wardrobe_archive: can_create,
    }
}

/// v4 `buildTextBlockSystemInstructions`.
pub fn build_text_block_system_instructions(o: &TextBlockEnabledToolOptions) -> String {
    build_text_block_instructions(&TextBlockPromptOptions {
        whisper: Some(o.whisper),
        search: Some(o.search),
        image_generation: Some(o.image_generation),
        web_search: Some(o.web_search),
        state: Some(o.state),
        rng: Some(o.rng),
        project_info: Some(o.project_info),
        help_search: Some(o.help_search),
        help_settings: Some(o.help_settings),
        help_navigate: Some(o.help_navigate),
        create_note: Some(o.create_note),
        wardrobe_list: Some(o.wardrobe_list),
        wardrobe_read: Some(o.wardrobe_read),
        wardrobe_wear: Some(o.wardrobe_wear),
        wardrobe_take_off: Some(o.wardrobe_take_off),
        wardrobe_create: Some(o.wardrobe_create),
        wardrobe_update: Some(o.wardrobe_update),
        wardrobe_archive: Some(o.wardrobe_archive),
    })
}

/// v4 `parseTextBlocksFromResponse`.
pub fn parse_text_blocks_from_response(response: &str) -> Vec<ToolCallRequest> {
    tb::parse_text_block_calls(response)
        .iter()
        .map(|p| {
            let r = tb::convert_text_block_to_tool_call_request(p);
            ToolCallRequest {
                name: r.name,
                arguments: r.arguments,
            }
        })
        .collect()
}

/// v4 `stripTextBlockMarkersFromResponse`.
pub fn strip_text_block_markers_from_response(response: &str) -> String {
    tb::strip_text_block_markers(response)
}

/// v4 `checkResolvedToolMode`.
pub fn check_resolved_tool_mode(
    model_supports_native_tools: bool,
    profile_override: Option<ToolMode>,
) -> ResolvedToolMode {
    resolve_tool_mode(model_supports_native_tools, profile_override)
}

/// v4 `buildSimpleJsonSystemInstructions`.
pub fn build_simple_json_system_instructions(o: &TextBlockEnabledToolOptions) -> String {
    build_simple_json_tool_instructions(&SimpleJsonPromptOptions {
        whisper: Some(o.whisper),
        search: Some(o.search),
        image_generation: Some(o.image_generation),
        web_search: Some(o.web_search),
        state: Some(o.state),
        rng: Some(o.rng),
        project_info: Some(o.project_info),
        help_search: Some(o.help_search),
        help_settings: Some(o.help_settings),
        help_navigate: Some(o.help_navigate),
        create_note: Some(o.create_note),
        wardrobe_list: Some(o.wardrobe_list),
        wardrobe_read: Some(o.wardrobe_read),
        wardrobe_wear: Some(o.wardrobe_wear),
        wardrobe_take_off: Some(o.wardrobe_take_off),
        wardrobe_create: Some(o.wardrobe_create),
        wardrobe_update: Some(o.wardrobe_update),
        wardrobe_archive: Some(o.wardrobe_archive),
    })
}

/// v4 `parseSimpleJsonFromResponse`.
pub fn parse_simple_json_from_response(response: &str) -> Vec<ToolCallRequest> {
    sj::parse_simple_json_calls(response)
        .iter()
        .map(|p| {
            let r = sj::convert_simple_json_to_tool_call_request(p);
            ToolCallRequest {
                name: r.name,
                arguments: r.arguments,
            }
        })
        .collect()
}

/// v4 `stripSimpleJsonFromResponse`.
pub fn strip_simple_json_from_response(response: &str) -> String {
    sj::strip_simple_json_markers(response)
}

/// v4 `hasSimpleJsonInResponse`.
pub fn has_simple_json_in_response(response: &str) -> bool {
    sj::has_simple_json_markers(response)
}

/// v4 `formatSimpleJsonToolResult`.
pub fn format_simple_json_tool_result(tool_name: &str, content: &str) -> String {
    format!(
        "<tool_result name=\"{}\">\n{}\n</tool_result>",
        sj::escape_xml_attribute(tool_name),
        content
    )
}

/// v4 `SIMPLE_JSON_STOP`.
pub const SIMPLE_JSON_STOP: &[&str] = sj::SIMPLE_JSON_STOP_SEQUENCES;
