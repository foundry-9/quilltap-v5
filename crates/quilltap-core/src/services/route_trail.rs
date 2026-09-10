//! Route Trail — the call sheet for one assistant turn (v4 `5841a8c62`,
//! `lib/services/chat-message/route-trail.ts`).
//!
//! Every connection profile that was *tried* for a message, in the order tried,
//! with why each one stepped aside. This module is the ONLY writer of
//! [`StreamingState::route_failures`] / [`StreamingState::route_via`] and the
//! only composer of the persisted `routeTrail` column: failures are recorded
//! where they happen (the failover service), and the answering row is composed
//! once at finalization, because the primary's "success" is only known after the
//! empty-body check has run.
//!
//! The trail deliberately overlaps `FallbackChainResult.attempts` /
//! `summarize_fallback_attempts`: those are the per-walk transient that feeds the
//! user-facing error text, this is the per-message persisted record. The
//! alternative would be threading chat state into the provider-layer fallback
//! engine, which must stay ignorant of it.
//!
//! ## Two shape divergences from v4, both structural
//!
//! 1. v4's writer takes a whole `ConnectionProfile`; v5's callers hold two
//!    different species (the failover's [`EffectiveProfile`] and the chain
//!    engine's `FallbackProfile`), so the four fields an entry actually needs
//!    travel as the borrowed [`RouteSeat`] view. Nothing is read that v4 does
//!    not read.
//! 2. v4's `buildRouteTrail(state)` reads the answering seat off
//!    `state.effectiveProfile`, which is non-optional there. v5's
//!    `StreamingState.effective_profile` is an `Option` and the finalizer holds
//!    a narrower projection than the whole state, so the composition is split:
//!    [`compose_route_trail`] takes the three pieces (failures, via, seat) and
//!    carries v4's debug line, and [`build_route_trail`] is the one-line
//!    `&StreamingState` adapter the preserve-partial path and the differential
//!    drive.

use serde::{Serialize, Serializer};

use crate::llm_fallback::{FallbackCandidateKind, FallbackTrigger};
use crate::services::primary_stream::{EffectiveProfile, StreamingState};

/// `detail` is a short reason for a human, never the full error body — it can
/// run long and can carry fragments of the request.
const DETAIL_MAX: usize = 200;

/// How a seat came to be asked (v4 `RouteAttemptViaEnum`).
///
/// - `Primary` — the profile the chat was configured with (or the one the
///   Concierge's *pre-call* reroute installed before anything was tried).
/// - `Retry` — the same profile, asked a second time after an empty body.
/// - `Concierge` — the uncensored profile the Concierge sent the turn to after
///   the first answer came back empty.
/// - `Understudy` — the profile's own named `fallbackProfileId`.
/// - `TierPick` — a stand-in the fallback engine drafted from the company.
///
/// `Default` is `Primary`: `StreamingState` derives `Default`, and v4's
/// `processMessage` seeds `routeVia: 'primary'` on every turn the Concierge did
/// not pre-empt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouteAttemptVia {
    #[default]
    Primary,
    Retry,
    Concierge,
    Understudy,
    TierPick,
}

impl RouteAttemptVia {
    /// The wire spelling — v4's union members verbatim. These are persisted
    /// bytes and reach the client, so they are bytes, not labels.
    pub fn as_str(self) -> &'static str {
        match self {
            RouteAttemptVia::Primary => "primary",
            RouteAttemptVia::Retry => "retry",
            RouteAttemptVia::Concierge => "concierge",
            RouteAttemptVia::Understudy => "understudy",
            RouteAttemptVia::TierPick => "tier-pick",
        }
    }
}

/// What became of one attempt (v4 `RouteAttemptOutcomeEnum`). `Failed` means the
/// profile fell over on its own (timeout, auth, rate limit, 5xx, an empty body
/// with no stated reason); `Refused` means it declined on content grounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteAttemptOutcome {
    Answered,
    Failed,
    Refused,
}

impl RouteAttemptOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            RouteAttemptOutcome::Answered => "answered",
            RouteAttemptOutcome::Failed => "failed",
            RouteAttemptOutcome::Refused => "refused",
        }
    }
}

/// How a refusal was established: the provider said so, or it was inferred from
/// an empty body on a Concierge-flagged turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteAttemptEvidence {
    FinishReason,
    Inferred,
}

impl RouteAttemptEvidence {
    pub fn as_str(self) -> &'static str {
        match self {
            RouteAttemptEvidence::FinishReason => "finish-reason",
            RouteAttemptEvidence::Inferred => "inferred",
        }
    }
}

