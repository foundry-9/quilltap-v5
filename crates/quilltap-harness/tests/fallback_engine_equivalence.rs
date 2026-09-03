//! P4.D135 — the pure fallback engine, tier-1 exact (v4 `65f5021c8`,
//! `lib/llm/fallback/`).
//!
//! Drives v4's REAL `classifyFallbackTrigger` / `buildFallbackChain` /
//! `recordAttempt` / `summarizeFallbackAttempts` / `pickTierCandidate` /
//! `tierMatches` over a corpus built from v4's own 568 test lines plus the arms
//! those tests do not reach, and diffs `quilltap_core::llm_fallback` against it.
//!
//! ## The capability comparand
//!
//! v4's own tests MOCK `providerCanTransportImages`, so they never compare the
//! real answer. The two ports resolve it differently — v5 reads its built-in
//! manifest registry first, while a plain tsx script leaves v4's registry
//! uninitialised and lands on the static mirror — so a provider the two disagree
//! about would show up as a wrong chain with no explanation. The oracle
//! therefore emits `kind: 'transport'` rows for every provider the corpus names,
//! and this test asserts v5's own `provider_can_transport_images` against them
//! FIRST. A capability divergence fails by name.
//!
//! ## The arrival verdict (bug 116)
//!
//! P4.D151 (v4 `0b0617fee`): the family gained a `verdict` kind driving v4's
//! REAL exported `verifyImageReachedModel` — the pure half of the
//! describer-arrival check. `{arrived, reason}` is compared byte-for-byte,
//! because the refusal sentence is what reaches the user inside
//! `describeImageWithProfile`'s `unsupported` error. Five shapes come from v4's
//! own new test block; the rest are the arms it does not reach (the `<=`
//! boundary at the ceiling itself, the `||` fallback on an EMPTY error string,
//! the id-matching rule with the matching failed entry SECOND, the ledger's
//! precedence over the token count).
//!
//! ## The error shape
//!
//! v4 classifies an `unknown` by walking `instanceof`, then `error.name`, then
//! `error.message`. v5 has no error-class hierarchy at the stream seam, so its
//! [`FallbackError`] names those three inputs. The oracle emits the OBSERVED
//! `(name, message)` pair rather than a constructor label, and this test maps
//! the name back to an [`LlmErrorKind`] where it is one of v4's eight classes —
//! so both sides classify from the same two strings.
//!
//! Generate the oracle (Node 24, from the v4 checkout; pin a detached worktree
//! via `recipe_sweep.py --v4` when v4 HEAD has moved past the baseline):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   LOG_LEVEL=error \
//!     $N/npx tsx $V5W/harness/oracle/cases/fallback-engine.ts \
//!     > /tmp/oracle-fallback-engine.ndjson
//!
//! ⚠ `LOG_LEVEL=error` is load-bearing: the engine logs through v4's real
//! logger, which writes JSON to STDOUT and would interleave log records into
//! the NDJSON. The reader below refuses such a line by name rather than dying
//! on a missing field.
//! Run:
//!   QT_ORACLE_FALLBACK_ENGINE=/tmp/oracle-fallback-engine.ndjson \
//!     cargo test -p quilltap-harness --test fallback_engine_equivalence -- --nocapture

use std::collections::BTreeMap;

use quilltap_core::files::image_transport::provider_can_transport_images;
use quilltap_core::llm_fallback::{
    build_fallback_chain, classify_fallback_trigger, pick_tier_candidate, record_attempt,
    summarize_fallback_attempts, FallbackAttempt, FallbackContext, FallbackError, FallbackProfile,
    FallbackPurpose, FallbackRepos, FallbackTrigger,
};
use quilltap_core::services::api_key_service::{
    provider_accepts_api_key, provider_requires_api_key,
};
use quilltap_core::services::llm_errors::LlmErrorKind;
use serde_json::Value;

