//! Differential: the provider tool-wire (wave 4 / W4.7c).
//!
//! For every committed corpus row, this recomputes the Rust `model::tool_wire`
//! result and diffs it byte-for-byte (compact JSON, key-order sensitive) against
//! the NDJSON RECORDED by driving each v4 provider plugin's REAL tool-wire method
//! (`record-tool-wire.mjs`). Each recorded line carries `{ kind, provider, case,
//! input, result }`, so the Rust side reads the SAME input, recomputes, and diffs.
//!
//! Coverage spans the three `formatTools`/`parseToolCalls` `toolFormat` branches
//! (anthropic / openai / google) + deepseek (an OpenAI-format provider over a
//! recorded escaped-quote rawResponse). The per-provider DISPATCH for every other
//! provider is proven by `provider_registry_equivalence` (the `toolFormat` rows);
//! `dispatch_is_uniform_within_tool_format` below re-confirms the Rust side keys
//! purely on `toolFormat`.
//!
//! The corpus is committed
//! (`harness/oracle/fixtures/tool-wire/tool-wire.recorded.ndjson`); no env var
//! is needed to run — the family runs in every plain `cargo test`. Regenerate
//! the corpus (Node 24, only after a v4 provider drift) with
//! `harness/oracle/providers/regenerate-tool-wire.sh`.

use quilltap_core::model::tool_wire::{
    detect_native_tool_calls, format_tools_for_provider, provider_has_text_markers,
    provider_has_text_tool_markers, provider_parse_text_tool_calls,
    provider_strip_text_tool_markers, ToolCallRequest,
};
use quilltap_core::provider_manifest::Registry;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/tool-wire/tool-wire.recorded.ndjson")
}

/// The oracle records the lowercase plugin name (`anthropic`); the manifest
/// registry keys by the canonical UPPERCASE id (`ANTHROPIC`, `Z_AI`) — the value
/// production passes (`connectionProfile.provider`).
fn registry_id(plugin_name: &str) -> String {
    plugin_name.to_uppercase().replace('-', "_")
}

/// Serialize a `ToolCallRequest` list to the v4 `ToolCallRequest[]` JSON shape:
/// `{ name, arguments, callId? }` (callId omitted when `None`, matching
/// `JSON.stringify`'s drop of an undefined field), in field order.
fn tool_calls_to_value(calls: &[ToolCallRequest]) -> Value {
    Value::Array(
        calls
            .iter()
            .map(|c| {
                let mut m = Map::new();
                m.insert("name".to_string(), Value::String(c.name.clone()));
                m.insert("arguments".to_string(), c.arguments.clone());
                if let Some(id) = &c.call_id {
                    m.insert("callId".to_string(), Value::String(id.clone()));
                }
                Value::Object(m)
            })
            .collect(),
    )
}

/// Byte-exact JSON comparison (compact, key-order sensitive under preserve_order).
fn assert_json_eq(kind: &str, provider: &str, case: &str, got: &Value, want: &Value) {
    let got_s = serde_json::to_string(got).unwrap();
    let want_s = serde_json::to_string(want).unwrap();
    assert_eq!(
        got_s, want_s,
        "\n{kind} / {provider} / {case} diverged\n  got:  {got_s}\n  want: {want_s}\n"
    );
}

