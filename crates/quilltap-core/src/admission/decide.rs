//! Port of v4 `lib/memory/admission/decide.ts` (`8af764514`, Phase C of the
//! memory system refactor) — what to do with a candidate, given its
//! neighbours.
//!
//! v4's why, carried: pure — no I/O, no writes, no clock. The neighbours
//! arrive already hydrated (live rows, superseded members replaced by their
//! digest — see P4.D290's `neighbours`), so promoting a cold row is an
//! *action* the caller applies, never a side effect of deciding.
//!
//! The rules, in order (memory refactor §3.6):
//!
//!  1. A neighbour under `related` is ignored; one in [`related`, `reinforce`)
//!     is a link target.
//!  2. A neighbour at or above `reinforce` may take the candidate only when it
//!     shares the candidate's **subject** (`aboutCharacterId`, a self memory
//!     reading as the holder) and **kind**, is not an extracted (AUTO) row
//!     under a hand-written (MANUAL) candidate, and is not a **different
//!     occasion** (both carry an `occurredAt`, more than `dateGuardDays`
//!     apart). Every such neighbour is checked, not only the closest; an
//!     ineligible one becomes a link target.
//!  3. The closest eligible neighbour takes the candidate: a cold row nothing
//!     supersedes is promoted first; a digest reached through a superseded
//!     member absorbs by count only; at or above `nearDuplicate` the candidate
//!     is absorbed; otherwise it reinforces.
//!  4. With no eligible neighbour, the candidate is inserted, linked to every
//!     link target when there are any.
//!
//! JS fidelity: the neighbour sort is v4's `[...neighbours].sort((a, b) =>
//! b.cosine − a.cosine)` through [`js_array_sort_by`] (stable; a NaN cosine
//! takes V8's side of every comparison rather than panicking Rust's
//! `sort_by`). A NaN cosine fails both `<` band tests, so it is assessed for
//! exclusion like a close neighbour — and is never `near` — exactly as v4.

use std::collections::HashSet;

use serde_json::Value;

use super::thresholds::AdmissionThresholds;
use super::types::{
    AdmissionAction, AdmissionDecision, AdmissionNeighbour, LinkReason, NeighbourAssessment,
    NeighbourExclusion, Verdict,
};
use crate::memory_scoring::js_sort::js_array_sort_by;

/// The parts of a candidate the decider reads (v4 `AdmissionCandidateView`).
/// `kind` is `'semantic' | 'episodic'`, `source` `'AUTO' | 'MANUAL' |
/// 'CONSOLIDATED'`; `None` is v4's `undefined` / `null`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AdmissionCandidateView<'a> {
    pub holder_id: &'a str,
    pub about_id: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub source: Option<&'a str>,
    pub occurred_at: Option<&'a str>,
}

/// True when both timestamps parse and sit more than `date_guard_days` apart
/// (v4 `occasionsAreDistinct(a, b, dateGuardDays)`): the same activity on a
/// different occasion, which must persist as its own row. A missing, empty
/// (JS-falsy) or unparseable stamp on either side is never "distinct" — the
/// guard only fires on evidence. Strictly greater: exactly the guard apart is
/// the same occasion.
///
/// `Date.parse` is [`crate::episodic::js_date_parse_ms`] (a zone-less datetime
/// reads as UTC — the ruled `ZONELESS_PARSE_AS_UTC`, LR-10).
pub fn occasions_are_distinct(
    candidate_occurred_at: Option<&str>,
    existing_occurred_at: Option<&str>,
    date_guard_days: f64,
) -> bool {
    let (Some(a), Some(b)) = (
        candidate_occurred_at.filter(|s| !s.is_empty()),
        existing_occurred_at.filter(|s| !s.is_empty()),
    ) else {
        return false;
    };
    let (Some(a), Some(b)) = (
        crate::episodic::js_date_parse_ms(a),
        crate::episodic::js_date_parse_ms(b),
    ) else {
        return false;
    };
    (a as f64 - b as f64).abs() > date_guard_days * 86_400_000.0
}

/// The subject a memory is about, a self memory reading as its holder (v4
/// `subjectOf(holderId, aboutId) = aboutId || holderId` — literally: an EMPTY
/// `about_id` is JS-falsy and reads as the holder too).
pub fn subject_of<'a>(holder_id: &'a str, about_id: Option<&'a str>) -> &'a str {
    match about_id {
        Some(about) if !about.is_empty() => about,
        _ => holder_id,
    }
}

/// JS truthiness of a JSON value (`undefined` is `None`).
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// v4's `x ?? fallback` over a row field: `null` / absent → the fallback.
fn or_default<'a>(v: Option<&'a Value>, fallback: &'a Value) -> &'a Value {
    match v {
        None | Some(Value::Null) => fallback,
        Some(v) => v,
    }
}

/// v4 `exclusionFor`: the FIRST exclusion that applies, in the order subject →
/// kind → manual-over-auto → date-guard.
fn exclusion_for(
    candidate: &AdmissionCandidateView<'_>,
    neighbour: &AdmissionNeighbour,
    thresholds: &AdmissionThresholds,
) -> Option<NeighbourExclusion> {
    let m = &neighbour.memory;
    // `subjectOf(m.characterId, m.aboutCharacterId)` over the row's JSON,
    // compared with `!==` to the candidate's subject (always a string).
    let about = m.get("aboutCharacterId");
    let row_subject = if js_truthy(about) {
        about
    } else {
        m.get("characterId")
    };
    let candidate_subject = subject_of(candidate.holder_id, candidate.about_id);
    if row_subject.and_then(Value::as_str) != Some(candidate_subject) {
        return Some(NeighbourExclusion::Subject);
    }
    let semantic = Value::String("semantic".into());
    let row_kind = or_default(m.get("kind"), &semantic);
    if row_kind.as_str() != Some(candidate.kind.unwrap_or("semantic")) {
        return Some(NeighbourExclusion::Kind);
    }
    let manual = Value::String("MANUAL".into());
    if candidate.source == Some("MANUAL")
        && or_default(m.get("source"), &manual).as_str() == Some("AUTO")
    {
        return Some(NeighbourExclusion::ManualOverAuto);
    }
    // v4's `Memory.occurredAt` is `string | null`; a non-string never reaches
    // here through the repository's schema.
    if occasions_are_distinct(
        candidate.occurred_at,
        m.get("occurredAt").and_then(Value::as_str),
        thresholds.date_guard_days,
    ) {
        return Some(NeighbourExclusion::DateGuard);
    }
    None
}

