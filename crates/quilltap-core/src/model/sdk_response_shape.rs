//! v4's THROW on a 2xx body its plugin cannot read (P4.141 — P4.128's
//! recorded "2xx body that fails to parse" approximation, measured wider than
//! recorded: a behaviour divergence for SEVEN providers' non-streaming send).
//!
//! v5's response parsers are TOTAL (`response_parse.rs` — a missing
//! `choices[0]` / `message` / `output` / `content` reads as empty), so before
//! P4.141 every posed 2xx JSON shape answered `Ok("")`. v4's plugins read the
//! SDK's parsed body with plain JavaScript property reads, and a read off
//! `undefined` / `null` THROWS — a `TypeError` that v4's failover classifier
//! files as `provider-error`, so the cheap path's stand-in chain tries the next
//! route where v5 handed the task an empty result (a blank title, an empty
//! extraction). This module reproduces each plugin's read chain as a
//! JS-faithful property walk over the parsed body and answers v4's thrown
//! text at the FIRST throwing read; it runs BESIDE the parsers (which stay
//! total — `completion_provider.rs` consults it before parsing).
//!
//! Per v4 plugin (`f6426e196`; the bytes recorded through the REAL plugins by
//! `harness/oracle/providers/record-text-errors.mjs`, the `ok_*` cases):
//!
//! | provider | v4's reads, in order |
//! |---|---|
//! | OPENAI_COMPATIBLE | `response.choices[0]`, `choice.message.tool_calls` (plugin-utils 2.6.2 `providers/index.js:425-426`) |
//! | DEEPSEEK, Z_AI | `response.choices[0]`, `choice.message`, `msg.reasoning_content` (DeepSeek `provider.ts:258-260`, Z.AI `:350-353`) |
//! | NANOGPT | `response.choices[0]`, `choice.message`, `msg.reasoning` (`:313-320`) |
//! | OPENAI, GROK | the SDK's `'object' in rsp` (`openai` 7.23.0 `resources/responses/responses.js:86`), its `addOutputText` walk when `rsp.object === 'response'` (`lib/ResponsesParser.js:195-208`), then the plugin's `for (const item of response.output)` reading `item.type` (OpenAI `:440`, Grok `:239` / `:341`) |
//! | ANTHROPIC | `response.content.filter(block => block.type === 'text')` (`:508-510`) |
//! | GOOGLE, OLLAMA, OPENROUTER | none that throws on a JSON shape (Google WARNs `No candidates found in Google response` instead; Ollama and OpenRouter's raw `fetch` read with `?.`) |
//!
//! The body each chain starts from is what the SDK's parse hands the plugin:
//! the openai SDK answers `undefined` for an EMPTY JSON body
//! (`internal/parse.js:32-44` — `!bodyText`), so the chain reads off
//! `undefined`; every other client (`@anthropic-ai/sdk`, `@google/genai`, the
//! raw `response.json()`) runs `JSON.parse('')` and throws V8's `Unexpected end
//! of JSON input`. A body that is not JSON throws V8's `JSON.parse` text on
//! every provider ([`v8_json_parse_message`](crate::jsstr::v8_json_parse_message)).
//!
//! V8's read error is exactly `Cannot read properties of undefined (reading
//! '<k>')` / `… of null (…)` with an array index key as `'0'`; a `for…of` over
//! a non-iterable is `<source expression> is not iterable` — V8 renders the
//! expression's SOURCE text, so the two Responses sites differ (`rsp.output`
//! in the SDK, `response.output` in the plugins); `'object' in v` on a
//! non-object is `Cannot use 'in' operator to search for 'object' in
//! <String(v)>`. All measured on Node 24.13.1.
//!
//! Recorded divergences (the walk stops where they begin — each answers
//! whatever v5's total parser makes of the body):
//! - the inputs serde rejects and V8 accepts (a number past f64, a lone
//!   `\ud800` escape, nesting past serde's 128-level limit): v4's plugin
//!   reads the parsed body on, v5 answers serde's own text — the twin's whole
//!   `None` scope since P4.154 taught it every V8 failure template (a failure
//!   INSIDE a value that starts legally — `{"choices":[] "x":1}` — now reads
//!   V8's `Expected ',' or '}' after property value in JSON at position 14
//!   (line 1 column 15)`, the `ok_json_missing_comma` rows);
//! - the SDK's content-type branch: a 2xx `text/plain` body is handed to the
//!   plugin as a STRING (`openai/internal/parse.js:49-50`); the transport keeps
//!   no headers, so every 2xx is read as JSON (the `google_json_as_text_plain`
//!   class);
//! - shapes past a `null` / `undefined` read: a non-array `tool_calls`
//!   (`normalizeToolCalls` / DeepSeek's `.filter` — `… is not a function` / `…
//!   is not iterable` with V8's callee text), a `tool_calls` element that is
//!   null or a primitive (`'function' in tc`), the Responses items' `content`
//!   / `summary` arms past `item.type`, Anthropic's blocks past `block.type`;
//! - a 2xx Responses body carrying a truthy `error` (OpenAI / Grok throw
//!   `<Name> API error: ${response.error.message}` after logging `Responses
//!   API returned error`) — no corpus row poses it.

