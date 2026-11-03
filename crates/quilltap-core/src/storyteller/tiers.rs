//! Storyteller bid tiers — what a bid may do once chosen (v4
//! `lib/storyteller/tiers.ts`, P4.D343).
//!
//! The server computes a bid's tier from the tools it names (D6): the highest
//! of the tier the character claimed and each named tool's effect. A named
//! built-in or plugin tool counts at its highest possible effect; a named
//! custom tool (`run_custom:<name>`) at its derived effect. A name that was not
//! on the bid's menu is dropped — it could not be used anyway. The claim is
//! only a cross-check: when the tools outrank it the board says so.

use super::types::{ParsedBid, StorytellerBidOffer, StorytellerTier};
use crate::jsstr::js_trim;
use crate::tools::effects::{max_tool_effect, ToolEffect};

/// The custom-tool prefix a bid uses to name one of Pascal's tools.
pub const CUSTOM_TOOL_PREFIX: &str = "run_custom:";

/// v4 `ComputedBidTier`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedBidTier {
    pub tier: StorytellerTier,
    /// True when the named tools put the bid above what the character claimed.
    pub raised: bool,
    /// Named tools that were on the menu, as given.
    pub tools: Vec<String>,
    /// Named tools that were not on the menu (dropped).
    pub unknown_tools: Vec<String>,
}

/// The effect of one named tool on a bid's menu, or `None` when it was not on
/// the menu (v4 `effectOfNamedTool`). A prefixed name is trimmed after the
/// prefix; a bare name hits the custom tools first, then the built-ins.
pub fn effect_of_named_tool(name: &str, offer: &StorytellerBidOffer) -> Option<ToolEffect> {
    if let Some(rest) = name.strip_prefix(CUSTOM_TOOL_PREFIX) {
        return offer.custom_tool(js_trim(rest));
    }
    if let Some(e) = offer.custom_tool(name) {
        return Some(e);
    }
    offer.tool(name)
}

/// v4 `computeBidTier(parsed, offer)`.
pub fn compute_bid_tier(parsed: &ParsedBid, offer: &StorytellerBidOffer) -> ComputedBidTier {
    let mut tools_effect = ToolEffect::None;
    let mut tools = Vec::new();
    let mut unknown_tools = Vec::new();
    for name in &parsed.tools {
        match effect_of_named_tool(name, offer) {
            None => unknown_tools.push(name.clone()),
            Some(effect) => {
                tools.push(name.clone());
                tools_effect = max_tool_effect(tools_effect, effect);
            }
        }
    }
    let stated = parsed.stated_tier.unwrap_or(StorytellerTier::Simple);
    let stated_effect = stated.effect_ceiling();
    let effect = max_tool_effect(stated_effect, tools_effect);
    let raised = parsed.stated_tier.is_some() && tools_effect.rank() > stated_effect.rank();
    // A Destroy claim without a destroy tool on the menu has nothing to destroy with.
    let tier = if effect == ToolEffect::Destroy && !offer.destroy_offered {
        StorytellerTier::Act
    } else {
        StorytellerTier::for_effect(effect)
    };
    ComputedBidTier {
        tier,
        raised,
        tools,
        unknown_tools,
    }
}
