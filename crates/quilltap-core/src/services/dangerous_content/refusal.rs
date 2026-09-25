//! Refusal classification — the one place Quilltap decides whether a provider
//! declined a request on content-moderation grounds (port of v4
//! `lib/services/dangerous-content/refusal.ts`, NEW at `8bd080267`, #73;
//! unchanged by `49059fb14`).
//!
//! Before this module there were five approximations: a six-substring match
//! for images, a finish-reason list for empty text bodies, and nothing at all
//! for a thrown text refusal (which fell through the fallback engine's generic
//! 4xx check and was treated as our own malformed request). Every caller now
//! asks here, and the evidence is ranked by how much it can be trusted:
//!
//!   1. `typed-error`     — the provider layer produced an error carrying
//!      `code: 'MODERATION_REJECTED'` (or named `ModerationRejectionError`).
//!   2. `provider-code`   — a known SDK / provider moderation code on the error.
//!   3. `finish-reason`   — a stated moderation stop reason.
//!   4. `message-pattern` — wording providers use for a refusal. Deliberately
//!      narrow: never a bare "400", never "try a different prompt", both of
//!      which are ambiguous.
//!   5. `inferred`        — an empty body on content the Concierge had flagged.
//!
//! A false positive tells a user their content was refused when it was not,
//! and spends an uncensored call on a rate limit — so anything unrecognised
//! stays unrecognised.
//!
//! ## The structured input (the one shape divergence, ruled at planning — E.2)
//!
//! v4 classifies an `unknown` thrown value: it duck-types `code`, `error.code`,
//! `name`, `providerReason` and `message` off whatever the plugin threw. v5 has
//! no plugins and no `unknown` — its provider errors are Rust types — so the
//! native dialects and decoders fill a [`RefusalError`] where v4's plugins throw
//! `ModerationRejectionError` (and where the OpenAI SDK's `APIError` carried
//! `code`/`error.code`/`status` for free, v5 reads them off the HTTP error body
//! itself). Every JS shape v4 accepts maps onto the struct without loss for
//! the classification: [`code_string`] is v4's `codeString` (a FINITE number
//! becomes its JS string, so Z.AI's numeric `1301` reads as `"1301"`), `code`
//! and `nested_code` are v4's `collectCodes` slots in their order, and a thrown
//! STRING is a `RefusalError` with only `message` set (v4's `asRecord` is null
//! for it, so neither code arm can fire — the same as every `None` here).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The contract between the provider layer and the host (v4
/// `MODERATION_REJECTION_CODE`, the `code` `ModerationRejectionError` carries).
pub const MODERATION_REJECTION_CODE: &str = "MODERATION_REJECTED";

/// The class name, accepted as well so an un-bundled plugin copy still
/// qualifies (v4 `MODERATION_REJECTION_NAME`).
pub const MODERATION_REJECTION_NAME: &str = "ModerationRejectionError";

/// How a refusal was established (v4 `RefusalEvidence`), strongest first.
///
/// These are persisted bytes — the trail's `evidence` on a message row and in a
/// `.qtap` export — so the wire spelling is v4's union member verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefusalEvidence {
    TypedError,
    ProviderCode,
    FinishReason,
    MessagePattern,
    Inferred,
}

impl RefusalEvidence {
    pub fn as_str(self) -> &'static str {
        match self {
            RefusalEvidence::TypedError => "typed-error",
            RefusalEvidence::ProviderCode => "provider-code",
            RefusalEvidence::FinishReason => "finish-reason",
            RefusalEvidence::MessagePattern => "message-pattern",
            RefusalEvidence::Inferred => "inferred",
        }
    }

    /// The inverse of [`Self::as_str`] — `None` for anything v4's enum does not
    /// name.
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "typed-error" => Some(RefusalEvidence::TypedError),
            "provider-code" => Some(RefusalEvidence::ProviderCode),
            "finish-reason" => Some(RefusalEvidence::FinishReason),
            "message-pattern" => Some(RefusalEvidence::MessagePattern),
            "inferred" => Some(RefusalEvidence::Inferred),
            _ => None,
        }
    }
}

/// v4 `RefusalVerdict`. `refused: false` carries NO evidence and NO detail —
/// v4 returns the bare `{ refused: false }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefusalVerdict {
    pub refused: bool,
    pub evidence: Option<RefusalEvidence>,
    /// ≤ 200 UTF-16 units, never the full error body.
    pub detail: Option<String>,
}

