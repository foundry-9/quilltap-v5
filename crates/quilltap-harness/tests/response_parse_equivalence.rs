//! P4.13 unit 4 — the recorded-body response-parse differential (dogfood
//! finding #24's close-out: `model/response_parse.rs` previously had NO oracle
//! differential at all).
//!
//! The committed corpus
//! (`harness/oracle/fixtures/response-bodies/response-bodies.recorded.ndjson`,
//! regen `harness/oracle/providers/regenerate-response-bodies.sh`) was produced
//! by running each v4 plugin's REAL `sendMessage` with the network mocked
//! UNDERNEATH its SDK — so the SDK's own unwrap (the openai SDK's
//! `addOutputText`, the @openrouter/sdk's inbound zod rename, the genai SDK's
//! `.text` accessor) ran inside the oracle loop, and each line records the
//! exact `LLMResponse` v4 built from the body bytes. This test feeds the SAME
//! bodies to v5's `parse_for_provider` and diffs field-by-field (tier 1
//! exact).
//!
//! ⚠ Capture tiers: every current body is doc-derived (`synthetic: true`) —
//! the diff proves v4/v5 PARSE AGREEMENT on those bytes; the wire-shape trust
//! boundary stays marked until a family gains a real capture (the corpus
//! `_meta` note in the recorder header).
//!
//! Fields compared: `content`, `finishReason`, `usage`, `toolCalls`,
//! `reasoningContent`, `thoughtSignature`, `cacheUsage`, `raw`.
//! `attachmentResults` is request-side state (always `{sent:[],failed:[]}`
//! here) — excluded, as `NonStreamingResponse` deliberately does not model it.
//! GOOGLE's `raw.sdkHttpResponse` is excluded: the genai SDK attaches the live
//! HTTP headers there, so its contents depend on the transport, not the body —
//! unpinnable by a body corpus (and unused: tool detection reads `candidates`).

use std::fs;
use std::path::Path;

use quilltap_core::model::response_parse::{parse_for_provider_ex, NonStreamingResponse};
use serde_json::{json, Map, Value};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/response-bodies/response-bodies.recorded.ndjson")
}

/// The recorder's provider names → v5's canonical UPPERCASE ids.
fn provider_id(name: &str) -> &'static str {
    match name {
        "anthropic" => "ANTHROPIC",
        "deepseek" => "DEEPSEEK",
        "z-ai" => "Z_AI",
        "openrouter" => "OPENROUTER",
        "ollama" => "OLLAMA",
        "openai" => "OPENAI",
        "grok" => "GROK",
        "google" => "GOOGLE",
        "openai-compatible" => "OPENAI_COMPATIBLE",
        other => panic!("unknown corpus provider {other}"),
    }
}

/// Render v5's parse result in v4's `LLMResponse` JSON vocabulary (undefined
/// keys dropped, exactly as v4's own serialization boundary drops them).
fn to_v4_json(r: &NonStreamingResponse) -> Value {
    let mut m = Map::new();
    m.insert("content".into(), json!(r.content));
    m.insert(
        "finishReason".into(),
        match &r.finish_reason {
            Some(f) => json!(f),
            None => Value::Null,
        },
    );
    m.insert(
        "usage".into(),
        json!({
            "promptTokens": r.usage.prompt_tokens,
            "completionTokens": r.usage.completion_tokens,
            "totalTokens": r.usage.total_tokens,
        }),
    );
    m.insert("raw".into(), r.raw.clone());
    if !r.tool_calls.is_empty() {
        let calls: Vec<Value> = r
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": tc.type_,
                    "function": { "name": tc.name, "arguments": tc.arguments },
                })
            })
            .collect();
        m.insert("toolCalls".into(), Value::Array(calls));
    }
    if let Some(rc) = &r.reasoning_content {
        m.insert("reasoningContent".into(), json!(rc));
    }
    if let Some(ts) = &r.thought_signature {
        m.insert("thoughtSignature".into(), json!(ts));
    }
    if let Some(cu) = &r.cache_usage {
        let mut c = Map::new();
        if let Some(v) = cu.cached_tokens {
            c.insert("cachedTokens".into(), json!(v));
        }
        if let Some(v) = cu.cache_discount {
            c.insert("cacheDiscount".into(), json!(v));
        }
        if let Some(v) = cu.cache_creation_input_tokens {
            c.insert("cacheCreationInputTokens".into(), json!(v));
        }
        if let Some(v) = cu.cache_read_input_tokens {
            c.insert("cacheReadInputTokens".into(), json!(v));
        }
        m.insert("cacheUsage".into(), Value::Object(c));
    }
    Value::Object(m)
}

