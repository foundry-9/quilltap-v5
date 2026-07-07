//! Tool-format converters (W4.7c) — v4
//! `packages/plugin-utils/src/tools/converters.ts`.
//!
//! The universal (OpenAI "function") shape is the baseline; these reshape a
//! universal tool into a provider's native tool definition. The two reshapes the
//! `formatTools` reshape needs are Anthropic (`input_schema`) and Google
//! (`parameters`). Both emit their object keys in a FIXED order (`name`,
//! `description`, then the schema object) — reproduced by building the value with
//! `serde_json::json!` under `preserve_order`, matching v4's `JSON.stringify`
//! byte-for-byte.

use serde_json::{json, Value};

/// `parameters?.properties ?? {}` — the `properties` sub-object of a universal
/// tool's `function.parameters`, or an empty object.
fn properties_or_empty(function: &Value) -> Value {
    function
        .get("parameters")
        .and_then(|p| p.get("properties"))
        .cloned()
        .unwrap_or_else(|| json!({}))
}

/// `parameters?.required ?? []` — the `required` array, or an empty array.
fn required_or_empty(function: &Value) -> Value {
    function
        .get("parameters")
        .and_then(|p| p.get("required"))
        .cloned()
        .unwrap_or_else(|| json!([]))
}

/// `tool.function.description ?? ''`.
fn description_or_empty(function: &Value) -> Value {
    match function.get("description") {
        Some(Value::Null) | None => json!(""),
        Some(v) => v.clone(),
    }
}

/// `tool.function.name` (or `null` if absent — v4 reads it directly).
fn name_value(function: &Value) -> Value {
    function.get("name").cloned().unwrap_or(Value::Null)
}

/// v4 `convertToAnthropicFormat` (as composed by the anthropic plugin's
/// `formatTools`: build a universal tool with defaults, then convert). Emits
/// `{ name, description, input_schema: { type, properties, required } }`.
pub fn convert_to_anthropic_format(tool: &Value) -> Value {
    let function = tool.get("function").cloned().unwrap_or(Value::Null);
    json!({
        "name": name_value(&function),
        "description": description_or_empty(&function),
        "input_schema": {
            "type": "object",
            "properties": properties_or_empty(&function),
            "required": required_or_empty(&function),
        },
    })
}

/// v4 `convertToGoogleFormat`. Emits
/// `{ name, description, parameters: { type, properties, required } }`.
pub fn convert_to_google_format(tool: &Value) -> Value {
    let function = tool.get("function").cloned().unwrap_or(Value::Null);
    json!({
        "name": name_value(&function),
        "description": description_or_empty(&function),
        "parameters": {
            "type": "object",
            "properties": properties_or_empty(&function),
            "required": required_or_empty(&function),
        },
    })
}