/// A provider error as the classifier reads it — the fields v4 duck-types off
/// a thrown value (see the module doc).
///
/// A field, not a variant: it rides as an optional side on `ImageGenError`,
/// `StreamError` and `FallbackError`, defaulting to `None`, so every existing
/// constructor still compiles (the `a-required-field-on-a-shared-struct` rule).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefusalError {
    /// v4 `messageOf(error)` — `''` when the thrown value had none.
    pub message: String,
    /// `record.code` through [`code_string`].
    pub code: Option<String>,
    /// `record.error.code` through [`code_string`] (the OpenAI SDK's nested
    /// copy of the response body's `error.code`).
    pub nested_code: Option<String>,
    /// `record.name` when it is a string.
    pub name: Option<String>,
    /// `record.providerReason` when it is a string.
    pub provider_reason: Option<String>,
    /// The HTTP status, when the provider answered with one. Carried for the
    /// trail's record; the classifier does not rank on it (neither does v4).
    pub status: Option<u16>,
}

impl RefusalError {
    /// An error with only a message — v4's thrown string, or an `Error` with
    /// no moderation fields.
    pub fn message_only(message: impl Into<String>) -> Self {
        RefusalError {
            message: message.into(),
            ..Default::default()
        }
    }

    /// The typed refusal v4's plugins throw (`new ModerationRejectionError(
    /// message, statusCode, providerReason, pluginName)`): `code` is
    /// `MODERATION_REJECTED`, `name` is `ModerationRejectionError`.
    pub fn typed(
        message: impl Into<String>,
        status: Option<u16>,
        provider_reason: Option<String>,
    ) -> Self {
        RefusalError {
            message: message.into(),
            code: Some(MODERATION_REJECTION_CODE.to_string()),
            nested_code: None,
            name: Some(MODERATION_REJECTION_NAME.to_string()),
            provider_reason,
            status,
        }
    }

    /// Read the classifier's fields off a JS-shaped error record — an HTTP
    /// error body, or an SDK error's own properties — exactly as v4's
    /// `asRecord` / `codeString` / `collectCodes` read them. `message` is
    /// supplied by the caller (v4's `messageOf` runs over the thrown value,
    /// which for an SDK error is its own rendering, not the body's).
    pub fn from_record(
        message: impl Into<String>,
        record: &serde_json::Map<String, Value>,
    ) -> Self {
        let nested_code = record
            .get("error")
            .and_then(Value::as_object)
            .and_then(|nested| nested.get("code"))
            .and_then(code_string);
        RefusalError {
            message: message.into(),
            code: record.get("code").and_then(code_string),
            nested_code,
            name: record
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string),
            provider_reason: record
                .get("providerReason")
                .and_then(Value::as_str)
                .map(str::to_string),
            status: record
                .get("status")
                .and_then(Value::as_u64)
                .and_then(|s| u16::try_from(s).ok()),
        }
    }

    /// True when this is the typed refusal (v4's evidence-1 test, on its own).
    pub fn is_typed_refusal(&self) -> bool {
        self.code.as_deref() == Some(MODERATION_REJECTION_CODE)
            || self.name.as_deref() == Some(MODERATION_REJECTION_NAME)
    }
}

/// v4 `RefusalInput`. `empty_body` / `content_was_flagged` are `Option` because
/// v4's are optional and the DEBUG line's bag omits an undefined one.
#[derive(Clone, Copy, Debug, Default)]
pub struct RefusalInput<'a> {
    pub error: Option<&'a RefusalError>,
    pub finish_reason: Option<&'a str>,
    pub empty_body: Option<bool>,
    pub content_was_flagged: Option<bool>,
}

/// Provider / SDK codes that mean a moderation refusal, lower-cased (v4
/// `PROVIDER_MODERATION_CODES`).
///
/// - `moderation_blocked`, `content_policy_violation` — OpenAI Images (gpt-image, DALL-E)
/// - `content_filter` — OpenAI / Azure
/// - `safety` — Google
/// - `1301` — Z.AI (GLM) "sensitive content"
pub const PROVIDER_MODERATION_CODES: [&str; 5] = [
    "moderation_blocked",
    "content_policy_violation",
    "content_filter",
    "safety",
    "1301",
];

/// Message fragments that state a refusal, lower-cased (v4
/// `REFUSAL_MESSAGE_PATTERNS`). The first six are the historical
/// `isImageModerationError` list, in its order; the rest cover Google's
/// Responsible AI filter, OpenRouter's "declined to generate", and Gemini's
/// block reasons as they appear in error text.
pub const REFUSAL_MESSAGE_PATTERNS: [&str; 11] = [
    "content moderation",
    "content_policy",
    "content policy",
    "safety system",
    "rejected by content",
    "moderation_blocked",
    "responsible ai",
    "declined to generate",
    "blocked by safety",
    "prompt_blocked",
    "image_safety",
];

