//! Differential: the per-provider request-envelope builders + the four
//! `RequestTransform` hooks (wave 4 / W4.7c part 2).
//!
//! For every committed line, this reconstructs the [`RequestInput`] from the
//! recorded `input` params, runs the Rust `model::request_builder`, and diffs the
//! built body / url / method byte-for-byte against the request envelope RECORDED
//! by intercepting the outgoing `fetch` of v4's REAL plugin — the SDK / raw fetch
//! sends `JSON.stringify(body)` verbatim, so the body is a byte comparison.
//!
//! **Both halves (P4.11).** Each line carries a `mode`: `"stream"` drives v4's
//! `streamMessage`, `"send"` its `sendMessage`, and the mode becomes
//! `RequestInput.stream`. A line with no `mode` predates P4.11 and means
//! `"stream"`. Recording only the streaming half is how dogfood #23 — every
//! builder hard-coding `stream: true`, so every non-streaming caller in
//! production got an SSE body it could not parse — survived a
//! differential-verified port; the coverage assertion at the bottom is what
//! keeps a (provider, mode) pair from going missing again.
//!
//! A line may instead carry `refused: true`: v4's plugin (or its SDK) rejected
//! the params CLIENT-SIDE and made no request. Those diff against
//! `BuildError::ProviderRefused` — a refusal is a contract, not a gap.
//!
//! Covers every `RequestTransform` branch: anthropic (plain + sampling-rejected +
//! thinking + caching + tools/stop + tool-roundtrip), openai (first-call vs
//! chained + reasoning-model + cache-retention), deepseek (reasoning echo + thinking
//! strip + profile params), plus the plain providers (z-ai, openrouter, ollama,
//! grok). Google's genai-SDK wire framing is deferred to the transport; its request
//! LOGIC is verified in `request_builder_google_equivalence`.
//!
//! **The leading-system fold (P4.D93, v4 bug 82).** `three-leading-system` rows
//! for ollama and openai-compatible (both modes) show the three-block turn head
//! folded into one system message; `three-leading-system-unfolded` for deepseek —
//! a hosted subclass of the SAME v4 base class — shows all three still on the
//! wire. That last row is the regression guard: folding there would cost the
//! cache prefix the three blocks exist to protect.
//!
//! **Headers (P4.44 item 3).** Each row also carries the outbound `headers` the
//! v4 plugin/SDK put on the wire. The differential compares them at the
//! POST-`apply_auth` point — v5's REAL header set is driven through
//! `execute_completion` (`build_request` → `transport_headers` → `apply_auth`,
//! completion_provider.rs:140-141), which is where User-Agent, OpenRouter's
//! `HTTP-Referer`/`X-Title`, and the api key actually land (not on `built.headers`
//! alone). It is a SUBSET check — every header v5 models must appear in v4's
//! recorded set with a matching value — because v4's SDKs add `x-stainless-*`
//! plumbing a single reqwest transport neither sends nor should. The
//! version-bearing User-Agent and the auth secret are normalized to placeholders.
//! One documented, OpenRouter-only divergence: v4's `@openrouter/sdk` (speakeasy)
//! send path overrides the User-Agent with its OWN and omits X-Title, so on those
//! rows v5's transport-level `user-agent`/`x-title` differ by design (the vision
//! send path is raw-fetch — it records `Quilltap/` + referer + title, matching
//! v5). The other providers' stainless SDKs respect the configured Quilltap UA.
//!
//! **Abort/timeout arming is deliberately NOT pinned here** (loud deferral): the
//! recorder observes the fetch ARGUMENTS (method/url/headers/body), but the
//! abort+timeout wiring is SDK-internal (AbortSignal + duration + retry config,
//! not a comparable value on the fetch call), and v5's equivalent lives in the
//! reqwest transport's `TransportPolicy`, not in this sans-IO build path. That
//! behavior is wall-clock — "no NDJSON corpus can observe it" (the P4.15
//! falsifiability ruling) — and is proven unit-tier in `model::transport`'s tests.
//!
//! The corpus is committed
//! (`harness/oracle/fixtures/request-envelopes/request-envelopes.recorded.ndjson`);
//! no env var is needed to run — the family runs in every plain `cargo test`.
//! Run (by name — the corpus is committed, so this IS the whole recipe;
//! recording is a deliberate by-hand step, never a sweep stage):
//!   cargo test -p quilltap-harness --test request_builder_equivalence -- --nocapture
//! Regenerate the corpus (Node 24, only after a v4 provider drift) with
//! `harness/oracle/providers/regenerate-request-envelopes.sh`.