/// Deep-sort object keys so structural comparison ignores insertion order
/// (v4's key order varies per provider construction site; the diff is
/// field-by-field, not byte order).
fn canon(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = Map::new();
            for k in keys {
                m.insert(k.clone(), canon(&o[k]));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
        _ => v.clone(),
    }
}

#[test]
fn response_parse_matches_v4_recorded_llm_responses() {
    let path = fixture_path();
    let text = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} (regenerate-response-bodies.sh)",
            path.display()
        )
    });

    let mut cases = 0;
    let mut failures: Vec<String> = Vec::new();
    // Coverage guard: every provider family must appear — a family silently
    // missing from the corpus is how a parse gap hides (#24's lesson).
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    // P4.D78 — the two Ollama reasoning channels plus the conditional attach.
    let (mut ollama_native_reasoning, mut ollama_inline_reasoning, mut ollama_no_reasoning) =
        (false, false, false);

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let rec: Value = serde_json::from_str(line).expect("corpus line parses");
        let provider = provider_id(rec["provider"].as_str().expect("provider"));
        let case = rec["case"].as_str().expect("case");
        seen.insert(provider.to_string());

        if !rec["error"].is_null() {
            panic!(
                "{provider}/{case}: the oracle recorded a sendMessage FAILURE ({}) — \
                 fix the corpus body or record the refusal deliberately",
                rec["error"]
            );
        }
        let body: Value =
            serde_json::from_str(rec["body"].as_str().expect("body string")).expect("body JSON");
        // v4 bug 31: an OpenRouter case driven WITH an image records the
        // raw-wire vision LLMResponse (sendViaChatCompletions), so the parse
        // must use the vision flavor — the recorder marks it `visionPath`.
        let openrouter_vision = rec
            .get("visionPath")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut want = rec["response"].clone();
        let mut got = to_v4_json(&parse_for_provider_ex(provider, &body, openrouter_vision));

        // attachmentResults is request-side (see the module note).
        want.as_object_mut().unwrap().remove("attachmentResults");
        // GOOGLE: the genai SDK attaches the live HTTP headers as
        // raw.sdkHttpResponse — a transport artifact, not a body field.
        if provider == "GOOGLE" {
            if let Some(raw) = want.get_mut("raw").and_then(Value::as_object_mut) {
                raw.remove("sdkHttpResponse");
            }
            if let Some(raw) = got.get_mut("raw").and_then(Value::as_object_mut) {
                raw.remove("sdkHttpResponse");
            }
        }

        if provider == "OLLAMA" {
            let recorded_reasoning = rec["response"]
                .get("reasoningContent")
                .and_then(Value::as_str);
            let native = body
                .get("message")
                .and_then(|m| m.get("thinking"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            match recorded_reasoning {
                None => ollama_no_reasoning = true,
                Some(_) if !native.is_empty() => ollama_native_reasoning = true,
                Some(_) => ollama_inline_reasoning = true,
            }
        }

        cases += 1;
        let (g, w) = (canon(&got), canon(&want));
        if g != w {
            failures.push(format!("{provider}/{case}:\n  rust:   {g}\n  oracle: {w}"));
        }
    }

    for family in [
        "ANTHROPIC",
        "DEEPSEEK",
        "Z_AI",
        "OPENROUTER",
        "OLLAMA",
        "OPENAI",
        "GROK",
        "GOOGLE",
        "OPENAI_COMPATIBLE",
    ] {
        assert!(
            seen.contains(family),
            "corpus is missing the {family} family entirely"
        );
    }
    assert!(cases >= 36, "corpus shrank: {cases} cases (expected >= 36)");
    // P4.D78 — the Ollama reasoning channels. Both must be exercised, and at
    // least one row must still record NO reasoningContent (v4's conditional
    // attach): a corpus that lost either arm would pass with the feature
    // untested. Asserted over v4's RECORDED response, not v5's parse.
    assert!(
        ollama_native_reasoning,
        "corpus lost the ollama native `message.thinking` reasoning row"
    );
    assert!(
        ollama_inline_reasoning,
        "corpus lost the ollama inline `<think>` reasoning row"
    );
    assert!(
        ollama_no_reasoning,
        "corpus lost the ollama row with NO reasoningContent (the conditional attach)"
    );
    assert!(
        failures.is_empty(),
        "{} of {cases} response-parse cases diverge from v4:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