fn ser_via<S: Serializer>(v: &RouteAttemptVia, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(v.as_str())
}
fn ser_outcome<S: Serializer>(v: &RouteAttemptOutcome, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(v.as_str())
}
fn ser_trigger<S: Serializer>(v: &Option<FallbackTrigger>, s: S) -> Result<S::Ok, S::Error> {
    // Only reached when the key is present (`skip_serializing_if`) — but a
    // persistence-path serializer must not be one attribute away from a panic.
    match v {
        Some(t) => s.serialize_str(t.as_str()),
        None => s.serialize_none(),
    }
}
fn ser_evidence<S: Serializer>(v: &Option<RouteAttemptEvidence>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(e) => s.serialize_str(e.as_str()),
        None => s.serialize_none(),
    }
}

/// One call (or one skipped-before-calling candidate) against one connection
/// profile, as recorded on an assistant message's `routeTrail`.
///
/// `profile_id` is stored for the record and is never dereferenced by the UI: a
/// profile can be deleted, and an imported trail may name one that never existed
/// in this instance. The name and model are what a reader needs. (It is also the
/// reason the import remap deliberately leaves it alone — P4.D171's pin.)
///
/// **Absent keys are ABSENT, not null.** v4 spreads `...(evidence ? {evidence} :
/// {})`, so a failure with no evidence carries no `evidence` key at all, and the
/// answering entry carries none of the three. Key presence is a comparand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteAttempt {
    pub profile_id: String,
    pub profile_name: String,
    pub provider: String,
    pub model_name: String,
    #[serde(serialize_with = "ser_via")]
    pub via: RouteAttemptVia,
    #[serde(serialize_with = "ser_outcome")]
    pub outcome: RouteAttemptOutcome,
    /// The engine's trigger class for a failure or refusal; absent when answered.
    ///
    /// v4 duplicates `FallbackTrigger` BY VALUE in a client-safe module and
    /// keeps the two unions identical with a parity assertion. v5 has one enum
    /// and reuses it, so the two cannot drift — the runtime half of v4's
    /// assertion is `trigger_union_matches_v4_route_attempt_schema` below.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_trigger"
    )]
    pub trigger: Option<FallbackTrigger>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "ser_evidence"
    )]
    pub evidence: Option<RouteAttemptEvidence>,
    /// Short human-readable reason, ≤ 200 UTF-16 units (the error message
    /// truncated, or the finish reason). Never the full error body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The four connection-profile fields a trail entry names, borrowed.
///
/// v5's two profile species — the failover's [`EffectiveProfile`] and the chain
/// engine's `FallbackProfile` — both project to this; v4 simply passes the whole
/// `ConnectionProfile`.
#[derive(Clone, Copy, Debug)]
pub struct RouteSeat<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub provider: &'a str,
    pub model_name: &'a str,
}

impl<'a> From<&'a EffectiveProfile> for RouteSeat<'a> {
    fn from(p: &'a EffectiveProfile) -> Self {
        RouteSeat {
            id: &p.id,
            name: &p.name,
            provider: &p.provider,
            model_name: &p.model_name,
        }
    }
}

impl<'a> From<&'a crate::llm_fallback::FallbackProfile> for RouteSeat<'a> {
    fn from(p: &'a crate::llm_fallback::FallbackProfile) -> Self {
        RouteSeat {
            id: &p.id,
            name: &p.name,
            provider: &p.provider,
            model_name: &p.model_name,
        }
    }
}

/// v4 `truncateDetail`: trim, drop an empty result, and cut a long one to 199
/// UTF-16 units plus U+2026 (200 total).
///
/// `String.prototype.slice` counts UTF-16 units, so an astral character
/// straddling the boundary is split into a lone surrogate in v4. `utf16_truncate`
/// is the ported twin: it takes the same UTF-16 prefix and answers U+FFFD for
/// the split pair (`String::from_utf16_lossy`), which is the only representable
/// Rust — the corpus carries an astral-boundary row so the divergence, if any,
/// is measured rather than assumed).
pub fn truncate_detail(detail: Option<&str>) -> Option<String> {
    let detail = detail?;
    let trimmed = crate::jsstr::js_trim(detail);
    if trimmed.is_empty() {
        return None;
    }
    if crate::jsstr::utf16_len(trimmed) > DETAIL_MAX {
        Some(format!(
            "{}…",
            crate::jsstr::utf16_truncate(trimmed, DETAIL_MAX - 1)
        ))
    } else {
        Some(trimmed.to_string())
    }
}

/// How a chain candidate came to be offered, in the trail's vocabulary (v4
/// `viaOf`).
pub fn via_of(kind: FallbackCandidateKind) -> RouteAttemptVia {
    match kind {
        FallbackCandidateKind::Configured => RouteAttemptVia::Understudy,
        FallbackCandidateKind::TierPick => RouteAttemptVia::TierPick,
        // Unreachable inside a walk — the failing profile is always in
        // `already_tried` — but the mapping is total so a future caller is safe.
        FallbackCandidateKind::Primary => RouteAttemptVia::Primary,
    }
}