use serde_json::Value;

use crate::model::provider_io::ProviderKind;

/// A JavaScript value as a plugin's property read sees it: `undefined`, a
/// JSON value from the body, or a one-unit string a string index yields.
#[derive(Clone, Debug)]
enum Js<'a> {
    Undefined,
    Val(&'a Value),
    Str(String),
}

impl Js<'_> {
    /// `String(v)` — what the `'in'` operator's message renders.
    fn js_string(&self) -> String {
        match self {
            Js::Undefined => "undefined".to_string(),
            Js::Str(s) => s.clone(),
            Js::Val(v) => match v {
                Value::Null => "null".to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Number(_) => crate::pascal::js_value::json_stringify(v),
                Value::String(s) => s.clone(),
                Value::Array(_) | Value::Object(_) => "[object Object]".to_string(),
            },
        }
    }

    fn is_str(&self, want: &str) -> bool {
        match self {
            Js::Str(s) => s == want,
            Js::Val(Value::String(s)) => s == want,
            _ => false,
        }
    }
}

/// A canonical JS array index (`"0"`, `"12"` — never `"01"` or `"-1"`).
fn array_index(key: &str) -> Option<usize> {
    let canonical = !key.is_empty()
        && key.bytes().all(|b| b.is_ascii_digit())
        && (key == "0" || !key.starts_with('0'));
    canonical.then(|| key.parse().ok()).flatten()
}

/// A string's UTF-16 code units — JS indexing's view of it.
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `value[key]` — V8's `Cannot read properties of undefined|null (reading
/// '<key>')` off a nullish value, else the property (`undefined` when absent;
/// a number or boolean has none a JSON body can name).
fn get<'a>(value: &Js<'a>, key: &str) -> Result<Js<'a>, String> {
    // No chain reads `length`, so a string answers its index keys only.
    let string_get = |s: &str| -> Js<'a> {
        array_index(key)
            .and_then(|i| units(s).get(i).copied())
            .map(|c| Js::Str(String::from_utf16_lossy(&[c])))
            .unwrap_or(Js::Undefined)
    };
    Ok(match value {
        Js::Undefined => {
            return Err(format!(
                "Cannot read properties of undefined (reading '{key}')"
            ))
        }
        Js::Val(Value::Null) => {
            return Err(format!("Cannot read properties of null (reading '{key}')"))
        }
        Js::Str(s) => string_get(s),
        Js::Val(Value::String(s)) => string_get(s),
        Js::Val(Value::Object(m)) => m.get(key).map(Js::Val).unwrap_or(Js::Undefined),
        Js::Val(Value::Array(a)) => array_index(key)
            .and_then(|i| a.get(i))
            .map(Js::Val)
            .unwrap_or(Js::Undefined),
        Js::Val(Value::Bool(_) | Value::Number(_)) => Js::Undefined,
    })
}

/// A `for…of` over `value` (written `expr` at the v4 site): the elements of
/// an array, the units of a string, else V8's `<expr> is not iterable`.
fn iterate<'a>(value: &Js<'a>, expr: &str) -> Result<Vec<Js<'a>>, String> {
    let chars = |s: &str| -> Vec<Js<'a>> {
        units(s)
            .into_iter()
            .map(|c| Js::Str(String::from_utf16_lossy(&[c])))
            .collect()
    };
    match value {
        Js::Val(Value::Array(a)) => Ok(a.iter().map(Js::Val).collect()),
        Js::Val(Value::String(s)) => Ok(chars(s)),
        Js::Str(s) => Ok(chars(s)),
        _ => Err(format!("{expr} is not iterable")),
    }
}

