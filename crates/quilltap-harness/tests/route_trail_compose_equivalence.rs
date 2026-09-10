//! Tier-1 differential (P4.D173): the route-trail chokepoint —
//! `quilltap_core::services::route_trail` vs v4's REAL
//! `lib/services/chat-message/route-trail.ts` exports (`5841a8c62`).
//!
//! Covers `viaOf` (all three candidate kinds), `classifyEmptyBody`'s whole
//! ladder (the moderation arm through every provider shape
//! `extractFinishReason` knows, its trim/case folding, its precedence over the
//! flagged inference; the inferred arm; the plain empty-response arm incl. the
//! JS-truthiness omission of `detail` for an EMPTY finish-reason string and the
//! probe ORDER between the four provider shapes), `recordRouteFailure` (the
//! pushed entry's exact JSON — key order AND key presence, since v4 spreads
//! `...(evidence ? {evidence} : {})` and an absent optional is genuinely absent,
//! never null — plus `truncateDetail`'s whole matrix through its only caller:
//! undefined / empty / whitespace-only / trimmed / 199 / 200 / 201 / an outer
//! trim that lands exactly on 200 / astral at the 200-unit boundary),
//! `setRouteVia`, `buildRouteTrail`'s NULL rule and `[...failures, answered]`
//! composition (the answering entry carrying none of the three optionals and
//! always naming `state.effectiveProfile`), and the orchestrator's `routeVia`
//! seeding expression in both arms.
//!
//! **ONE RECORDED DIVERGENCE, pinned in BOTH directions** (`detail-astral-*`):
//! v4's `truncateDetail` cuts with `String.prototype.slice(0, 199)`, which
//! counts UTF-16 units, so a cut that lands INSIDE a surrogate pair leaves v4
//! holding a lone high surrogate — an ill-formed string that JS carries happily
//! and `JSON.stringify` emits as `\ud83d`. Rust has no representation for one:
//! `String::from_utf16_lossy` answers U+FFFD. The rows assert v4 still emits the
//! lone surrogate AND that v5's answer is v4's with exactly that escape replaced
//! by U+FFFD — so a v4 fix (or a v5 drift) trips this immediately.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/route-trail-compose.ts \
//!     > /tmp/oracle-route-trail-compose.ndjson
//! Run:
//!   QT_ORACLE_ROUTE_TRAIL=/tmp/oracle-route-trail-compose.ndjson \
//!     cargo test -p quilltap-harness --test route_trail_compose_equivalence

