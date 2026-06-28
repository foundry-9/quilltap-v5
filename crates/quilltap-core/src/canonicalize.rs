//! Port of v4's lib/tools/canonicalize.ts — byte-stable serialization of
//! `UniversalTool` arrays so the provider-side cache prefix stays identical
//! across turns.
//!
//! Two orderings are in play, and they are NOT the same:
//!
//!   * **Object keys inside `function.parameters`** are sorted with JS
//!     `Object.keys(obj).sort()` — the *default* `Array.sort`, i.e. UTF-16
//!     code-unit order, NOT locale collation. Rust's `str` `Ord` is
//!     Unicode-scalar order, which equals UTF-16 code-unit order across the BMP;
//!     every real JSON-Schema key is ASCII, so this is faithful (a
//!     supplementary-plane key would be a residual seam).
//!   * **The tool array** is sorted by `function.name.localeCompare(...)` — true
//!     ICU collation. The interface documents tool names as "lowercase with
//!     underscores", and that corpus (lowercase letters + `_`, no digits, no
//!     mixed case) collates identically under code-unit ordering, which is used
//!     here. Mixed-case / digit-bearing names would diverge; reproducing
//!     `localeCompare` generally is deferred to the ICU-collation decision when
//!     the ~30 Phase-2/3 `localeCompare` call sites land.
//!
//! The canonical tool object itself is rebuilt in v4's literal field order
//! (`type`, then `function` = `name`, `description`, `parameters`) — only
//! `parameters` is key-sorted — which the struct layout below reproduces.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool in OpenAI "function" shape. `parameters` is an arbitrary JSON-Schema
/// object carried as a `Value` so its keys can be deep-sorted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniversalTool {
    #[serde(rename = "type")]
    pub type_: String,
    pub function: ToolFunction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Recursively sort object keys (code-unit order), mapping into arrays and
/// passing scalars through — v4's `sortKeysDeep`. Implemented explicitly rather
/// than leaning on `serde_json`'s default `BTreeMap` ordering so it stays
/// correct even if the `preserve_order` feature is ever enabled.
fn sort_keys_deep(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            let mut sorted = serde_json::Map::new();
            for (k, v) in entries {
                sorted.insert(k, sort_keys_deep(v));
            }
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sort_keys_deep).collect()),
        other => other,
    }
}

/// Canonicalize one tool: fixed field order, with `function.parameters`
/// deep-key-sorted.
pub fn canonicalize_universal_tool(tool: &UniversalTool) -> UniversalTool {
    UniversalTool {
        type_: tool.type_.clone(),
        function: ToolFunction {
            name: tool.function.name.clone(),
            description: tool.function.description.clone(),
            parameters: sort_keys_deep(tool.function.parameters.clone()),
        },
    }
}

/// Canonicalize a tool array: each tool canonicalized, then the array sorted by
/// tool name (stably, as JS `Array.sort` and Rust `sort_by` both are). See the
/// module docs for the `localeCompare` residual seam.
pub fn canonicalize_universal_tools(tools: &[UniversalTool]) -> Vec<UniversalTool> {
    let mut out: Vec<UniversalTool> = tools.iter().map(canonicalize_universal_tool).collect();
    out.sort_by(|a, b| a.function.name.cmp(&b.function.name));
    out
}