/// `response.choices[0].message.<next>` — the three openai-SDK chat plugins'
/// chain (`<next>` = `tool_calls` for the OpenAI-compatible base,
/// `reasoning_content` for DeepSeek / Z.AI, `reasoning` for NanoGPT).
fn chat_completions_chain(body: &Js<'_>, next: &str) -> Result<(), String> {
    let choice = get(&get(body, "choices")?, "0")?;
    let message = get(&choice, "message")?;
    get(&message, next)?;
    Ok(())
}

/// The Responses API chain (OPENAI / GROK): the SDK's `'object' in rsp` +
/// `addOutputText`, then the plugin's loop over `response.output`.
fn responses_chain(body: &Js<'_>) -> Result<(), String> {
    let is_object = matches!(body, Js::Val(Value::Object(_) | Value::Array(_)));
    if !is_object {
        return Err(format!(
            "Cannot use 'in' operator to search for 'object' in {}",
            body.js_string()
        ));
    }
    let object = get(body, "object")?;
    if object.is_str("response") {
        // `addOutputText(rsp)`: every output's `type`, and a `message`'s
        // `content` items' `type`.
        for output in iterate(&get(body, "output")?, "rsp.output")? {
            let ty = get(&output, "type")?;
            if !ty.is_str("message") {
                continue;
            }
            for content in iterate(&get(&output, "content")?, "output.content")? {
                get(&content, "type")?;
            }
        }
    }
    for item in iterate(&get(body, "output")?, "response.output")? {
        get(&item, "type")?;
    }
    Ok(())
}

/// `response.content.filter(block => block.type === 'text')` (ANTHROPIC),
/// then `rawUsage.cache_creation_input_tokens` off `rawUsage = response.usage`
/// (`anthropic/provider.ts:519-520`) — a 2xx with no `usage` (a compat gateway)
/// throws there, so v4's cheap chain fails over where an answer would
/// otherwise come back (a §3 finding at the `f6426e196` recorded-divergences
/// unification; the second `content.filter` walk reads nothing new).
fn anthropic_chain(body: &Js<'_>) -> Result<(), String> {
    let content = get(body, "content")?;
    // `.filter` off a nullish `content` names the read that throws.
    get(&content, "filter")?;
    if !matches!(content, Js::Val(Value::Array(_))) {
        return Err("response.content.filter is not a function".to_string());
    }
    for block in iterate(&content, "response.content")? {
        get(&block, "type")?;
    }
    let usage = get(body, "usage")?;
    get(&usage, "cache_creation_input_tokens")?;
    Ok(())
}

/// V8's `JSON.parse` text for a body that is not JSON — or, where V8 ACCEPTS
/// what serde refused ([`v8_json_parse_message`](crate::jsstr::v8_json_parse_message)
/// answers `None`), serde's own text: a RECORDED divergence (the module doc).
fn json_parse_failure(body: &[u8], serde_error: &serde_json::Error) -> String {
    crate::jsstr::v8_json_parse_message(&String::from_utf8_lossy(body))
        .unwrap_or_else(|| serde_error.to_string())
}

/// Which client parsed the body (the empty-body rule).
fn is_openai_sdk(kind: ProviderKind) -> bool {
    matches!(
        kind,
        ProviderKind::OpenAi
            | ProviderKind::OpenAiCompatible
            | ProviderKind::DeepSeek
            | ProviderKind::NanoGpt
            | ProviderKind::ZAi
            | ProviderKind::Grok
    )
}

/// The text v4's `provider` plugin THROWS on a non-streaming 2xx `body`, or
/// `None` when v4 answers it (see the module doc). The caller has already
/// seen a 2xx; an unknown provider is never guarded.
pub fn v4_send_shape_error(provider: &str, body: &[u8]) -> Option<String> {
    let kind = ProviderKind::of(provider)?;
    let owned: Value;
    let parsed = if body.is_empty() && is_openai_sdk(kind) {
        // `internal/parse.js`: `if (!bodyText) return undefined`.
        Js::Undefined
    } else {
        owned = match serde_json::from_slice::<Value>(body) {
            Ok(v) => v,
            Err(e) => return Some(json_parse_failure(body, &e)),
        };
        Js::Val(&owned)
    };
    let chain = match kind {
        ProviderKind::OpenAiCompatible => chat_completions_chain(&parsed, "tool_calls"),
        ProviderKind::DeepSeek | ProviderKind::ZAi => {
            chat_completions_chain(&parsed, "reasoning_content")
        }
        ProviderKind::NanoGpt => chat_completions_chain(&parsed, "reasoning"),
        ProviderKind::OpenAi | ProviderKind::Grok => responses_chain(&parsed),
        ProviderKind::Anthropic => anthropic_chain(&parsed),
        ProviderKind::Google | ProviderKind::Ollama | ProviderKind::OpenRouter => Ok(()),
    };
    chain.err()
}

