//! Simple-JSON prompt builder — port of v4 `lib/tools/simple-json-prompt.ts`.
//!
//! Renders one function-signature line per enabled tool, with signatures derived
//! from each tool definition's `parameters` JSON Schema (the b.3 catalog). Pair
//! the prompt with `stop: ['</tool_call>']` at the streaming layer.

use std::collections::HashSet;

use serde_json::Value;

use crate::jsstr::js_trim;
use crate::tools::definitions::definition_by_key;

/// Which tools to document. v4 `SimpleJsonPromptOptions`. `create_note` is part
/// of the interface but has NO rendered entry in the builder (faithful to v4).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SimpleJsonPromptOptions {
    pub whisper: Option<bool>,
    pub search: Option<bool>,
    pub image_generation: Option<bool>,
    pub web_search: Option<bool>,
    pub state: Option<bool>,
    pub rng: Option<bool>,
    pub project_info: Option<bool>,
    pub help_search: Option<bool>,
    pub help_settings: Option<bool>,
    pub help_navigate: Option<bool>,
    pub create_note: Option<bool>,
    pub wardrobe_list: Option<bool>,
    pub wardrobe_read: Option<bool>,
    pub wardrobe_wear: Option<bool>,
    pub wardrobe_take_off: Option<bool>,
    pub wardrobe_create: Option<bool>,
    pub wardrobe_update: Option<bool>,
    pub wardrobe_archive: Option<bool>,
}

fn on(v: Option<bool>) -> bool {
    v.unwrap_or(false)
}

/// JS `String(v)` for a JSON scalar (enum / type-union member).
fn js_string_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if n.as_i64().is_none() && n.as_u64().is_none() && f.is_finite() && f.fract() == 0.0
                {
                    return (f as i64).to_string();
                }
            }
            n.to_string()
        }
        other => other.to_string(),
    }
}

/// v4 `renderTypeHint`.
fn render_type_hint(prop: &Value) -> String {
    if let Some(en) = prop.get("enum").and_then(Value::as_array) {
        if !en.is_empty() {
            return en
                .iter()
                .map(|v| match v {
                    Value::String(s) => format!("\"{s}\""),
                    other => js_string_of(other),
                })
                .collect::<Vec<_>>()
                .join(" | ");
        }
    }

    if let Some(variants) = prop
        .get("oneOf")
        .or_else(|| prop.get("anyOf"))
        .and_then(Value::as_array)
    {
        if !variants.is_empty() {
            return variants
                .iter()
                .map(render_type_hint)
                .collect::<Vec<_>>()
                .join(" | ");
        }
    }

    if let Some(types) = prop.get("type").and_then(Value::as_array) {
        return types
            .iter()
            .map(js_string_of)
            .collect::<Vec<_>>()
            .join(" | ");
    }

    match prop.get("type").and_then(Value::as_str) {
        Some("string") => "string".to_string(),
        Some("number") | Some("integer") => "number".to_string(),
        Some("boolean") => "boolean".to_string(),
        Some("array") => {
            let item = prop
                .get("items")
                .map(render_type_hint)
                .unwrap_or_else(|| "unknown".to_string());
            format!("{item}[]")
        }
        Some("object") => "object".to_string(),
        Some("null") => "null".to_string(),
        _ => "unknown".to_string(),
    }
}