/// Append a failed or refused attempt to the turn's trail (v4
/// `recordRouteFailure`).
///
/// Call it at the point the failure is already being logged; the seat is whoever
/// was actually asked, which for the Concierge's uncensored reroute is NOT
/// `state.effective_profile` (that swap only happens on success).
pub fn record_route_failure<'a>(
    state: &mut StreamingState,
    seat: impl Into<RouteSeat<'a>>,
    via: RouteAttemptVia,
    outcome: RouteAttemptOutcome,
    trigger: FallbackTrigger,
    detail: Option<&str>,
    evidence: Option<RouteAttemptEvidence>,
) {
    let seat: RouteSeat<'a> = seat.into();
    let attempt = RouteAttempt {
        profile_id: seat.id.to_string(),
        profile_name: seat.name.to_string(),
        provider: seat.provider.to_string(),
        model_name: seat.model_name.to_string(),
        via,
        outcome,
        trigger: Some(trigger),
        evidence,
        detail: truncate_detail(detail),
    };
    state.route_failures.push(attempt);
    tracing::debug!(
        target: "quilltap::route_trail",
        profile_id = %seat.id,
        profile_name = %seat.name,
        provider = %seat.provider,
        model = %seat.model_name,
        via = via.as_str(),
        outcome = outcome.as_str(),
        trigger = trigger.as_str(),
        evidence = evidence.map(RouteAttemptEvidence::as_str),
        failures_so_far = state.route_failures.len(),
        "Recorded a route-trail failure"
    );
}

/// Tag how `state.effective_profile` came to hold the turn (v4 `setRouteVia`).
/// Set beside every swap.
pub fn set_route_via(state: &mut StreamingState, via: RouteAttemptVia) {
    state.route_via = via;
    // v4 logs `state.effectiveProfile.id`/`.name` unconditionally; v5's field is
    // an `Option`, so an unset seat logs empty strings rather than panicking.
    let (id, name) = state
        .effective_profile
        .as_ref()
        .map(|p| (p.id.as_str(), p.name.as_str()))
        .unwrap_or(("", ""));
    tracing::debug!(
        target: "quilltap::route_trail",
        profile_id = %id,
        profile_name = %name,
        via = via.as_str(),
        "Route trail: the answering seat changed hands"
    );
}

/// The verdict on a call that produced nothing (v4 `EmptyBodyVerdict`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmptyBodyVerdict {
    pub outcome: RouteAttemptOutcome,
    pub trigger: FallbackTrigger,
    pub evidence: Option<RouteAttemptEvidence>,
    pub detail: Option<String>,
}

/// Classify an empty body (v4 `classifyEmptyBody`).
///
/// A provider that named a moderation stop is testimony: a stated refusal. An
/// empty body on a turn the Concierge had already flagged is the existing code's
/// own reading — it skips the same-profile retry on exactly that basis — so it
/// is recorded as an *inferred* refusal. Anything else is a plain empty
/// response, which is usually transient.
///
/// **Must be called before `reset_streaming_buffers_for_swap`**, which clears
/// `state.raw_response` — the finish reason lives in there.
pub fn classify_empty_body(
    state: &StreamingState,
    content_was_flagged_dangerous: bool,
) -> EmptyBodyVerdict {
    // v4 passes `state.rawResponse`, which may be `undefined`;
    // `extractFinishReason` answers null for anything not an object.
    let raw = state
        .raw_response
        .clone()
        .unwrap_or(serde_json::Value::Null);
    let finish_reason = crate::finish_reason::extract_finish_reason(&raw);

    if crate::moderation_finish_reason::is_moderation_finish_reason(finish_reason.as_deref()) {
        return EmptyBodyVerdict {
            outcome: RouteAttemptOutcome::Refused,
            trigger: FallbackTrigger::ModerationRefusal,
            evidence: Some(RouteAttemptEvidence::FinishReason),
            detail: Some(format!(
                "finish_reason: {}",
                finish_reason.unwrap_or_default()
            )),
        };
    }

    if content_was_flagged_dangerous {
        return EmptyBodyVerdict {
            outcome: RouteAttemptOutcome::Refused,
            trigger: FallbackTrigger::ModerationRefusal,
            evidence: Some(RouteAttemptEvidence::Inferred),
            detail: Some("empty response on content the Concierge had flagged".to_string()),
        };
    }

    // No detail when the provider said nothing about why: "fell over:
    // empty-response (empty response)" is a tooltip repeating itself.
    //
    // v4's `...(finishReason ? {detail} : {})` is JS truthiness, so an EMPTY
    // finish-reason string omits the key exactly as a null one does.
    EmptyBodyVerdict {
        outcome: RouteAttemptOutcome::Failed,
        trigger: FallbackTrigger::EmptyResponse,
        evidence: None,
        detail: finish_reason
            .filter(|r| !r.is_empty())
            .map(|r| format!("finish_reason: {r}")),
    }
}

