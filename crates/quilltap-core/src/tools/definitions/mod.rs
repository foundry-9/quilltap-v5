//! Tool-definition catalog (wave 4 / W4.1b.3).
//!
//! The byte-exact port of every v4 `lib/tools/*-tool.ts` definition. The raw
//! `{name, description, parameters}` JSON lives in the generated [`data`]
//! submodule (transcribed from v4's `JSON.stringify` output — see its header for
//! the regen recipe); this module provides the accessor API and the bridge into
//! [`crate::canonicalize`] (`UniversalTool`).

mod data;

use serde_json::Value;

use crate::canonicalize::{ToolFunction, UniversalTool};

pub use data::TOOL_DEFINITIONS;

/// One catalog entry: the v4 `ALL_TOOLS` camelCase key, the `function.name`, and
/// the byte-exact `{name, description, parameters}` JSON.
pub struct ToolDef {
    pub key: &'static str,
    pub name: &'static str,
    pub json: &'static str,
}

/// Look up the raw definition JSON by its `ALL_TOOLS` key (e.g. `"rng"`,
/// `"searchScriptorium"`).
pub fn definition_json_by_key(key: &str) -> Option<&'static str> {
    TOOL_DEFINITIONS
        .iter()
        .find(|d| d.key == key)
        .map(|d| d.json)
}

/// Parse a definition (by key) into its `{name, description, parameters}` value.
pub fn definition_by_key(key: &str) -> Option<Value> {
    definition_json_by_key(key).map(|j| serde_json::from_str(j).expect("catalog JSON is valid"))
}

/// Look up the raw definition JSON by its `function.name` (e.g. `"rng"`).
/// Returns the first match (definitions have distinct names).
pub fn definition_json_by_name(name: &str) -> Option<&'static str> {
    TOOL_DEFINITIONS
        .iter()
        .find(|d| d.name == name)
        .map(|d| d.json)
}

/// Build a [`UniversalTool`] (`type: "function"`, `function: {name, description,
/// parameters}`) from a parsed definition value.
fn universal_from_value(def: &Value) -> UniversalTool {
    UniversalTool {
        type_: "function".to_string(),
        function: ToolFunction {
            name: def
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: def
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            parameters: def.get("parameters").cloned().unwrap_or(Value::Null),
        },
    }
}

/// One [`UniversalTool`] by catalog key.
pub fn universal_tool_by_key(key: &str) -> Option<UniversalTool> {
    definition_by_key(key).map(|def| universal_from_value(&def))
}

/// The full catalog as [`UniversalTool`]s, in `ALL_TOOLS` order.
pub fn all_universal_tools() -> Vec<UniversalTool> {
    TOOL_DEFINITIONS
        .iter()
        .map(|d| {
            let def: Value = serde_json::from_str(d.json).expect("catalog JSON is valid");
            universal_from_value(&def)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_complete_and_parses() {
        // 57 since P4.d13 unit 6 DELETED `memorySearch` (v4 8bf3cb5f removed
        // `memory-search-tool.ts` outright — the old memory-only tool collided
        // with the Scriptorium `search` on the wire name `search`). The count is
        // the tripwire for a regenerated catalog silently gaining or losing a
        // tool.
        assert_eq!(TOOL_DEFINITIONS.len(), 57);
        for d in TOOL_DEFINITIONS {
            let v: Value = serde_json::from_str(d.json).expect("valid JSON");
            assert_eq!(v.get("name").and_then(Value::as_str), Some(d.name));
            assert_eq!(
                v.get("parameters")
                    .and_then(|p| p.get("type"))
                    .and_then(Value::as_str),
                Some("object"),
                "tool {} parameters.type",
                d.key
            );
        }
    }

    #[test]
    fn lookup_by_key_and_name() {
        assert!(definition_json_by_key("rng").is_some());
        assert!(definition_json_by_name("rng").is_some());
        assert!(definition_json_by_key("nonexistent").is_none());
        // `run_custom` is published under the camelCase key but the snake_case
        // function name — the pair a lookup typo would silently swap.
        assert!(definition_json_by_key("runCustom").is_some());
        assert!(definition_json_by_name("run_custom").is_some());
        let ut = universal_tool_by_key("rng").unwrap();
        assert_eq!(ut.type_, "function");
        assert_eq!(ut.function.name, "rng");
    }
}
