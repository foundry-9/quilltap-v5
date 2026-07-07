//! Port of v4's `lib/llm/cache-prefix-hashes.ts` — the per-tier SHA-256 hashes of
//! the cacheable regions of an LLM request (the two/three system blocks, the
//! tools array, and the append-only history tail). `llm-logging` stamps these
//! onto each `llm_logs` row so a drift in the byte-stable prefix (which would
//! cause a provider cache miss) is visible after the fact.
//!
//! ## The `stableStringify` seam (⚠️ SORTS keys — unlike every other serializer)
//!
//! v4's `stableStringify` **sorts object keys** (`Object.keys(v).sort()`) before
//! emitting, arrays in order. This is deliberately deterministic-regardless-of-
//! insertion-order, the OPPOSITE of the insertion-order `JSON.stringify` idiom
//! every other ported serializer in this port reproduces (the typed-struct-in-
//! schema-order rule for JSON columns). [`stable_stringify`] here is the sorted
//! variant and must never be confused with those — it is private to this module.
//!
//! Key-sort fidelity: JS `Array.prototype.sort()` orders by UTF-16 code units;
//! Rust `str` `Ord` orders by Unicode scalar value. These agree for every BMP
//! code point (code unit == code point) and diverge only for astral-plane keys
//! (surrogate pairs `0xD800..=0xDFFF` sort below `0xE000..` in UTF-16 but above
//! in scalar order). The hashed objects here carry only ASCII field names
//! (`role`/`content`/`name`/`toolCallId`/`toolCalls` and whatever keys live
//! inside tool/content payloads), so the corpus stays in the agreeing range; an
//! astral key would be a documented seam.
//!
//! ## The `undefined` quirk in the history-tail mapping
//!
//! v4 maps each frozen message to `{ role, content, name, toolCallId, toolCalls }`
//! — an object where the last three keys are frequently `undefined`. Because the
//! keys were assigned (even to `undefined`), `Object.keys` still lists them, and
//! `stableStringify(undefined)` returns the JS value `undefined`, which the
//! surrounding string concatenation coerces to the literal text `undefined`. So
//! an absent `name` renders as `"name":undefined` in the stabilized string (NOT
//! omitted, NOT `null`). [`PrefixMessage`] models each of the three optional
//! fields as `Option<Value>` and [`stable_stringify_frozen`] emits `undefined`
//! for `None`, reproducing this byte-for-byte.

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::db::llm_logs::LlmLogRequestHashes;

const HASH_LENGTH: usize = 16;

/// v4 `hashString`: SHA-256 of the UTF-8 bytes, hex-encoded, truncated to the
/// FIRST 16 hex characters.
fn hash_string(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = hex::encode(digest);
    hex[..HASH_LENGTH].to_string()
}