/// Rebuild the `CompletionResponse` a `verdict` row describes. Only the three
/// fields `verify_image_reached_model` reads are carried; `content` is
/// irrelevant to it (the verdict runs BEFORE any content check, which is the
/// whole point of bug 116).
///
/// ⚠ An absent `usage` is the whole of v4's `typeof promptTokens !== 'number'`
/// arm here — `CompletionUsage.prompt_tokens` is an `i64` and structurally
/// cannot be a non-number, so the oracle emits no such row.
fn completion_response_from(v: &Value) -> quilltap_core::model::completion::CompletionResponse {
    use quilltap_core::model::completion::{CompletionResponse, CompletionUsage};
    use quilltap_core::model::stream::{
        StreamAttachmentFailure, StreamAttachmentResults, StreamCacheUsage,
    };
    let usage = v
        .get("usage")
        .filter(|u| u.is_object())
        .map(|u| CompletionUsage {
            prompt_tokens: u["promptTokens"].as_i64().unwrap_or(0),
            completion_tokens: u["completionTokens"].as_i64().unwrap_or(0),
            total_tokens: u["totalTokens"].as_i64().unwrap_or(0),
        });
    let cache_usage = v
        .get("cacheUsage")
        .filter(|c| c.is_object())
        .map(|c| StreamCacheUsage {
            cached_tokens: c["cachedTokens"].as_i64(),
            cache_discount: c["cacheDiscount"].as_f64(),
            cache_creation_input_tokens: c["cacheCreationInputTokens"].as_i64(),
            cache_read_input_tokens: c["cacheReadInputTokens"].as_i64(),
        });
    let attachment_results = v
        .get("attachmentResults")
        .filter(|a| a.is_object())
        .map(|a| StreamAttachmentResults {
            sent: a["sent"]
                .as_array()
                .map(|xs| {
                    xs.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            failed: a["failed"]
                .as_array()
                .map(|xs| {
                    xs.iter()
                        .map(|f| StreamAttachmentFailure {
                            id: f["id"].as_str().unwrap_or("").to_string(),
                            error: f["error"].as_str().unwrap_or("").to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        });
    CompletionResponse {
        content: String::new(),
        usage,
        finish_reason: None,
        attachment_results,
        cache_usage,
    }
}

/// The in-memory repo the oracle's `makeRepos` is.
struct MemoryRepos(Vec<FallbackProfile>);

impl FallbackRepos for MemoryRepos {
    fn find_by_id(&self, id: &str) -> Option<FallbackProfile> {
        self.0.iter().find(|p| p.id == id).cloned()
    }
    fn find_by_user_id(&self, user_id: &str) -> Vec<FallbackProfile> {
        self.0
            .iter()
            .filter(|p| p.user_id == user_id)
            .cloned()
            .collect()
    }
}

/// v4's class `name` → the normalized kind. Anything else (`Error`, `ZodError`,
/// `CheapLLMTimeoutError`) rides `FallbackError::name`, exactly as v4's
/// classifier reads it.
fn kind_for(name: &str) -> Option<LlmErrorKind> {
    match name {
        "LLMProviderError" => Some(LlmErrorKind::Base),
        "APIKeyError" => Some(LlmErrorKind::ApiKey),
        "RateLimitError" => Some(LlmErrorKind::RateLimit),
        "NetworkError" => Some(LlmErrorKind::Network),
        "ModelNotFoundError" => Some(LlmErrorKind::ModelNotFound),
        "InvalidRequestError" => Some(LlmErrorKind::InvalidRequest),
        "TokenLimitError" => Some(LlmErrorKind::TokenLimit),
        "ContentLimitError" => Some(LlmErrorKind::ContentLimit),
        _ => None,
    }
}

fn error_from(case: &Value) -> (Option<LlmErrorKind>, Option<String>, String) {
    let message = case["errMessage"].as_str().unwrap_or("").to_string();
    let is_error = case["isError"].as_bool().unwrap_or(false);
    if !is_error {
        // v4's non-Error throw: `String(error ?? '')`, and no `name` at all.
        return (None, None, message);
    }
    let name = case["errName"].as_str().unwrap_or("Error").to_string();
    let kind = kind_for(&name);
    (kind, Some(name), message)
}

/// The `Option<&str>` [`record_attempt`] takes: `None` for v4's `throw null` /
/// `throw undefined` (its `?? 'unknown error'` arm), `Some` for everything
/// else — an `Error` with an empty message included, which `String(error ?? '')`
/// alone renders identically to a nullish throw.
fn record_message(case: &Value) -> Option<String> {
    if case["isNullish"].as_bool().unwrap_or(false) {
        None
    } else {
        Some(case["errMessage"].as_str().unwrap_or("").to_string())
    }
}

fn trigger_name(t: Option<FallbackTrigger>) -> Value {
    match t {
        Some(t) => Value::String(t.as_str().to_string()),
        None => Value::Null,
    }
}

fn trigger_from(s: &str) -> FallbackTrigger {
    match s {
        "auth" => FallbackTrigger::Auth,
        "rate-limit" => FallbackTrigger::RateLimit,
        "network" => FallbackTrigger::Network,
        "model-missing" => FallbackTrigger::ModelMissing,
        "provider-error" => FallbackTrigger::ProviderError,
        "empty-response" => FallbackTrigger::EmptyResponse,
        "moderation-refusal" => FallbackTrigger::ModerationRefusal,
        other => panic!("unknown trigger in the oracle: {other}"),
    }
}

fn purpose_from(s: &str) -> FallbackPurpose {
    match s {
        "chat" => FallbackPurpose::Chat,
        "cheap" => FallbackPurpose::Cheap,
        "vision" => FallbackPurpose::Vision,
        "carina" => FallbackPurpose::Carina,
        "console" => FallbackPurpose::Console,
        "help" => FallbackPurpose::Help,
        other => panic!("unknown purpose in the oracle: {other}"),
    }
}

fn context_from(v: &Value) -> FallbackContext {
    FallbackContext {
        user_id: v["userId"].as_str().unwrap_or("").to_string(),
        purpose: purpose_from(v["purpose"].as_str().unwrap_or("chat")),
        dangerous: v["dangerous"].as_bool().unwrap_or(false),
        needs_vision: v["needsVision"].as_bool().unwrap_or(false),
        needs_tools: v["needsTools"].as_bool().unwrap_or(false),
        already_tried: v["alreadyTried"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn profile_from(v: &Value) -> FallbackProfile {
    FallbackProfile::from_value(v).unwrap_or_else(|| panic!("oracle profile has no id: {v}"))
}

fn attempt_from(v: &Value) -> FallbackAttempt {
    FallbackAttempt {
        profile_id: v["profileId"].as_str().unwrap_or("").to_string(),
        profile_name: v["profileName"].as_str().unwrap_or("").to_string(),
        provider: v["provider"].as_str().unwrap_or("").to_string(),
        model_name: v["modelName"].as_str().unwrap_or("").to_string(),
        trigger: trigger_from(v["trigger"].as_str().unwrap_or("provider-error")),
        error: v["error"].as_str().unwrap_or("").to_string(),
    }
}

#[test]
fn fallback_engine_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_FALLBACK_ENGINE") else {
        eprintln!("SKIP: set QT_ORACLE_FALLBACK_ENGINE to the oracle NDJSON (see the header).");
        return;
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let cases: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();
    assert!(!cases.is_empty(), "oracle produced no cases");

    let mut failed: Vec<String> = Vec::new();
    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();

    for case in &cases {
        // A log record that leaked onto stdout has neither key. Say so, rather
        // than dying on `case id` three frames later.
        let (Some(id), Some(kind)) = (case["id"].as_str(), case["kind"].as_str()) else {
            panic!(
                "the oracle NDJSON carries a line that is not a corpus row — v4's logger \
                 wrote to stdout. Regenerate with LOG_LEVEL=error (see the header). Line: {}",
                serde_json::to_string(case).unwrap_or_default()
            );
        };
        *by_kind
            .entry(match kind {
                "transport" => "transport",
                "apiKeyCapability" => "apiKeyCapability",
                "classify" => "classify",
                "tierMatches" => "tierMatches",
                "pick" => "pick",
                "chain" => "chain",
                "record" => "record",
                "summarize" => "summarize",
                "verdict" => "verdict",
                other => panic!("unknown oracle kind {other}"),
            })
            .or_default() += 1;

        let mut diffs: Vec<String> = Vec::new();

        match kind {
            // The shared capability answers. Asserted FIRST so a
            // registry-vs-mirror disagreement is a named failure rather than a
            // mystery chain three cases later.
            "apiKeyCapability" => {
                let provider = case["provider"].as_str().unwrap_or("");
                for (label, oracle, rust) in [
                    (
                        "provider_accepts_api_key",
                        case["accepts"].as_bool().unwrap_or(false),
                        provider_accepts_api_key(provider),
                    ),
                    (
                        "provider_requires_api_key",
                        case["requires"].as_bool().unwrap_or(false),
                        provider_requires_api_key(provider),
                    ),
                ] {
                    if rust != oracle {
                        diffs.push(format!(
                            "    {label}({provider:?}): rust {rust} != v4 {oracle} — the \
                             credential filter below is measuring a different world"
                        ));
                    }
                }
            }
            "transport" => {
                let provider = case["provider"].as_str().unwrap_or("");
                let oracle = case["canTransport"].as_bool().unwrap_or(false);
                let rust = provider_can_transport_images(provider);
                if rust != oracle {
                    diffs.push(format!(
                        "    provider_can_transport_images({provider:?}): rust {rust} != v4 \
                         {oracle} — the two capability sources disagree, so every vision arm \
                         below is measuring a different world"
                    ));
                }
            }
            "classify" => {
                let (k, name, message) = error_from(case);
                let got = classify_fallback_trigger(FallbackError {
                    kind: k,
                    name: name.as_deref(),
                    message: &message,
                });
                if trigger_name(got) != case["trigger"] {
                    diffs.push(format!(
                        "    trigger: rust {} != oracle {} (name {:?}, message {:?})",
                        trigger_name(got),
                        case["trigger"],
                        name,
                        message
                    ));
                }
            }
            "tierMatches" => {
                let candidate = FallbackProfile {
                    model_class: case["candidateClass"].as_str().map(str::to_string),
                    ..blank("cand")
                };
                let failed_profile = FallbackProfile {
                    model_class: case["failedClass"].as_str().map(str::to_string),
                    ..blank("failed")
                };
                let got = quilltap_core::llm_fallback::tier_matches(&candidate, &failed_profile);
                if Value::Bool(got) != case["result"] {
                    diffs.push(format!(
                        "    tierMatches: rust {got} != oracle {}",
                        case["result"]
                    ));
                }
            }
            "pick" => {
                let failed_profile = profile_from(&case["failed"]);
                let candidates: Vec<FallbackProfile> = case["candidates"]
                    .as_array()
                    .expect("candidates")
                    .iter()
                    .map(profile_from)
                    .collect();
                let context = context_from(&case["context"]);
                let got = pick_tier_candidate(&failed_profile, &candidates, &context);
                let got_id = got
                    .map(|p| Value::String(p.id.clone()))
                    .unwrap_or(Value::Null);
                if got_id != case["pickedId"] {
                    diffs.push(format!(
                        "    pickedId: rust {got_id} != oracle {}",
                        case["pickedId"]
                    ));
                }
            }
            "chain" => {
                let primary = profile_from(&case["primary"]);
                let mut all = vec![primary.clone()];
                all.extend(
                    case["others"]
                        .as_array()
                        .expect("others")
                        .iter()
                        .map(profile_from),
                );
                let repos = MemoryRepos(all);
                let context = context_from(&case["context"]);
                let chain = build_fallback_chain(&primary, &repos, &context);
                let got: Value = Value::Array(
                    chain
                        .iter()
                        .map(|c| serde_json::json!({ "id": c.profile.id, "kind": c.kind.as_str() }))
                        .collect(),
                );
                if got != case["chain"] {
                    diffs.push(format!("    chain: rust {got} != oracle {}", case["chain"]));
                }
            }
            "record" => {
                let profile = profile_from(&case["profile"]);
                let message = record_message(case);
                let got = record_attempt(
                    &profile,
                    trigger_from(case["trigger"].as_str().unwrap_or("provider-error")),
                    message.as_deref(),
                );
                let oracle = attempt_from(&case["attempt"]);
                if got != oracle {
                    diffs.push(format!("    attempt: rust {got:?} != oracle {oracle:?}"));
                }
            }
            "summarize" => {
                let attempts: Vec<FallbackAttempt> = case["attempts"]
                    .as_array()
                    .expect("attempts")
                    .iter()
                    .map(attempt_from)
                    .collect();
                let offered = case["tierPickWasOffered"].as_bool().unwrap_or(false);
                let got = summarize_fallback_attempts(&attempts, offered);
                if Value::String(got.clone()) != case["text"] {
                    diffs.push(format!(
                        "    summary: rust {got:?} != oracle {}",
                        case["text"]
                    ));
                }
            }
            // Bug 116 (v4 `0b0617fee`): the arrival verdict, byte-compared.
            // `{arrived, reason}` — the sentence itself is the comparand,
            // because it reaches the user inside the describer's refusal.
            "verdict" => {
                let response = completion_response_from(&case["response"]);
                let attachment_id = case["attachmentId"].as_str().unwrap_or("");
                let got = quilltap_core::services::file_fallback::verify_image_reached_model(
                    &response,
                    attachment_id,
                );
                if Value::Bool(got.arrived()) != case["arrived"] {
                    diffs.push(format!(
                        "    arrived: rust {} != oracle {}",
                        got.arrived(),
                        case["arrived"]
                    ));
                }
                let got_reason = match got.reason() {
                    Some(r) => Value::String(r.to_string()),
                    None => Value::Null,
                };
                if got_reason != case["reason"] {
                    diffs.push(format!(
                        "    reason: rust {got_reason} != oracle {}",
                        case["reason"]
                    ));
                }
            }
            _ => unreachable!(),
        }

        if !diffs.is_empty() {
            failed.push(format!("{id}:\n{}", diffs.join("\n")));
        }
    }

    assert!(
        failed.is_empty(),
        "{} of {} case(s) failed:\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );

    // Shape assertions, not hand counts: a corpus that lost a whole family
    // would otherwise go green having measured less than it claims.
    for (kind, floor) in [
        ("transport", 10usize),
        ("apiKeyCapability", 10),
        ("classify", 35),
        ("tierMatches", 30),
        ("pick", 15),
        ("chain", 18),
        ("record", 5),
        ("summarize", 6),
        ("verdict", 14),
    ] {
        let n = by_kind.get(kind).copied().unwrap_or(0);
        assert!(
            n >= floor,
            "the corpus lost its `{kind}` rows ({n} < {floor}) — regenerate the oracle"
        );
    }
    eprintln!("OK fallback engine: {} cases ({by_kind:?})", cases.len());
}

/// A minimal profile for the `tierMatches` rows, which only read `modelClass`.
fn blank(id: &str) -> FallbackProfile {
    FallbackProfile {
        id: id.to_string(),
        user_id: "u1".into(),
        name: "Profile".into(),
        provider: "OPENAI".into(),
        model_name: "m".into(),
        base_url: None,
        api_key_id: Some("k1".into()),
        transport: "api".into(),
        is_cheap: false,
        is_dangerous_compatible: false,
        supports_image_upload: false,
        allow_tool_use: true,
        model_class: None,
        sort_index: 0.0,
        fallback_profile_id: None,
        allow_tier_fallback: false,
        parameters: None,
    }
}
