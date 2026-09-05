//! Tier-1 differential test (W4.7d): the LLM provider error taxonomy, with
//! regression rows for the already-ported predicates.
//!
//! Drives v4's REAL `lib/llm/errors.ts` exports (see the oracle case) and diffs
//! byte-for-byte: the constructor default messages / `retryAfter` /
//! content-value formatting; and every predicate output.
//!
//! P4.D157: v4 `d4138b96b` deleted `handleProviderError` / `getUserFriendlyError`;
//! every v5 caller of either lived in `llm_errors.rs`'s own `#[cfg(test)]`
//! module, so both twins were deleted here. The `handle` rows left the case and
//! the `construct` rows dropped their `userFriendly` field (54 -> 30 rows; every
//! surviving field byte-identical).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/llm-errors.ts \
//!     > /tmp/oracle-llm-errors.ndjson
//! Run:
//!   QT_ORACLE_LLM_ERRORS=/tmp/oracle-llm-errors.ndjson \
//!     cargo test -p quilltap-harness --test llm_errors_equivalence

use quilltap_core::services::llm_errors::LlmProviderError;
use quilltap_core::services::primary_stream::{
    is_content_limit_error, is_recoverable_request_error, is_token_limit_error,
    is_tool_unsupported_error, parse_content_limit_error, parse_token_limit_error,
    ContentLimitType,
};
use serde_json::Value;

fn opt_i64(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

fn content_type_str(t: ContentLimitType) -> &'static str {
    match t {
        ContentLimitType::Token => "token",
        ContentLimitType::PdfPages => "pdf_pages",
        ContentLimitType::ImageSize => "image_size",
        ContentLimitType::FileSize => "file_size",
        ContentLimitType::Unknown => "unknown",
    }
}

fn content_type_from_str(s: &str) -> ContentLimitType {
    match s {
        "token" => ContentLimitType::Token,
        "pdf_pages" => ContentLimitType::PdfPages,
        "image_size" => ContentLimitType::ImageSize,
        "file_size" => ContentLimitType::FileSize,
        _ => ContentLimitType::Unknown,
    }
}

/// Build the LlmProviderError a `construct` row's `spec` describes.
fn build_from_spec(spec: &Value) -> LlmProviderError {
    let provider = spec.get("provider").and_then(Value::as_str).unwrap_or("");
    let msg = spec
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string);
    match spec.get("ctor").and_then(Value::as_str).unwrap_or("") {
        "apiKey" => LlmProviderError::api_key(provider),
        "rateLimit" => LlmProviderError::rate_limit(provider, opt_i64(spec, "retryAfter")),
        "network" => LlmProviderError::network(provider, msg),
        "modelNotFound" => LlmProviderError::model_not_found(
            provider,
            spec.get("model").and_then(Value::as_str).unwrap_or(""),
        ),
        "invalidRequest" => LlmProviderError::invalid_request(provider, msg.unwrap_or_default()),
        "tokenLimit" => LlmProviderError::token_limit(
            provider,
            opt_i64(spec, "requestedTokens"),
            opt_i64(spec, "maxTokens"),
            msg,
        ),
        "contentLimit" => LlmProviderError::content_limit(
            provider,
            content_type_from_str(
                spec.get("limitType")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
            ),
            opt_i64(spec, "limitValue"),
            opt_i64(spec, "maxValue"),
            msg,
        ),
        "base" => LlmProviderError::base(provider, msg.unwrap_or_default()),
        other => panic!("unknown ctor {other}"),
    }
}

#[test]
fn llm_errors_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_LLM_ERRORS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_LLM_ERRORS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    // Per-kind counters: a single total cannot tell a whole vanished kind from a
    // shrunk oracle (P4.D157 split this case; the guard now sees each half).
    let mut counts = [0usize; 2]; // [construct, predicate]
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let id = row.get("id").and_then(Value::as_str).unwrap_or("?");
        match row.get("kind").and_then(Value::as_str).unwrap_or("") {
            "construct" => {
                let e = build_from_spec(&row["spec"]);
                assert_eq!(
                    e.kind.name(),
                    row["name"].as_str().unwrap(),
                    "ctor name '{id}'"
                );
                assert_eq!(
                    e.message,
                    row["message"].as_str().unwrap(),
                    "ctor message '{id}'"
                );
                counts[0] += 1;
            }
            "predicate" => {
                let input = row["input"].as_str().unwrap();
                assert_eq!(
                    is_token_limit_error(input),
                    row["isToken"].as_bool().unwrap(),
                    "isToken '{id}'"
                );
                assert_eq!(
                    is_content_limit_error(input),
                    row["isContent"].as_bool().unwrap(),
                    "isContent '{id}'"
                );
                assert_eq!(
                    is_tool_unsupported_error(input),
                    row["isToolUnsupported"].as_bool().unwrap(),
                    "isToolUnsupported '{id}'"
                );
                assert_eq!(
                    is_recoverable_request_error(input),
                    row["isRecoverable"].as_bool().unwrap(),
                    "isRecoverable '{id}'"
                );
                let (req, max) = parse_token_limit_error(input);
                assert_eq!(req, opt_i64(&row, "parseTokenReq"), "parseTokenReq '{id}'");
                assert_eq!(max, opt_i64(&row, "parseTokenMax"), "parseTokenMax '{id}'");
                let (ct, cmax, cdesc) = parse_content_limit_error(input);
                assert_eq!(
                    content_type_str(ct),
                    row["parseContentType"].as_str().unwrap(),
                    "parseContentType '{id}'"
                );
                assert_eq!(
                    cmax,
                    opt_i64(&row, "parseContentMax"),
                    "parseContentMax '{id}'"
                );
                let want_desc = row
                    .get("parseContentDesc")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                assert_eq!(cdesc, want_desc, "parseContentDesc '{id}'");
                counts[1] += 1;
            }
            other => panic!("unknown row kind {other}"),
        }
        count += 1;
    }

    assert!(
        counts.iter().all(|&c| c > 0),
        "oracle file looks empty/partial: {counts:?}"
    );
    eprintln!("OK: llm-errors matched oracle ({count} rows, counts {counts:?}).");
}