use quilltap_core::llm_fallback::{FallbackCandidateKind, FallbackTrigger};
use quilltap_core::services::primary_stream::{EffectiveProfile, StreamingState};
use quilltap_core::services::route_trail::{
    build_route_trail, classify_empty_body, record_route_failure, set_route_via, via_of,
    RouteAttempt, RouteAttemptEvidence, RouteAttemptOutcome, RouteAttemptVia, RouteTrailLogContext,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Seat {
    id: String,
    name: String,
    provider: String,
    model_name: String,
}

impl Seat {
    fn to_effective(&self) -> EffectiveProfile {
        EffectiveProfile {
            id: self.id.clone(),
            name: self.name.clone(),
            provider: self.provider.clone(),
            model_name: self.model_name.clone(),
            base_url: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClassifyOut {
    outcome: String,
    trigger: String,
    #[serde(default)]
    evidence: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum OracleRow {
    #[serde(rename = "viaOf")]
    ViaOf {
        id: String,
        candidate_kind: String,
        out: String,
    },
    Classify {
        id: String,
        #[serde(default)]
        raw_response: Option<Value>,
        content_was_flagged_dangerous: bool,
        out: ClassifyOut,
    },
    Record {
        id: String,
        seat: Seat,
        via: String,
        outcome: String,
        trigger: String,
        #[serde(default)]
        detail: Option<String>,
        #[serde(default)]
        evidence: Option<String>,
        out: String,
        failures_after: usize,
    },
    Compose {
        id: String,
        failures: Vec<Value>,
        route_via: String,
        seat: Seat,
        out: Option<String>,
    },
    #[serde(rename = "setVia")]
    SetVia { id: String, to: String, out: String },
    Seed {
        id: String,
        danger_profile_id: String,
        connection_profile_id: String,
        out: String,
    },
}

fn via(s: &str) -> RouteAttemptVia {
    match s {
        "primary" => RouteAttemptVia::Primary,
        "retry" => RouteAttemptVia::Retry,
        "concierge" => RouteAttemptVia::Concierge,
        "understudy" => RouteAttemptVia::Understudy,
        "tier-pick" => RouteAttemptVia::TierPick,
        other => panic!("unknown via {other:?}"),
    }
}

fn outcome(s: &str) -> RouteAttemptOutcome {
    match s {
        "answered" => RouteAttemptOutcome::Answered,
        "failed" => RouteAttemptOutcome::Failed,
        "refused" => RouteAttemptOutcome::Refused,
        other => panic!("unknown outcome {other:?}"),
    }
}

fn trigger(s: &str) -> FallbackTrigger {
    match s {
        "auth" => FallbackTrigger::Auth,
        "rate-limit" => FallbackTrigger::RateLimit,
        "network" => FallbackTrigger::Network,
        "model-missing" => FallbackTrigger::ModelMissing,
        "provider-error" => FallbackTrigger::ProviderError,
        "empty-response" => FallbackTrigger::EmptyResponse,
        "moderation-refusal" => FallbackTrigger::ModerationRefusal,
        other => panic!("unknown trigger {other:?}"),
    }
}

fn evidence(s: &str) -> RouteAttemptEvidence {
    match s {
        "finish-reason" => RouteAttemptEvidence::FinishReason,
        "inferred" => RouteAttemptEvidence::Inferred,
        other => panic!("unknown evidence {other:?}"),
    }
}

fn candidate_kind(s: &str) -> FallbackCandidateKind {
    match s {
        "primary" => FallbackCandidateKind::Primary,
        "configured" => FallbackCandidateKind::Configured,
        "tier-pick" => FallbackCandidateKind::TierPick,
        other => panic!("unknown candidate kind {other:?}"),
    }
}

/// A `RouteAttempt` from the oracle's plain-object failure list.
fn attempt_from_value(v: &Value) -> RouteAttempt {
    let s = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    RouteAttempt {
        profile_id: s("profileId"),
        profile_name: s("profileName"),
        provider: s("provider"),
        model_name: s("modelName"),
        via: via(v.get("via").and_then(Value::as_str).unwrap()),
        outcome: outcome(v.get("outcome").and_then(Value::as_str).unwrap()),
        trigger: v.get("trigger").and_then(Value::as_str).map(trigger),
        evidence: v.get("evidence").and_then(Value::as_str).map(evidence),
        detail: v.get("detail").and_then(Value::as_str).map(str::to_string),
    }
}

/// The lone-surrogate divergence: v4's ill-formed cut, rendered as Rust would.
///
/// `\ud83d` is what v4's `JSON.stringify` emits for the orphaned high surrogate;
/// `String::from_utf16_lossy` answers U+FFFD for the same unit. Applying this
/// rewrite to v4's bytes must yield v5's exactly — nothing else may differ.
fn v4_lone_surrogates_as_replacement(s: &str) -> String {
    let mut out = s.to_string();
    for hi in 0xD800u32..=0xDBFFu32 {
        let esc = format!("\\u{hi:04x}");
        if out.contains(&esc) {
            out = out.replace(&esc, "\u{FFFD}");
        }
        let esc_upper = format!("\\u{:04X}", hi);
        if out.contains(&esc_upper) {
            out = out.replace(&esc_upper, "\u{FFFD}");
        }
    }
    out
}

fn state_with(
    raw: Option<Value>,
    failures: Vec<RouteAttempt>,
    v: RouteAttemptVia,
    seat: Option<&Seat>,
) -> StreamingState {
    StreamingState {
        raw_response: raw,
        route_failures: failures,
        route_via: v,
        effective_profile: seat.map(Seat::to_effective),
        ..Default::default()
    }
}

#[test]
fn route_trail_compose_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_ROUTE_TRAIL") else {
        println!("SKIP: QT_ORACLE_ROUTE_TRAIL unset");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    assert!(!text.trim().is_empty(), "oracle NDJSON is EMPTY: {path}");

    // [kind index] viaOf, classify, record, setVia, compose, seed
    let mut counts = [0usize; 6];
    let mut divergences = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: OracleRow = serde_json::from_str(line).expect("parse oracle row");
        match row {
            OracleRow::ViaOf {
                id,
                candidate_kind: kind,
                out,
            } => {
                assert_eq!(via_of(candidate_kind(&kind)).as_str(), out, "viaOf '{id}'");
                counts[0] += 1;
            }
            OracleRow::Classify {
                id,
                raw_response,
                content_was_flagged_dangerous,
                out,
            } => {
                let state = state_with(raw_response, vec![], RouteAttemptVia::Primary, None);
                let got = classify_empty_body(&state, content_was_flagged_dangerous);
                assert_eq!(got.outcome.as_str(), out.outcome, "classify '{id}' outcome");
                assert_eq!(got.trigger.as_str(), out.trigger, "classify '{id}' trigger");
                assert_eq!(
                    got.evidence.map(|e| e.as_str().to_string()),
                    out.evidence,
                    "classify '{id}' evidence"
                );
                assert_eq!(got.detail, out.detail, "classify '{id}' detail");
                counts[1] += 1;
            }
            OracleRow::Record {
                id,
                seat,
                via: v,
                outcome: o,
                trigger: t,
                detail,
                evidence: e,
                out,
                failures_after,
            } => {
                let mut state = state_with(None, vec![], RouteAttemptVia::Primary, None);
                record_route_failure(
                    &mut state,
                    &seat.to_effective(),
                    via(&v),
                    outcome(&o),
                    trigger(&t),
                    detail.as_deref(),
                    e.as_deref().map(evidence),
                );
                assert_eq!(
                    state.route_failures.len(),
                    failures_after,
                    "record '{id}' pushed exactly one"
                );
                let got = serde_json::to_string(&state.route_failures[0]).expect("serialize");
                if got == out {
                    counts[2] += 1;
                    continue;
                }
                // The one recorded divergence — pinned in BOTH directions.
                assert!(
                    id.starts_with("detail-astral-splits") || id == "detail-201-bmp-astral-tail",
                    "record '{id}' differs from v4:\n  v4: {out}\n  v5: {got}"
                );
                assert!(
                    out.contains("\\ud83d") || out.contains("\\uD83D"),
                    "record '{id}': v4 no longer emits a lone surrogate — the \
                     divergence has CONVERGED and this arm must retire to a plain \
                     equality.\n  v4: {out}"
                );
                assert!(
                    got.contains('\u{FFFD}'),
                    "record '{id}': v5 should answer U+FFFD for the split pair.\n  v5: {got}"
                );
                assert_eq!(
                    v4_lone_surrogates_as_replacement(&out),
                    got,
                    "record '{id}': v4 and v5 differ by MORE than the lone surrogate"
                );
                divergences += 1;
                counts[2] += 1;
            }
            OracleRow::SetVia { id, to, out } => {
                let mut state = state_with(None, vec![], RouteAttemptVia::Primary, None);
                set_route_via(&mut state, via(&to));
                assert_eq!(state.route_via.as_str(), out, "setVia '{id}'");
                counts[3] += 1;
            }
            OracleRow::Compose {
                id,
                failures,
                route_via,
                seat,
                out,
            } => {
                let state = state_with(
                    None,
                    failures.iter().map(attempt_from_value).collect(),
                    via(&route_via),
                    Some(&seat),
                );
                let got = build_route_trail(
                    &state,
                    RouteTrailLogContext {
                        chat_id: Some("chat-1"),
                        message_id: Some("msg-1"),
                    },
                );
                match (got, out) {
                    (None, None) => {}
                    (Some(trail), Some(expected)) => {
                        assert_eq!(
                            serde_json::to_string(&trail).expect("serialize"),
                            expected,
                            "compose '{id}'"
                        );
                    }
                    (g, e) => panic!("compose '{id}': null rule disagrees — v5 {g:?} vs v4 {e:?}"),
                }
                counts[4] += 1;
            }
            OracleRow::Seed {
                id,
                danger_profile_id,
                connection_profile_id,
                out,
            } => {
                // The orchestrator's `:540` ternary, in v5's spelling.
                let seeded = if danger_profile_id != connection_profile_id {
                    RouteAttemptVia::Concierge
                } else {
                    RouteAttemptVia::Primary
                };
                assert_eq!(seeded.as_str(), out, "seed '{id}'");
                counts[5] += 1;
            }
        }
    }

    let total: usize = counts.iter().sum();
    assert!(
        counts.iter().all(|&c| c > 0),
        "every kind exercised: {counts:?}"
    );
    assert!(total >= 30, "corpus floor: {total} rows");
    assert_eq!(
        divergences, 2,
        "exactly the two recorded lone-surrogate rows may diverge"
    );
    println!("route_trail_compose_equivalence: {total} rows green ({counts:?}), {divergences} recorded divergences");
}