mod provider_header_common;

use quilltap_core::model::request_builder::{
    build_request, RequestInput, StreamMessage, ToolCallFunction, ToolCallPayload,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/request-envelopes/request-envelopes.recorded.ndjson")
}

fn registry_id(plugin_name: &str) -> String {
    plugin_name.to_uppercase().replace('-', "_")
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn message_from_json(m: &Value) -> StreamMessage {
    let content = m
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match m.get("role").and_then(Value::as_str).unwrap_or_default() {
        "system" => StreamMessage::system(content),
        "assistant" => StreamMessage::Assistant {
            content,
            tool_calls: m
                .get("toolCalls")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .map(|tc| {
                            // The corpus only records type "function" (v4's
                            // detection emits nothing else); the enum's static
                            // kind makes any other type a loud loader error.
                            assert_eq!(
                                tc.get("type").and_then(Value::as_str),
                                Some("function"),
                                "corpus tool call with a non-function type"
                            );
                            ToolCallPayload {
                                id: tc
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                kind: "function",
                                function: ToolCallFunction {
                                    name: tc
                                        .get("function")
                                        .and_then(|f| f.get("name"))
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                    arguments: tc
                                        .get("function")
                                        .and_then(|f| f.get("arguments"))
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string(),
                                },
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            reasoning_content: opt_str(m, "reasoningContent"),
            thought_signature: opt_str(m, "thoughtSignature"),
            cache_control: m.get("cacheControl").cloned(),
        },
        // An id-less tool message is unrepresentable in v5 (the carrying enum
        // requires the call id); a corpus vector carrying one must FAIL the
        // loader loudly rather than be silently reshaped.
        "tool" => StreamMessage::Tool {
            call_id: opt_str(m, "toolCallId")
                .expect("corpus tool message without a toolCallId (unrepresentable in v5)"),
            name: opt_str(m, "name"),
            content,
        },
        _ => StreamMessage::User {
            content,
            cache_control: m.get("cacheControl").cloned(),
            attachments: m
                .get("attachments")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        },
    }
}

fn input_from_json(input: &Value, stream: bool) -> RequestInput {
    let messages = input
        .get("messages")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(message_from_json).collect())
        .unwrap_or_default();
    let stop = input.get("stop").and_then(Value::as_array).map(|arr| {
        arr.iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect()
    });
    RequestInput {
        model: input
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        messages,
        temperature: input.get("temperature").and_then(Value::as_f64),
        max_tokens: input.get("maxTokens").and_then(Value::as_i64),
        top_p: input.get("topP").and_then(Value::as_f64),
        stop,
        tools: input.get("tools").and_then(Value::as_array).cloned(),
        tool_choice: input.get("toolChoice").cloned(),
        response_format: input.get("responseFormat").cloned(),
        web_search_enabled: input
            .get("webSearchEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        profile_parameters: input.get("profileParameters").cloned(),
        cache_key: opt_str(input, "cacheKey"),
        previous_response_id: opt_str(input, "previousResponseId"),
        strict_max_tokens: input
            .get("strictMaxTokens")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        stream,
    }
}

/// `mode` → `RequestInput.stream`. Absent means the pre-P4.11 streaming vectors.
fn stream_from_mode(row: &Value) -> bool {
    match row.get("mode").and_then(Value::as_str).unwrap_or("stream") {
        "stream" => true,
        "send" => false,
        other => panic!("unknown recorded mode {other:?}"),
    }
}

/// The (provider, case, mode) triples where v4 itself refuses before any HTTP
/// call. Enumerated, never inferred: an unexpected refusal line is a recorder or
/// oracle regression, and a refusal that quietly stops being recorded is exactly
/// the "no case" silence that hid dogfood #23.
const EXPECTED_REFUSALS: &[(&str, &str, &str)] = &[
    // The @openrouter/sdk's ChatToolMessage schema wants `toolCallId`; v4 sends
    // `tool_call_id`, so a tool round-trip fails the SDK's INPUT validation and
    // no request leaves the process. (v4's own defect, carried faithfully.) This
    // is the IMAGE-FREE tool round-trip: v4 bug 31 escapes the image case to the
    // raw vision path, where a tool-role message is fine (the
    // `image-tool-roundtrip` send vector succeeds).
    ("OPENROUTER", "tool-roundtrip", "send"),
    // v4 bug 31 (`43a1b5b1`) RETIRED the two non-streaming vision refusals: an
    // image attachment now routes the send around the SDK to a direct
    // chat-completions POST, so `image-attachment[send]` and
    // `image-attachment-tools[send]` produce bodies, not refusals.
];

// ── P4.44 item 3: the outbound-header pin ───────────────────────────────────
//
// The capture machinery lives in `provider_header_common` — it drives the
// production line (`execute_completion` → `build_request` → `transport_headers`
// → `apply_auth`, completion_provider.rs:140-141), which is where User-Agent /
// HTTP-Referer / X-Title / the api key actually land, not `built.headers` alone.
// P4.47 (B) hoisted it there when the google-wire family gained the same pin;
// the two must not drift, so there is one implementation.
//
// The differential compares only the headers v5 MODELS against v4's recorded
// set (a SUBSET: v4's SDKs add `x-stainless-*` plumbing a single reqwest
// transport neither sends nor should), normalizing the version-bearing
// User-Agent and the auth secret.
use provider_header_common::{normalize_header, v5_headers};

#[test]
fn request_builder_matches_v4() {
    let text = std::fs::read_to_string(corpus_path()).expect("committed request-envelope NDJSON");
    let mut rows = 0usize;
    let mut refusals = 0usize;
    let mut pairs = std::collections::HashSet::new();
    // P4.21 attachment-coverage shape (the pre-P4.21 corpus had ZERO attachment
    // vectors — the blind spot that hid dogfood #37; assert the SHAPE, not a
    // hand count, per the corpus-shape-constants-rot rule).
    let mut att_pairs = std::collections::HashSet::new();
    let (mut saw_pdf, mut saw_text_doc, mut saw_no_data, mut saw_multi, mut saw_no_data_failure) =
        (false, false, false, false, false);

    // P4.44 item 3 — header pin. `execute_completion` needs a runtime; v5's
    // headers are provider-invariant, so memoize one set per provider.
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut v5_header_cache: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut header_providers = std::collections::HashSet::new();
    // The OpenRouter SDK-send path where v4's UA/X-Title legitimately diverge —
    // asserted exercised so a corpus that lost those rows cannot pass silently.
    let mut openrouter_sdk_rows = 0usize;
    // P4.D78 Ollama thinking-wire shape: `think` is now on EVERY ollama body,
    // and `options.num_ctx` appears only for a bag that coerces to a finite
    // positive number. Assert the SHAPE (a `think:true` arm, a `think:false`
    // arm, a num_ctx-present arm, and a bag-present-but-key-omitted arm) rather
    // than a hand count — a regenerated corpus that lost one of these would
    // otherwise pass green with the feature untested.
    //
    // P4.D83 widens it: `think` may now be an effort LEVEL string, `keep_alive`
    // reaches the top level (with the sentinels as NUMBERS), and the widened
    // `options` table carries the rest of the profile's sampler knobs. Each of
    // those gets its own arm for the same reason.
    let (
        mut saw_think_true,
        mut saw_think_false,
        mut saw_num_ctx,
        mut saw_num_ctx_omitted_with_bag,
    ) = (false, false, false, false);
    let (
        mut saw_think_level,
        mut saw_keep_alive_number,
        mut saw_keep_alive_duration,
        mut saw_widened_option,
    ) = (false, false, false, false);

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).unwrap();
        let plugin = row["provider"].as_str().unwrap();
        let case = row["case"].as_str().unwrap();
        let mode = row.get("mode").and_then(Value::as_str).unwrap_or("stream");
        let provider = registry_id(plugin);
        pairs.insert((provider.clone(), mode.to_string()));
        rows += 1;

        let input = input_from_json(&row["input"], stream_from_mode(&row));
        let built = build_request(&provider, &input);

        // Attachment-coverage bookkeeping (over the recorded INPUT, so refusal
        // rows count toward coverage too).
        if let Some(msgs) = row["input"].get("messages").and_then(Value::as_array) {
            for m in msgs {
                let Some(atts) = m.get("attachments").and_then(Value::as_array) else {
                    continue;
                };
                if atts.is_empty() {
                    continue;
                }
                att_pairs.insert((provider.clone(), mode.to_string()));
                if atts.len() >= 2 {
                    saw_multi = true;
                }
                for a in atts {
                    match a.get("mimeType").and_then(Value::as_str) {
                        Some("application/pdf") => saw_pdf = true,
                        Some("text/plain") => saw_text_doc = true,
                        _ => {}
                    }
                    if a.get("data").is_none() {
                        saw_no_data = true;
                    }
                }
            }
        }
        if let Some(failed) = row
            .get("attachmentResults")
            .and_then(|r| r.get("failed"))
            .and_then(Value::as_array)
        {
            if failed
                .iter()
                .any(|f| f.get("error").and_then(Value::as_str) == Some("File data not loaded"))
            {
                saw_no_data_failure = true;
            }
        }

        // P4.D78 shape bookkeeping, read off v4's RECORDED body (the oracle
        // side), so a v5 regression cannot make the coverage claim true.
        if provider == "OLLAMA" {
            if let Some(body) = row
                .get("body")
                .and_then(Value::as_str)
                .and_then(|b| serde_json::from_str::<Value>(b).ok())
            {
                match body.get("think") {
                    Some(Value::Bool(true)) => saw_think_true = true,
                    Some(Value::Bool(false)) => saw_think_false = true,
                    // P4.D83: an effort level. v4 refuses any string outside
                    // OLLAMA_THINK_LEVELS, so a level outside the set here means
                    // the recorder (or the port) invented one.
                    Some(Value::String(s)) => {
                        assert!(
                            ["low", "medium", "high", "max"].contains(&s.as_str()),
                            "OLLAMA/{case}[{mode}]: v4 recorded think={s:?}, not one of \
                             Ollama's four levels"
                        );
                        saw_think_level = true;
                    }
                    other => panic!(
                        "OLLAMA/{case}[{mode}]: v4 recorded think={other:?} — the field is \
                         supposed to be present on every body, as a boolean or a level"
                    ),
                }
                let opts = body.get("options");
                let has_num_ctx = opts.and_then(|o| o.get("num_ctx")).is_some();
                let has_bag = row["input"].get("profileParameters").is_some();
                if has_num_ctx {
                    saw_num_ctx = true;
                } else if has_bag {
                    saw_num_ctx_omitted_with_bag = true;
                }
                // P4.D83: at least one body must carry a key from the WIDENED
                // options table (i.e. one that is neither the four literals nor
                // num_ctx), or the table is untested.
                if let Some(Value::Object(o)) = opts {
                    if o.keys().any(|k| {
                        !matches!(
                            k.as_str(),
                            "temperature" | "num_predict" | "top_p" | "stop" | "num_ctx"
                        )
                    }) {
                        saw_widened_option = true;
                    }
                }
                match body.get("keep_alive") {
                    Some(Value::Number(_)) => saw_keep_alive_number = true,
                    Some(Value::String(_)) => saw_keep_alive_duration = true,
                    Some(other) => panic!(
                        "OLLAMA/{case}[{mode}]: v4 recorded keep_alive={other:?} — it goes out \
                         as a number (the sentinels) or a duration string, nothing else"
                    ),
                    None => {}
                }
            }
        }

        // A recorded refusal: v4 made no request, so v5 must not build one.
        if row.get("refused").and_then(Value::as_bool) == Some(true) {
            assert!(
                EXPECTED_REFUSALS.contains(&(provider.as_str(), case, mode)),
                "{provider}/{case}[{mode}] recorded a NEW refusal — v4 rejected the \
                 params before any request ({}). Add it to EXPECTED_REFUSALS with \
                 the reason, or fix the recorder.",
                row.get("error").and_then(Value::as_str).unwrap_or("?")
            );
            match built {
                Err(quilltap_core::model::request_builder::BuildError::ProviderRefused(_)) => {}
                Err(e) => panic!("{provider}/{case}[{mode}]: expected ProviderRefused, got {e}"),
                Ok(_) => {
                    panic!("{provider}/{case}[{mode}]: v4 REFUSES these params but v5 built a body")
                }
            }
            refusals += 1;
            continue;
        }

        let built =
            built.unwrap_or_else(|e| panic!("{provider}/{case}[{mode}]: build failed: {e}"));

        // Body byte-exact (the transforms live here).
        let want_body = row["body"].as_str().unwrap();
        let got_body = built.body_string();
        assert_eq!(
            got_body, want_body,
            "\n{provider}/{case}[{mode}] BODY diverged\n  got:  {got_body}\n  want: {want_body}\n"
        );

        // Method + url.
        assert_eq!(
            built.method,
            row["method"].as_str().unwrap(),
            "{provider}/{case}[{mode}] method"
        );
        assert_eq!(
            built.url,
            row["url"].as_str().unwrap(),
            "{provider}/{case}[{mode}] url"
        );

        // Headers (P4.44 item 3): every header v5 MODELS must appear in v4's
        // recorded set with the same value (UA + auth normalized). A SUBSET check
        // — v4's SDKs add `x-stainless-*` plumbing v5's single reqwest transport
        // does not. An absent `headers` field is a pre-P4.44 line (skipped).
        if let Some(recorded) = row.get("headers").and_then(Value::as_object) {
            // v4's @openrouter/sdk (speakeasy) SDK-send path overrides the
            // User-Agent with its OWN and omits X-Title; every other provider's
            // SDK (the stainless family) respects the configured Quilltap UA. v5
            // uses ONE reqwest transport for stream+send, so on that path its
            // transport headers (Quilltap UA + X-Title) legitimately differ — a
            // documented, OpenRouter-only divergence rooted in the single-
            // transport design. Detect it by the recorded UA not being Quilltap's.
            let recorded_ua = recorded
                .get("user-agent")
                .and_then(Value::as_str)
                .unwrap_or("");
            let sdk_path = !recorded_ua.is_empty() && !recorded_ua.starts_with("Quilltap/");
            if sdk_path {
                assert_eq!(
                    provider, "OPENROUTER",
                    "{provider}/{case}[{mode}]: a provider SDK overrode the User-Agent \
                     ({recorded_ua:?}) — only the @openrouter/sdk does this today; a new \
                     one is a real divergence to investigate"
                );
                openrouter_sdk_rows += 1;
            }
            let v5 = v5_header_cache
                .entry(provider.clone())
                .or_insert_with(|| v5_headers(&rt, &provider));
            for (name, value) in v5.iter() {
                // On v4's SDK-delegated path, v5's transport-level `user-agent`
                // and `x-title` differ by design — pin the rest.
                if sdk_path && (name == "user-agent" || name == "x-title") {
                    continue;
                }
                let want = normalize_header(name, value);
                match recorded.get(name).and_then(Value::as_str) {
                    Some(got_raw) => {
                        let got = normalize_header(name, got_raw);
                        assert_eq!(
                            got, want,
                            "{provider}/{case}[{mode}] header `{name}` diverged"
                        )
                    }
                    None => panic!(
                        "{provider}/{case}[{mode}]: v5 sends header `{name}`={want:?} but v4 \
                         recorded none — regenerate the corpus or fix the port"
                    ),
                }
            }
            header_providers.insert(provider.clone());
        }

        // Attachment results (P4.21): the recorder keeps the field only when
        // non-empty, so an absent field means v4 reported NOTHING — and v5
        // must report nothing too, not just "we didn't check".
        let got_results = serde_json::to_value(&built.attachment_results).unwrap();
        match row.get("attachmentResults") {
            Some(want) => assert_eq!(
                &got_results, want,
                "{provider}/{case}[{mode}] attachmentResults diverged"
            ),
            None => assert_eq!(
                got_results,
                serde_json::json!({ "sent": [], "failed": [] }),
                "{provider}/{case}[{mode}] reported attachment results v4 did not"
            ),
        }
    }

    assert!(rows >= 25, "expected a substantial corpus, got {rows}");
    assert_eq!(
        refusals,
        EXPECTED_REFUSALS.len(),
        "every enumerated refusal must still be recorded — a vanished refusal line \
         reads as 'no case', which is how a whole mode went unchecked before"
    );

    // Coverage: EVERY provider in BOTH modes. Not an eyeball — the blind spot
    // this closes was precisely a mode nobody noticed was absent.
    for p in [
        "ANTHROPIC",
        "OPENAI",
        "DEEPSEEK",
        "OLLAMA",
        "GROK",
        "Z_AI",
        "OPENROUTER",
        "OPENAI_COMPATIBLE",
    ] {
        for mode in ["stream", "send"] {
            assert!(
                pairs.contains(&(p.to_string(), mode.to_string())),
                "corpus missing provider {p} in mode {mode}"
            );
        }
    }
    // Every provider must have an attachment vector in BOTH modes — a corpus
    // with zero attachment vectors is exactly how #37 survived green.
    for p in [
        "ANTHROPIC",
        "OPENAI",
        "DEEPSEEK",
        "OLLAMA",
        "GROK",
        "Z_AI",
        "OPENROUTER",
        "OPENAI_COMPATIBLE",
    ] {
        for mode in ["stream", "send"] {
            assert!(
                att_pairs.contains(&(p.to_string(), mode.to_string())),
                "corpus has no attachment vector for provider {p} in mode {mode}"
            );
        }
    }
    assert!(saw_pdf, "corpus lost its PDF-attachment vector");
    assert!(saw_text_doc, "corpus lost its text-document vectors");
    assert!(saw_no_data, "corpus lost its data-not-loaded vector");
    assert!(saw_multi, "corpus lost its multi-attachment vector");
    assert!(
        saw_no_data_failure,
        "corpus lost the recorded 'File data not loaded' failure arm"
    );

    // Header coverage (P4.44 item 3): every provider must have had at least one
    // row whose recorded headers were checked against v5's modeled set — a corpus
    // that lost its `headers` key for a provider would silently stop pinning it.
    for p in [
        "ANTHROPIC",
        "OPENAI",
        "DEEPSEEK",
        "OLLAMA",
        "GROK",
        "Z_AI",
        "OPENROUTER",
        "OPENAI_COMPATIBLE",
    ] {
        assert!(
            header_providers.contains(p),
            "no recorded headers checked for provider {p} — regenerate the corpus"
        );
    }
    // P4.D78 — the Ollama thinking-wire coverage shape.
    assert!(
        saw_think_true,
        "corpus lost its ollama `think: true` vector (enable_thinking on)"
    );
    assert!(
        saw_think_false,
        "corpus lost its ollama `think: false` vector (the always-present default)"
    );
    assert!(
        saw_num_ctx,
        "corpus lost its ollama `options.num_ctx` vector"
    );
    assert!(
        saw_num_ctx_omitted_with_bag,
        "corpus lost its ollama num_ctx-rejected vector (a profile bag whose \
         value leaves the key off the wire)"
    );
    assert!(
        saw_think_level,
        "corpus lost its ollama `think: <level>` vector (thinking_effort folded \
         into the think field)"
    );
    assert!(
        saw_widened_option,
        "corpus lost its ollama widened-`options` vector — the allow-list table \
         beyond the four literals is untested"
    );
    assert!(
        saw_keep_alive_number,
        "corpus lost its ollama numeric `keep_alive` vector (the -1/0 sentinels, \
         which 0.32.1 refuses as duration STRINGS)"
    );
    assert!(
        saw_keep_alive_duration,
        "corpus lost its ollama duration-string `keep_alive` vector"
    );
    assert!(
        openrouter_sdk_rows > 0,
        "the OpenRouter SDK-send divergence (speakeasy UA / no X-Title) is no longer \
         exercised — a lost vector, or the SDK stopped overriding the UA"
    );

    eprintln!(
        "OK: {rows} request envelopes ({refusals} recorded refusal(s)) matched v4; \
         headers pinned for {} providers.",
        header_providers.len()
    );
}