const DETAIL_MAX: usize = 200;

/// v4 `truncate` — trim, then cut a long result to 199 UTF-16 units plus `…`.
/// Unlike the route trail's `truncate_detail`, an empty result is kept (v4's
/// classifier returns `''` rather than dropping it).
fn truncate(text: &str) -> String {
    let trimmed = crate::jsstr::js_trim(text);
    if crate::jsstr::utf16_len(trimmed) > DETAIL_MAX {
        format!("{}…", crate::jsstr::utf16_truncate(trimmed, DETAIL_MAX - 1))
    } else {
        trimmed.to_string()
    }
}

/// v4 `codeString`: a non-empty string is itself; a FINITE number is its JS
/// string (`String(v)`); anything else is no code.
pub fn code_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => n.as_f64().and_then(code_string_number),
        _ => None,
    }
}

/// The number arm of [`code_string`] on its own — a JSON number is always
/// finite, so NaN/Infinity (which JS can carry on a thrown object) reach this
/// only from a caller that holds a raw `f64`.
pub fn code_string_number(n: f64) -> Option<String> {
    if n.is_finite() {
        Some(crate::pascal::js_value::number_to_string(n))
    } else {
        None
    }
}

fn classify(input: &RefusalInput<'_>) -> RefusalVerdict {
    if let Some(error) = input.error {
        let message = error.message.as_str();

        // 1. The provider layer said so.
        if error.is_typed_refusal() {
            let detail = match error.provider_reason.as_deref() {
                // `reason ? … : …` — an EMPTY providerReason is falsy.
                Some(reason) if !reason.is_empty() => format!("{message} ({reason})"),
                _ if !message.is_empty() => message.to_string(),
                _ => "moderation rejection".to_string(),
            };
            return RefusalVerdict {
                refused: true,
                evidence: Some(RefusalEvidence::TypedError),
                detail: Some(truncate(&detail)),
            };
        }

        // 2. A known provider code — `record.code` THEN `record.error.code`,
        //    compared lower-cased, the ORIGINAL case in the detail.
        let moderation_code = [error.code.as_deref(), error.nested_code.as_deref()]
            .into_iter()
            .flatten()
            .find(|c| PROVIDER_MODERATION_CODES.contains(&c.to_lowercase().as_str()));
        if let Some(code) = moderation_code {
            let detail = if message.is_empty() {
                format!("code {code}")
            } else {
                format!("code {code}: {message}")
            };
            return RefusalVerdict {
                refused: true,
                evidence: Some(RefusalEvidence::ProviderCode),
                detail: Some(truncate(&detail)),
            };
        }
    }

    // 3. A stated moderation stop — with or without an error, and BEFORE the
    //    message patterns. The detail is the RAW reason, not truncated.
    if crate::moderation_finish_reason::is_moderation_finish_reason(input.finish_reason) {
        return RefusalVerdict {
            refused: true,
            evidence: Some(RefusalEvidence::FinishReason),
            detail: Some(format!(
                "finish_reason: {}",
                input.finish_reason.unwrap_or_default()
            )),
        };
    }

    // 4. Refusal wording in the error text. `lowered &&` short-circuits on the
    //    empty string.
    if let Some(error) = input.error {
        let lowered = error.message.to_lowercase();
        if !lowered.is_empty() && REFUSAL_MESSAGE_PATTERNS.iter().any(|p| lowered.contains(p)) {
            return RefusalVerdict {
                refused: true,
                evidence: Some(RefusalEvidence::MessagePattern),
                detail: Some(truncate(&error.message)),
            };
        }
    }

    // 5. Inference: nothing came back for content the Concierge had flagged.
    if input.empty_body == Some(true) && input.content_was_flagged == Some(true) {
        return RefusalVerdict {
            refused: true,
            evidence: Some(RefusalEvidence::Inferred),
            detail: Some("empty response on content the Concierge had flagged".to_string()),
        };
    }

    RefusalVerdict::default()
}