/// v4 `describeToolSignature`. `def` is the `{name, description, parameters}`
/// value (the catalog shape; equivalent to v4's `tool.function`).
pub fn describe_tool_signature(def: &Value) -> String {
    let name = def.get("name").and_then(Value::as_str).unwrap_or_default();
    let params = def.get("parameters");
    let properties = params
        .and_then(|p| p.get("properties"))
        .and_then(Value::as_object);

    let Some(props) = properties else {
        return format!("{name}()");
    };

    let required: HashSet<&str> = params
        .and_then(|p| p.get("required"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let parts: Vec<String> = props
        .iter()
        .map(|(pname, prop)| {
            let optional = if required.contains(pname.as_str()) {
                ""
            } else {
                "?"
            };
            format!("{pname}{optional}: {}", render_type_hint(prop))
        })
        .collect();

    format!("{name}({})", parts.join(", "))
}

/// v4 `renderToolEntry`.
fn render_tool_entry(def: &Value) -> String {
    let signature = describe_tool_signature(def);
    let description = def
        .get("description")
        .and_then(Value::as_str)
        .and_then(|d| d.split('\n').next())
        .map(js_trim)
        .unwrap_or("");
    if description.is_empty() {
        format!("- {signature}")
    } else {
        format!("- {signature}: {description}")
    }
}

fn entry_for(key: &str) -> String {
    let def = definition_by_key(key).unwrap_or_else(|| panic!("catalog missing key {key}"));
    render_tool_entry(&def)
}

const PREFIX: &str = "\n## Available tools\n\nYou may call any of the following tools when it would genuinely help you respond. Calls are framed as JSON inside a `<tool_call>` XML tag:\n\n";

const SUFFIX: &str = "\n\n### How to use a tool\n\nWhen you want to call a tool, write a single line of prose introducing it (optional), then emit exactly ONE `<tool_call>` block in this exact shape and stop:\n\n<tool_call>\n{\"name\": \"<tool_name>\", \"arguments\": {\"<param>\": \"<value>\"}}\n</tool_call>\n\nRules:\n- Use the canonical tool name from the list above (e.g. `\"search\"`, not `\"SEARCH\"`).\n- The `arguments` object holds the named parameters. Omit optional ones you don't need.\n- Emit at most ONE `<tool_call>` per turn. Stop immediately after the closing `</tool_call>` tag — do not narrate fictional results.\n- After you stop, the system will execute the tool and reply with a `<tool_result name=\"...\">` block. Read it, then either respond to the user or make another tool call.\n- Only call a tool when it would genuinely improve your answer. Don't decorate the response with tool calls you don't need.\n";

/// v4 `buildSimpleJsonToolInstructions`.
pub fn build_simple_json_tool_instructions(options: &SimpleJsonPromptOptions) -> String {
    let mut entries: Vec<String> = Vec::new();

    if on(options.whisper) {
        entries.push(entry_for("whisper"));
    }
    // `search !== false` — on by default.
    if options.search != Some(false) {
        entries.push(entry_for("searchScriptorium"));
    }
    if on(options.image_generation) {
        entries.push(entry_for("imageGeneration"));
    }
    if on(options.web_search) {
        entries.push(entry_for("webSearch"));
    }
    if on(options.state) {
        entries.push(entry_for("state"));
    }
    if on(options.rng) {
        entries.push(entry_for("rng"));
    }
    if on(options.project_info) {
        entries.push(entry_for("projectInfo"));
    }
    if on(options.help_search) {
        entries.push(entry_for("helpSearch"));
    }
    if on(options.help_settings) {
        entries.push(entry_for("helpSettings"));
    }
    if on(options.help_navigate) {
        entries.push(entry_for("helpNavigate"));
    }
    if on(options.wardrobe_list) {
        entries.push(entry_for("wardrobeList"));
    }
    if on(options.wardrobe_read) {
        entries.push(entry_for("wardrobeRead"));
    }
    if on(options.wardrobe_wear) {
        entries.push(entry_for("wardrobeWear"));
    }
    if on(options.wardrobe_take_off) {
        entries.push(entry_for("wardrobeTakeOff"));
    }
    if on(options.wardrobe_create) {
        entries.push(entry_for("wardrobeCreate"));
    }
    if on(options.wardrobe_update) {
        entries.push(entry_for("wardrobeUpdate"));
    }
    if on(options.wardrobe_archive) {
        entries.push(entry_for("wardrobeArchive"));
    }

    if entries.is_empty() {
        return String::new();
    }

    let full = format!("{}{}{}", PREFIX, entries.join("\n"), SUFFIX);
    js_trim(&full).to_string()
}