#[cfg(test)]
mod tests {
    //! One pin per family × posed shape. The `ok_*` bytes are the recorded v4
    //! throws (`text-http-errors.recorded.ndjson`); the null / primitive /
    //! element arms are V8's texts measured on Node 24.13.1 (the module doc).
    use super::*;

    fn g(provider: &str, body: &str) -> Option<String> {
        v4_send_shape_error(provider, body.as_bytes())
    }

    const UNDEF: fn(&str) -> String =
        |k| format!("Cannot read properties of undefined (reading '{k}')");
    const NULL: fn(&str) -> String = |k| format!("Cannot read properties of null (reading '{k}')");

    #[test]
    fn chat_completions_chains() {
        for (p, next) in [
            ("OPENAI_COMPATIBLE", "tool_calls"),
            ("DEEPSEEK", "reasoning_content"),
            ("Z_AI", "reasoning_content"),
            ("NANOGPT", "reasoning"),
        ] {
            assert_eq!(g(p, r#"{"choices":[]}"#), Some(UNDEF("message")), "{p}");
            assert_eq!(g(p, r#"{"choices":[{}]}"#), Some(UNDEF(next)), "{p}");
            assert_eq!(g(p, "{}"), Some(UNDEF("0")), "{p}");
            assert_eq!(g(p, ""), Some(UNDEF("choices")), "{p}: the SDK's undefined");
            assert_eq!(g(p, r#"{"choices":[{"message":{}}]}"#), None, "{p}");
            assert_eq!(
                g(p, r#"{"choices":[{"message":{"content":"hi"}}]}"#),
                None,
                "{p}"
            );
            // Null-valued keys and a `[null]` choices array.
            assert_eq!(g(p, r#"{"choices":null}"#), Some(NULL("0")), "{p}");
            assert_eq!(g(p, r#"{"choices":[null]}"#), Some(NULL("message")), "{p}");
            assert_eq!(
                g(p, r#"{"choices":[{"message":null}]}"#),
                Some(NULL(next)),
                "{p}"
            );
            assert_eq!(g(p, "null"), Some(NULL("choices")), "{p}");
            // A primitive body has no `choices`.
            assert_eq!(g(p, "42"), Some(UNDEF("0")), "{p}");
            // A string `choices` indexes to a one-unit string, whose
            // `.message` is undefined.
            assert_eq!(g(p, r#"{"choices":"ab"}"#), Some(UNDEF(next)), "{p}");
        }
    }

    #[test]
    fn responses_chain() {
        for p in ["OPENAI", "GROK"] {
            let not_iterable = Some("response.output is not iterable".to_string());
            assert_eq!(g(p, r#"{"choices":[]}"#), not_iterable, "{p}");
            assert_eq!(g(p, "{}"), not_iterable, "{p}");
            assert_eq!(g(p, r#"{"output":null}"#), not_iterable, "{p}");
            assert_eq!(g(p, r#"{"output":5}"#), not_iterable, "{p}");
            assert_eq!(
                g(p, ""),
                Some("Cannot use 'in' operator to search for 'object' in undefined".into()),
                "{p}"
            );
            assert_eq!(
                g(p, "null"),
                Some("Cannot use 'in' operator to search for 'object' in null".into())
            );
            assert_eq!(
                g(p, "1.5"),
                Some("Cannot use 'in' operator to search for 'object' in 1.5".into())
            );
            assert_eq!(
                g(p, r#""abc""#),
                Some("Cannot use 'in' operator to search for 'object' in abc".into())
            );
            assert_eq!(g(p, r#"{"output":[]}"#), None);
            assert_eq!(g(p, r#"{"output":"ab"}"#), None, "a string is iterable");
            assert_eq!(g(p, r#"{"output":[null]}"#), Some(NULL("type")));
            // The SDK's `addOutputText` walks first when `object` is `response`.
            assert_eq!(
                g(p, r#"{"object":"response"}"#),
                Some("rsp.output is not iterable".into())
            );
            assert_eq!(
                g(
                    p,
                    r#"{"object":"response","output":[{"type":"message","content":null}]}"#
                ),
                Some("output.content is not iterable".into())
            );
            assert_eq!(
                g(
                    p,
                    r#"{"object":"response","output":[{"type":"message","content":[{"type":"output_text","text":"x"}]}]}"#
                ),
                None
            );
        }
    }

    #[test]
    fn anthropic_chain() {
        let p = "ANTHROPIC";
        assert_eq!(g(p, r#"{"choices":[]}"#), Some(UNDEF("filter")));
        assert_eq!(g(p, "{}"), Some(UNDEF("filter")));
        assert_eq!(g(p, r#"{"content":null}"#), Some(NULL("filter")));
        assert_eq!(g(p, "null"), Some(NULL("content")));
        assert_eq!(
            g(p, ""),
            Some("Unexpected end of JSON input".into()),
            "JSON.parse('')"
        );
        for c in [r#""abc""#, "5", "{}"] {
            assert_eq!(
                g(p, &format!(r#"{{"content":{c}}}"#)),
                Some("response.content.filter is not a function".into())
            );
        }
        assert_eq!(g(p, r#"{"content":[null]}"#), Some(NULL("type")));
        // `rawUsage.cache_creation_input_tokens` off a missing / null `usage`.
        assert_eq!(
            g(p, r#"{"content":[{"type":"text","text":"x"}]}"#),
            Some(UNDEF("cache_creation_input_tokens"))
        );
        assert_eq!(
            g(p, r#"{"content":[],"usage":null}"#),
            Some(NULL("cache_creation_input_tokens"))
        );
        assert_eq!(
            g(
                p,
                r#"{"content":[{"type":"text","text":"x"}],"usage":{"input_tokens":1,"output_tokens":1}}"#
            ),
            None
        );
        assert_eq!(g(p, r#"{"content":[],"usage":{}}"#), None);
    }

    #[test]
    fn the_fetch_and_genai_plugins_never_throw_on_a_json_shape() {
        for p in ["GOOGLE", "OLLAMA", "OPENROUTER"] {
            for body in [
                r#"{"choices":[]}"#,
                r#"{"choices":[{}]}"#,
                "{}",
                "null",
                "[]",
            ] {
                assert_eq!(g(p, body), None, "{p} {body}");
            }
            assert_eq!(g(p, ""), Some("Unexpected end of JSON input".into()), "{p}");
        }
    }

    #[test]
    fn a_body_that_is_not_json_throws_v8s_parse_text_everywhere() {
        for p in [
            "OPENAI",
            "OPENAI_COMPATIBLE",
            "DEEPSEEK",
            "NANOGPT",
            "Z_AI",
            "GROK",
            "ANTHROPIC",
            "GOOGLE",
            "OLLAMA",
            "OPENROUTER",
        ] {
            assert_eq!(
                g(p, "not json"),
                Some(r#"Unexpected token 'o', "not json" is not valid JSON"#.into()),
                "{p}"
            );
            // Whitespace is not empty: `JSON.parse("  ")` on every client.
            assert_eq!(
                g(p, "  "),
                Some("Unexpected end of JSON input".into()),
                "{p}"
            );
        }
    }

    /// P4.154: a failure INSIDE a value that starts legally reads V8's
    /// template (the `bare-key` row of the recorded
    /// `v8-json-parse-messages` corpus) — before P4.154 serde's `key must be a
    /// string at line 1 column 2` stood in. What still falls back to serde is
    /// a body V8 ACCEPTS (a number past f64 is `Infinity` to V8): v4's plugin
    /// reads on, v5 answers serde's refusal — the RECORDED divergence.
    #[test]
    fn an_in_value_parse_failure_reads_v8s_template() {
        let got = g("DEEPSEEK", "{a:1}").expect("throws");
        assert_eq!(
            got,
            "Expected property name or '}' in JSON at position 1 (line 1 column 2)"
        );
        let got = g("DEEPSEEK", r#"{"a":1e400}"#).expect("serde refuses");
        assert_eq!(got, "number out of range at line 1 column 10");
    }

    #[test]
    fn an_unknown_provider_is_never_guarded() {
        assert_eq!(g("NOT_A_PROVIDER", "not json"), None);
    }
}