/// Decide what admission does with a candidate (v4 `decideAdmission`).
/// `neighbours` may arrive in any order; ties keep their given order.
pub fn decide_admission(
    candidate: &AdmissionCandidateView<'_>,
    neighbours: &[AdmissionNeighbour],
    thresholds: &AdmissionThresholds,
) -> AdmissionDecision {
    let mut sorted: Vec<&AdmissionNeighbour> = neighbours.iter().collect();
    js_array_sort_by(&mut sorted, |a, b| b.cosine - a.cosine);

    let mut assessments: Vec<NeighbourAssessment> = Vec::new();
    let mut target: Option<&AdmissionNeighbour> = None;
    let mut link_to: Vec<String> = Vec::new();
    let mut exclusions: HashSet<NeighbourExclusion> = HashSet::new();

    for n in sorted {
        let id = n
            .memory
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        // `...(n.via ? { via: n.via } : {})` — truthy only.
        let via = n.via.clone().filter(|v| !v.is_empty());
        let assess =
            |verdict: Verdict, excluded_by: Option<NeighbourExclusion>| NeighbourAssessment {
                id: id.clone(),
                cosine: n.cosine,
                via: via.clone(),
                verdict,
                excluded_by,
            };
        if n.cosine < thresholds.related {
            assessments.push(assess(Verdict::Below, None));
            continue;
        }
        if n.cosine < thresholds.reinforce {
            assessments.push(assess(Verdict::Band, None));
            if !link_to.contains(&id) {
                link_to.push(id.clone());
            }
            continue;
        }
        if let Some(excluded_by) = exclusion_for(candidate, n, thresholds) {
            assessments.push(assess(Verdict::Excluded, Some(excluded_by)));
            exclusions.insert(excluded_by);
            if !link_to.contains(&id) {
                link_to.push(id.clone());
            }
            continue;
        }
        assessments.push(assess(Verdict::Eligible, None));
        if target.is_none() {
            target = Some(n);
        }
    }

    let action = if let Some(target) = target {
        let m = &target.memory;
        let into = m
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let near = target.cosine >= thresholds.near_duplicate;
        let cold_unsuperseded = m.get("tier").and_then(Value::as_str) == Some("cold")
            && (!js_truthy(m.get("supersededById")) || target.dangling_digest);
        let via = target.via.as_deref().filter(|v| !v.is_empty());
        if cold_unsuperseded {
            AdmissionAction::PromoteAndReinforce {
                into,
                with_footnotes: !near,
            }
        } else if let Some(via) = via {
            // A digest is reinforced by count only: the next consolidation
            // folds any novel detail in.
            AdmissionAction::Absorb {
                into,
                redirected_from: Some(via.to_string()),
            }
        } else if near {
            AdmissionAction::Absorb {
                into,
                redirected_from: None,
            }
        } else {
            AdmissionAction::Reinforce { into }
        }
    } else if !link_to.is_empty() {
        let reason = if exclusions.contains(&NeighbourExclusion::DateGuard) {
            LinkReason::DateGuard
        } else if !exclusions.is_empty() {
            LinkReason::Mismatch
        } else {
            LinkReason::Band
        };
        AdmissionAction::InsertLinked { link_to, reason }
    } else {
        AdmissionAction::Insert
    };

    AdmissionDecision {
        action,
        assessments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::ADMISSION_THRESHOLDS;
    use serde_json::json;

    fn n(id: &str, cosine: f64) -> AdmissionNeighbour {
        AdmissionNeighbour {
            memory: json!({ "id": id, "characterId": "h", "aboutCharacterId": null, "kind": "semantic", "source": "AUTO", "tier": "hot" }),
            cosine,
            via: None,
            dangling_digest: false,
        }
    }

    const SELF: AdmissionCandidateView<'static> = AdmissionCandidateView {
        holder_id: "h",
        about_id: None,
        kind: Some("semantic"),
        source: Some("AUTO"),
        occurred_at: None,
    };

    #[test]
    fn several_band_rows_link_best_first() {
        let d = decide_admission(
            &SELF,
            &[n("b", 0.72), n("a", 0.81), n("c", 0.5)],
            &ADMISSION_THRESHOLDS,
        );
        assert_eq!(
            d.action,
            AdmissionAction::InsertLinked {
                link_to: vec!["a".into(), "b".into()],
                reason: LinkReason::Band
            }
        );
        assert_eq!(
            serde_json::to_string(&d.action).unwrap(),
            r#"{"kind":"insert-linked","linkTo":["a","b"],"reason":"band"}"#
        );
    }

    #[test]
    fn subject_of_reads_an_empty_about_as_the_holder() {
        assert_eq!(subject_of("h", Some("")), "h");
        assert_eq!(subject_of("h", None), "h");
        assert_eq!(subject_of("h", Some("bob")), "bob");
    }
}