/// Decide whether a call was refused on content-moderation grounds (v4
/// `classifyRefusal`).
///
/// First hit wins, in the order of trust documented on the module. Pure apart
/// from logging: the DEBUG line fires on EVERY classification and the INFO
/// line on every refusal, so every caller's capture sees them.
pub fn classify_refusal(input: RefusalInput<'_>) -> RefusalVerdict {
    let verdict = classify(&input);

    tracing::debug!(
        target: "quilltap::concierge_refusal",
        refused = verdict.refused,
        evidence = verdict.evidence.map(RefusalEvidence::as_str),
        has_error = input.error.is_some(),
        finish_reason = input.finish_reason,
        empty_body = input.empty_body,
        content_was_flagged = input.content_was_flagged,
        "Classified a provider outcome for refusal"
    );
    if verdict.refused {
        tracing::info!(
            target: "quilltap::concierge_refusal",
            evidence = verdict.evidence.map(RefusalEvidence::as_str),
            detail = verdict.detail.as_deref(),
            "Provider refused on content-moderation grounds"
        );
    }

    verdict
}

/// Shorthand for the common question at a catch site (v4
/// `isModerationRefusal`).
pub fn is_moderation_refusal(error: &RefusalError) -> bool {
    classify_refusal(RefusalInput {
        error: Some(error),
        ..Default::default()
    })
    .refused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict_of(error: RefusalError) -> RefusalVerdict {
        classify_refusal(RefusalInput {
            error: Some(&error),
            ..Default::default()
        })
    }

    #[test]
    fn typed_error_outranks_everything() {
        let v = verdict_of(RefusalError {
            message: "safety system said no".into(),
            code: Some(MODERATION_REJECTION_CODE.into()),
            nested_code: Some("content_filter".into()),
            provider_reason: Some("moderation_blocked".into()),
            ..Default::default()
        });
        assert_eq!(v.evidence, Some(RefusalEvidence::TypedError));
        assert_eq!(
            v.detail.as_deref(),
            Some("safety system said no (moderation_blocked)")
        );
    }

    #[test]
    fn a_name_match_with_no_code_is_typed() {
        let v = verdict_of(RefusalError {
            message: String::new(),
            name: Some(MODERATION_REJECTION_NAME.into()),
            ..Default::default()
        });
        assert_eq!(v.evidence, Some(RefusalEvidence::TypedError));
        assert_eq!(v.detail.as_deref(), Some("moderation rejection"));
    }

    #[test]
    fn provider_code_keeps_the_original_case_in_the_detail() {
        let v = verdict_of(RefusalError {
            message: "nope".into(),
            nested_code: Some("Moderation_Blocked".into()),
            ..Default::default()
        });
        assert_eq!(v.evidence, Some(RefusalEvidence::ProviderCode));
        assert_eq!(v.detail.as_deref(), Some("code Moderation_Blocked: nope"));
    }

    #[test]
    fn finish_reason_runs_before_message_patterns_and_without_an_error() {
        let err = RefusalError::message_only("the safety system declined");
        let v = classify_refusal(RefusalInput {
            error: Some(&err),
            finish_reason: Some("content_filter"),
            ..Default::default()
        });
        assert_eq!(v.evidence, Some(RefusalEvidence::FinishReason));
        let v = classify_refusal(RefusalInput {
            finish_reason: Some("SAFETY"),
            ..Default::default()
        });
        assert_eq!(v.detail.as_deref(), Some("finish_reason: SAFETY"));
    }

    #[test]
    fn inferred_needs_both_flags_and_refused_false_is_bare() {
        let v = classify_refusal(RefusalInput {
            empty_body: Some(true),
            content_was_flagged: Some(true),
            ..Default::default()
        });
        assert_eq!(v.evidence, Some(RefusalEvidence::Inferred));
        let v = classify_refusal(RefusalInput {
            empty_body: Some(true),
            ..Default::default()
        });
        assert_eq!(v, RefusalVerdict::default());
    }

    #[test]
    fn code_string_formats_finite_numbers_like_js() {
        assert_eq!(
            code_string(&serde_json::json!(1301)).as_deref(),
            Some("1301")
        );
        assert_eq!(code_string(&serde_json::json!(1.5)).as_deref(), Some("1.5"));
        assert_eq!(code_string(&serde_json::json!("")), None);
        assert_eq!(code_string_number(f64::NAN), None);
        assert_eq!(code_string_number(f64::INFINITY), None);
    }

    #[test]
    fn evidence_wire_spellings_round_trip() {
        for e in [
            RefusalEvidence::TypedError,
            RefusalEvidence::ProviderCode,
            RefusalEvidence::FinishReason,
            RefusalEvidence::MessagePattern,
            RefusalEvidence::Inferred,
        ] {
            assert_eq!(RefusalEvidence::from_wire(e.as_str()), Some(e));
            assert_eq!(
                serde_json::to_value(e).unwrap(),
                serde_json::json!(e.as_str())
            );
        }
        assert_eq!(RefusalEvidence::from_wire("made-up"), None);
    }
}