#[test]
fn tool_wire_matches_v4() {
    let registry = Registry::built_in();
    let text = std::fs::read_to_string(corpus_path())
        .expect("committed tool-wire oracle NDJSON is present");

    let mut seen_kinds = std::collections::HashSet::new();
    let mut rows = 0usize;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).unwrap();
        let kind = row["kind"].as_str().unwrap();
        let plugin_name = row["provider"].as_str().unwrap();
        let provider = registry_id(plugin_name);
        let provider = provider.as_str();
        let case = row["case"].as_str().unwrap();
        let input = &row["input"];
        let want = &row["result"];
        seen_kinds.insert(kind.to_string());
        rows += 1;

        match kind {
            "formatTools" => {
                let catalog: Vec<Value> = input.as_array().unwrap().clone();
                let got = Value::Array(format_tools_for_provider(registry, provider, &catalog));
                assert_json_eq(kind, provider, case, &got, want);
            }
            "parseToolCalls" => {
                let calls = detect_native_tool_calls(registry, input, provider);
                let got = tool_calls_to_value(&calls);
                assert_json_eq(kind, provider, case, &got, want);
            }
            "hasMarkers" => {
                let got =
                    provider_has_text_tool_markers(registry, provider, input.as_str().unwrap());
                assert_eq!(
                    Value::Bool(got),
                    *want,
                    "hasMarkers / {provider} / {case} diverged"
                );
            }
            "parseText" => {
                let calls =
                    provider_parse_text_tool_calls(registry, provider, input.as_str().unwrap());
                let got = tool_calls_to_value(&calls);
                assert_json_eq(kind, provider, case, &got, want);
            }
            "stripText" => {
                let got =
                    provider_strip_text_tool_markers(registry, provider, input.as_str().unwrap());
                assert_eq!(
                    Value::String(got),
                    *want,
                    "stripText / {provider} / {case} diverged"
                );
            }
            other => panic!("unknown oracle kind: {other}"),
        }
    }

    assert!(rows > 100, "expected a substantial corpus, got {rows} rows");
    for k in [
        "formatTools",
        "parseToolCalls",
        "hasMarkers",
        "parseText",
        "stripText",
    ] {
        assert!(seen_kinds.contains(k), "corpus missing kind {k}");
    }
}

/// The Rust dispatch keys purely on the manifest `toolFormat`, so every provider
/// that shares a `toolFormat` produces an identical reshape/parse — confirming
/// that driving one provider per branch in the oracle covers all nine.
#[test]
fn dispatch_is_uniform_within_tool_format() {
    let registry = Registry::built_in();

    // OpenAI-format providers all reshape/parse identically to OPENAI.
    let catalog = vec![serde_json::json!({
        "type": "function",
        "function": { "name": "t", "description": "d", "parameters": { "type": "object", "properties": {}, "required": [] } }
    })];
    let openai = format_tools_for_provider(registry, "OPENAI", &catalog);
    for p in [
        "DEEPSEEK",
        "Z_AI",
        "OPENROUTER",
        "OLLAMA",
        "GROK",
        "OPENAI_COMPATIBLE",
    ] {
        assert_eq!(
            format_tools_for_provider(registry, p, &catalog),
            openai,
            "{p} formatTools should match OPENAI"
        );
    }

    // ANTHROPIC and GOOGLE are the only non-openai reshapes.
    let anth = format_tools_for_provider(registry, "ANTHROPIC", &catalog);
    let goog = format_tools_for_provider(registry, "GOOGLE", &catalog);
    assert!(anth[0].get("input_schema").is_some());
    assert!(goog[0].get("parameters").is_some());
    assert_ne!(anth, openai);
    assert_ne!(goog, openai);

    // Native parse dispatch: anthropic content vs openai tool_calls vs google parts.
    let resp = serde_json::json!({ "content": [{ "type": "tool_use", "id": "x", "name": "n", "input": {} }] });
    assert_eq!(
        detect_native_tool_calls(registry, &resp, "ANTHROPIC").len(),
        1
    );
    assert_eq!(detect_native_tool_calls(registry, &resp, "OPENAI").len(), 0);

    // Unknown provider → empty native parse; the text-markers pass is gated OFF
    // (v4's `if (providerPlugin?.hasTextToolMarkers...)` skip).
    assert!(detect_native_tool_calls(registry, &resp, "NOPE").is_empty());
    assert!(!provider_has_text_markers(registry, "NOPE"));
    assert!(provider_has_text_markers(registry, "ANTHROPIC"));

    // GOOGLE text markers use tool_use only; ANTHROPIC uses the composite.
    let fc = "<function_calls><invoke name=\"search\"></invoke></function_calls>";
    assert!(provider_has_text_tool_markers(registry, "ANTHROPIC", fc));
    assert!(!provider_has_text_tool_markers(registry, "GOOGLE", fc));
}