/// v4 `stableStringify`: like `JSON.stringify` but with object keys **sorted**.
/// Base case (null / bool / number / string) delegates to `serde_json::to_string`
/// — byte-identical to `JSON.stringify` over that value space (string escaping,
/// number rendering; the corpus avoids integer-valued floats, which would render
/// `3.0` here vs `3` in JS, and lone surrogates, which `str` cannot hold).
fn stable_stringify(value: &Value) -> String {
    match value {
        Value::Array(arr) => {
            let inner: Vec<String> = arr.iter().map(stable_stringify).collect();
            format!("[{}]", inner.join(","))
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .iter()
                .map(|k| {
                    let key = serde_json::to_string(k).expect("string key serializes");
                    format!("{}:{}", key, stable_stringify(&map[*k]))
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        // null / bool / number / string — v4's `typeof value !== 'object'` branch.
        other => serde_json::to_string(other).expect("scalar serializes"),
    }
}

/// One request message, as consumed by [`compute_request_prefix_hashes`]. `role`
/// and `content` are always present (`content` is a string for a system block, or
/// an arbitrary value for the history tail); `name` / `tool_call_id` /
/// `tool_calls` are `Option<Value>` where `None` reproduces v4's `undefined`
/// (rendered as the literal `undefined` in the history-tail hash — see the module
/// docs).
///
/// Deserialization note: `name`/`toolCallId`/`toolCalls` use `#[serde(default)]`,
/// so an ABSENT key → `None` → renders `undefined`. serde maps an explicit JSON
/// `null` to `None` too — but v4's `LLMMessage` never produces `null` for these
/// (`string`/array `| undefined` only), so the corpus stays free of explicit
/// nulls and the absent==undefined mapping is exact.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefixMessage {
    pub role: String,
    pub content: Value,
    #[serde(default)]
    pub name: Option<Value>,
    #[serde(default)]
    pub tool_call_id: Option<Value>,
    #[serde(default)]
    pub tool_calls: Option<Value>,
}

/// Stabilize one frozen history message: the fixed 5-key object v4 builds
/// (`{ role, content, name, toolCallId, toolCalls }`), keys emitted in sorted
/// order (`content` < `name` < `role` < `toolCallId` < `toolCalls`), with a
/// `None` optional rendered as the literal `undefined`.
fn stable_stringify_frozen(msg: &PrefixMessage) -> String {
    let field = |v: &Option<Value>| match v {
        Some(val) => stable_stringify(val),
        None => "undefined".to_string(),
    };
    format!(
        "{{\"content\":{},\"name\":{},\"role\":{},\"toolCallId\":{},\"toolCalls\":{}}}",
        stable_stringify(&msg.content),
        field(&msg.name),
        stable_stringify(&Value::String(msg.role.clone())),
        field(&msg.tool_call_id),
        field(&msg.tool_calls),
    )
}

/// v4 `computeRequestPrefixHashes`: per-tier SHA-256 (first 16 hex) of the
/// cacheable request regions.
///
///   - `systemBlock{1,2,3}Hash`: the first three system messages whose `content`
///     is a string (hashed directly).
///   - `toolsArrayHash`: the stabilized tools array, only when non-empty.
///   - `historyTailHash`: the stabilized non-system messages minus the last one
///     (the frozen history), only when more than one non-system message exists.
pub fn compute_request_prefix_hashes(
    messages: &[PrefixMessage],
    tools: Option<&[Value]>,
) -> LlmLogRequestHashes {
    let mut result = LlmLogRequestHashes {
        system_block1_hash: None,
        system_block2_hash: None,
        system_block3_hash: None,
        tools_array_hash: None,
        history_tail_hash: None,
    };

    // System blocks: role == 'system' AND content is a string.
    let system_blocks: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == "system")
        .filter_map(|m| m.content.as_str())
        .collect();
    if let Some(s) = system_blocks.first() {
        result.system_block1_hash = Some(hash_string(s));
    }
    if let Some(s) = system_blocks.get(1) {
        result.system_block2_hash = Some(hash_string(s));
    }
    if let Some(s) = system_blocks.get(2) {
        result.system_block3_hash = Some(hash_string(s));
    }

    if let Some(tools) = tools {
        if !tools.is_empty() {
            let arr: Vec<String> = tools.iter().map(stable_stringify).collect();
            result.tools_array_hash = Some(hash_string(&format!("[{}]", arr.join(","))));
        }
    }

    let non_system: Vec<&PrefixMessage> = messages.iter().filter(|m| m.role != "system").collect();
    if non_system.len() > 1 {
        let frozen = &non_system[..non_system.len() - 1];
        let inner: Vec<String> = frozen.iter().map(|m| stable_stringify_frozen(m)).collect();
        result.history_tail_hash = Some(hash_string(&format!("[{}]", inner.join(","))));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sys(content: &str) -> PrefixMessage {
        PrefixMessage {
            role: "system".to_string(),
            content: json!(content),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }
    fn user(content: &str) -> PrefixMessage {
        PrefixMessage {
            role: "user".to_string(),
            content: json!(content),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    #[test]
    fn stable_stringify_sorts_object_keys() {
        // Insertion order b,a,c -> sorted a,b,c.
        let v = json!({ "b": 1, "a": 2, "c": 3 });
        assert_eq!(stable_stringify(&v), r#"{"a":2,"b":1,"c":3}"#);
    }

    #[test]
    fn frozen_undefined_fields_render_literally() {
        let m = user("hello");
        // content,name,role,toolCallId,toolCalls with the three optionals undefined.
        assert_eq!(
            stable_stringify_frozen(&m),
            r#"{"content":"hello","name":undefined,"role":"user","toolCallId":undefined,"toolCalls":undefined}"#
        );
    }

    #[test]
    fn system_blocks_and_tail() {
        let msgs = vec![sys("A"), sys("B"), user("u1"), user("u2")];
        let out = compute_request_prefix_hashes(&msgs, None);
        assert!(out.system_block1_hash.is_some());
        assert!(out.system_block2_hash.is_some());
        assert!(out.system_block3_hash.is_none());
        // Two non-system -> tail over the first (frozen = [u1]).
        assert!(out.history_tail_hash.is_some());
        // Distinct block contents -> distinct hashes.
        assert_ne!(out.system_block1_hash, out.system_block2_hash);
    }

    #[test]
    fn single_non_system_has_no_tail() {
        let msgs = vec![sys("A"), user("only")];
        let out = compute_request_prefix_hashes(&msgs, None);
        assert!(out.history_tail_hash.is_none());
    }

    #[test]
    fn empty_tools_no_hash() {
        let msgs = vec![user("x"), user("y")];
        assert!(compute_request_prefix_hashes(&msgs, Some(&[]))
            .tools_array_hash
            .is_none());
        assert!(
            compute_request_prefix_hashes(&msgs, Some(&[json!({"t":1})]))
                .tools_array_hash
                .is_some()
        );
    }
}
