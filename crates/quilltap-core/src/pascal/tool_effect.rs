//! A custom tool's effect on the world, derived from its definition (v4
//! `lib/pascal/tool-effect.ts`, P4.D343).
//!
//! A custom tool is `read` unless it carries effects — a `state` / `progress`
//! write, a wardrobe or inventory gesture, or a log line — in which case it is
//! `write`. Rolling, consulting an LLM and posting the outcome are reporting,
//! not side effects. No custom tool deletes anything, so none is `destroy`.
//!
//! The Workbench shows the derived effect, the bid prompt lists it, and
//! `run_custom`'s computed effect and the trigger runner enforce it.

use std::collections::BTreeMap;

use crate::pascal::custom_tool_types::QtapCustomTool;
use crate::tools::effects::ToolEffect;

/// v4 `customToolEffect(definition)`.
pub fn custom_tool_effect(definition: &QtapCustomTool) -> ToolEffect {
    let writes = definition
        .effects
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .any(|effect| {
            effect.is_write_effect()
                || effect.is_wardrobe_effect()
                || effect.is_inventory_effect()
                || effect.is_log_effect()
        });
    if writes {
        ToolEffect::Write
    } else {
        ToolEffect::Read
    }
}

/// Custom tool name → derived effect, for a resolved roster (v4
/// `customToolEffectMap`; a later duplicate name overwrites an earlier one, as
/// `Map.set` does).
pub fn custom_tool_effect_map<'a, I>(tools: I) -> BTreeMap<String, ToolEffect>
where
    I: IntoIterator<Item = (&'a str, &'a QtapCustomTool)>,
{
    let mut map = BTreeMap::new();
    for (name, definition) in tools {
        map.insert(name.to_string(), custom_tool_effect(definition));
    }
    map
}