/// Where a composed trail came from, for the debug line (v4's `logContext`).
#[derive(Clone, Copy, Debug, Default)]
pub struct RouteTrailLogContext<'a> {
    pub chat_id: Option<&'a str>,
    pub message_id: Option<&'a str>,
}

/// The value to persist: NULL when nothing failed, else every failure in order
/// followed by the seat that answered (v4 `buildRouteTrail`'s body).
///
/// NULL is the whole point of the design — a one-entry trail says nothing the
/// `provider`/`model_name` columns already say, and assistant rows are the
/// largest table in the instance.
pub fn compose_route_trail<'a>(
    failures: &[RouteAttempt],
    via: RouteAttemptVia,
    seat: impl Into<RouteSeat<'a>>,
    log_context: RouteTrailLogContext<'_>,
) -> Option<Vec<RouteAttempt>> {
    if failures.is_empty() {
        return None;
    }
    let seat: RouteSeat<'a> = seat.into();
    let answered = RouteAttempt {
        profile_id: seat.id.to_string(),
        profile_name: seat.name.to_string(),
        provider: seat.provider.to_string(),
        model_name: seat.model_name.to_string(),
        via,
        outcome: RouteAttemptOutcome::Answered,
        trigger: None,
        evidence: None,
        detail: None,
    };

    let mut trail = failures.to_vec();
    trail.push(answered);

    tracing::debug!(
        target: "quilltap::route_trail",
        chat_id = log_context.chat_id,
        message_id = log_context.message_id,
        length = trail.len(),
        trail = %serde_json::to_string(
            &trail
                .iter()
                .map(|a| serde_json::json!({
                    "profileName": a.profile_name,
                    "via": a.via.as_str(),
                    "outcome": a.outcome.as_str(),
                    "trigger": a.trigger.map(FallbackTrigger::as_str),
                }))
                .collect::<Vec<_>>()
        )
        .unwrap_or_default(),
        "Composed a route trail for the turn"
    );

    Some(trail)
}

/// v4 `buildRouteTrail(state, logContext)` — the `&StreamingState` adapter over
/// [`compose_route_trail`].
///
/// An unset `effective_profile` (v5's `Option`; v4's field is non-optional)
/// composes an answering entry of empty strings rather than dropping the
/// failures on the floor — a silent drop is the class of defect dogfood findings
/// #103/#110 were both about, and the row's own `provider`/`modelName` are
/// written from the same absent value.
pub fn build_route_trail(
    state: &StreamingState,
    log_context: RouteTrailLogContext<'_>,
) -> Option<Vec<RouteAttempt>> {
    const UNSET: RouteSeat<'static> = RouteSeat {
        id: "",
        name: "",
        provider: "",
        model_name: "",
    };
    let seat = state
        .effective_profile
        .as_ref()
        .map_or(UNSET, RouteSeat::from);
    compose_route_trail(&state.route_failures, state.route_via, seat, log_context)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's `engine.test.ts` parity assertion, runtime half: every
    /// `FallbackTrigger` is a legal `RouteAttemptSchema.trigger` value, and
    /// nothing else is.
    ///
    /// The compile-time half is free here: v5 has ONE enum, reused by both the
    /// engine and the trail, so the unions cannot drift apart at all.
    #[test]
    fn trigger_union_matches_v4_route_attempt_schema() {
        // v4 `RouteAttemptSchema.shape.trigger`'s member list, verbatim.
        const V4_TRIGGERS: [&str; 7] = [
            "auth",
            "rate-limit",
            "network",
            "model-missing",
            "provider-error",
            "empty-response",
            "moderation-refusal",
        ];
        let ours: Vec<&str> = [
            FallbackTrigger::Auth,
            FallbackTrigger::RateLimit,
            FallbackTrigger::Network,
            FallbackTrigger::ModelMissing,
            FallbackTrigger::ProviderError,
            FallbackTrigger::EmptyResponse,
            FallbackTrigger::ModerationRefusal,
        ]
        .iter()
        .map(|t| t.as_str())
        .collect();
        assert_eq!(ours, V4_TRIGGERS.to_vec());
        assert!(!V4_TRIGGERS.contains(&"made-up"));
    }
}
