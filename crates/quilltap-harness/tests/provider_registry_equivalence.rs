//! Tier-1 differential (wave 4 / W4.7a): the provider-registry convenience
//! getters. The v4 oracle drives the REAL `lib/plugins/provider-registry` getters
//! over every registered provider (registry initialized the runtime way); the
//! Rust side answers from the generated built-in manifests
//! (`quilltap_core::provider_manifest::Registry::built_in`). Diffed field-by-field,
//! exact — including defaults for absent fields, the legacy-name lookups (v4 does
//! NOT resolve them), and a determinism dump.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/provider-registry.ts \
//!     > /tmp/oracle-provider-registry.ndjson
//! Run:
//!   QT_ORACLE_PROVIDER_REGISTRY=/tmp/oracle-provider-registry.ndjson \
//!     cargo test -p quilltap-harness provider_registry
//!
//! This is also the drift-catch point for the four seam-input replacements: the
//! registry values these getters expose are the SAME values the leaf consumers
//! (`model_context` registry default, `cheap_model`/`cheap_llm`
//! recommended/default, `tool_build` web-search, `message_formatter` name-field)
//! now source from the registry — so if a manifest drifts from v4, this
//! differential catches it, not the leaf differentials silently.

use quilltap_core::provider_manifest::{Capability, Registry, ToolFormat};
use serde_json::{json, Map, Value};

/// Drop null-valued keys recursively so a v4 object that OMITS an undefined field
/// (e.g. `attachmentSupport.maxFiles`, `messageFormat.maxNameLength`) compares
/// equal to the Rust side, which carries the field as `null`. Numbers/strings/
/// bools/arrays pass through; arrays are element-normalized.
fn strip_nulls(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, val) in map {
                if val.is_null() {
                    continue;
                }
                out.insert(k.clone(), strip_nulls(val));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(strip_nulls).collect()),
        _ => v.clone(),
    }
}

fn cap_from_str(s: &str) -> Capability {
    match s {
        "chat" => Capability::Chat,
        "imageGeneration" => Capability::ImageGeneration,
        "embeddings" => Capability::Embeddings,
        "webSearch" => Capability::WebSearch,
        "toolUse" => Capability::ToolUse,
        other => panic!("unknown capability {other}"),
    }
}

fn tool_format_str(tf: ToolFormat) -> &'static str {
    match tf {
        ToolFormat::Openai => "openai",
        ToolFormat::Anthropic => "anthropic",
        ToolFormat::Google => "google",
    }
}

/// Build the Rust registry's answer for a getter row as a `Value` shaped like
/// v4's, ready for a strip-nulls diff against `out`.
fn rust_answer(reg: &Registry, row: &Map<String, Value>) -> Value {
    let kind = row["kind"].as_str().unwrap();
    let id = row.get("id").and_then(Value::as_str).unwrap_or("");

    match kind {
        "names" => {
            json!({ "names": reg.provider_names() })
        }
        "determinism" => {
            json!({ "pass": row["pass"], "names": reg.provider_names() })
        }
        "getProvider" => {
            json!({ "id": id, "found": reg.get_provider(id).is_some() })
        }
        "hasProvider" => {
            json!({ "id": id, "has": reg.has_provider(id) })
        }
        "supportsCapability" => {
            let cap = row["capability"].as_str().unwrap();
            json!({
                "id": id,
                "capability": cap,
                "out": reg.supports_capability(id, cap_from_str(cap)),
            })
        }
        "attachmentSupport" => {
            let out = reg.attachment_support(id).map(|a| {
                json!({
                    "supportsAttachments": a.supports_attachments,
                    "supportedMimeTypes": a.supported_mime_types,
                    "description": a.description,
                    "notes": a.notes,
                    "maxFileSize": a.max_file_size,
                    "maxBase64Size": a.max_base64_size,
                    "maxFiles": a.max_files,
                })
            });
            json!({ "id": id, "out": out })
        }
        "messageFormat" => {
            let mf = reg.message_format(id);
            let roles: Vec<&str> = mf
                .supported_roles
                .iter()
                .map(|r| match r {
                    quilltap_core::provider_manifest::MessageRole::User => "user",
                    quilltap_core::provider_manifest::MessageRole::Assistant => "assistant",
                })
                .collect();
            json!({
                "id": id,
                "out": {
                    "supportsNameField": mf.supports_name_field,
                    "supportedRoles": roles,
                    "maxNameLength": mf.max_name_length,
                },
            })
        }
        "charsPerToken" => {
            json!({ "id": id, "out": reg.chars_per_token(id) })
        }
        "toolFormat" => {
            json!({ "id": id, "out": tool_format_str(reg.tool_format(id)) })
        }
        "cheapModelConfig" => {
            let out = reg.cheap_model_config(id).map(|c| {
                json!({
                    "defaultModel": c.default_model,
                    "recommendedModels": c.recommended_models,
                })
            });
            json!({ "id": id, "out": out })
        }
        "defaultContextWindow" => {
            json!({ "id": id, "out": reg.default_context_window(id) })
        }
        "legacyNames" => {
            let out = reg
                .get_provider(id)
                .map(|p| p.legacy_names.clone())
                .unwrap_or_default();
            json!({ "id": id, "out": out })
        }
        "modelPricing" => {
            let model_id = row["modelId"].as_str().unwrap();
            let out = reg
                .model_pricing(id, model_id)
                .map(|p| json!({ "input": p.input, "output": p.output }));
            json!({ "id": id, "modelId": model_id, "out": out })
        }
        other => panic!("unhandled oracle row kind {other}"),
    }
}

#[test]
fn provider_registry_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PROVIDER_REGISTRY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PROVIDER_REGISTRY to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let reg = Registry::built_in();
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        let row = v.as_object().expect("oracle row is an object");
        let kind = row["kind"].as_str().unwrap();

        // The v4 row minus its `kind` tag is the expected shape.
        let mut expected = row.clone();
        expected.remove("kind");
        let expected = Value::Object(expected);

        let got = rust_answer(reg, row);

        assert_eq!(
            strip_nulls(&got),
            strip_nulls(&expected),
            "provider-registry divergence on kind={kind}\n  row={line}"
        );
        count += 1;
    }

    assert!(count > 200, "oracle file had too few rows: {count}");
    eprintln!("OK: provider-registry matched oracle ({count} rows).");
}
