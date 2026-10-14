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

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of v4 `lib/memory/__tests__/admission-decide.test.ts`
    //! (pin `83334b9a5`), plus the `occasionsAreDistinct` half of
    //! `lib/memory/__tests__/gate-date-guard.test.ts` (its retro-signature half
    //! is recall's). v4's `Memory` objects are the row JSON the decider reads.

    use super::*;
    use crate::admission::ADMISSION_THRESHOLDS;
    use serde_json::json;

    const HOLDER: &str = "char-holder";
    const BOB: &str = "char-bob";

    /// v4's `mem(id, overrides)`.
    fn mem(id: &str, overrides: Value) -> Value {
        let mut m = json!({
            "id": id,
            "characterId": HOLDER,
            "content": format!("content {id}"),
            "summary": format!("summary {id}"),
            "keywords": [],
            "tags": [],
            "importance": 0.5,
            "aboutCharacterId": null,
            "source": "AUTO",
            "kind": "semantic",
            "reinforcementCount": 1,
            "relatedMemoryIds": [],
            "reinforcedImportance": 0.5,
            "tier": "hot",
            "supersededById": null,
            "consolidatedFrom": [],
            "entities": [],
            "occurredAt": null,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z",
        });
        if let (Value::Object(base), Value::Object(over)) = (&mut m, overrides) {
            base.extend(over);
        }
        m
    }

    /// v4's `n(memory, cosine)`.
    fn n(memory: Value, cosine: f64) -> AdmissionNeighbour {
        AdmissionNeighbour {
            memory,
            cosine,
            via: None,
            dangling_digest: false,
        }
    }

    const AUTO_SELF: AdmissionCandidateView<'static> = AdmissionCandidateView {
        holder_id: HOLDER,
        about_id: None,
        kind: Some("semantic"),
        source: Some("AUTO"),
        occurred_at: None,
    };

    fn decide(
        neighbours: &[AdmissionNeighbour],
        candidate: AdmissionCandidateView<'_>,
    ) -> AdmissionAction {
        decide_admission(&candidate, neighbours, &ADMISSION_THRESHOLDS).action
    }

    fn insert_linked(ids: &[&str], reason: LinkReason) -> AdmissionAction {
        AdmissionAction::InsertLinked {
            link_to: ids.iter().map(|s| s.to_string()).collect(),
            reason,
        }
    }

    fn reinforce(into: &str) -> AdmissionAction {
        AdmissionAction::Reinforce { into: into.into() }
    }

    fn absorb(into: &str) -> AdmissionAction {
        AdmissionAction::Absorb {
            into: into.into(),
            redirected_from: None,
        }
    }

    fn none() -> Value {
        json!({})
    }

    // --- describe('decideAdmission — threshold bands') ---

    /// `it.each(cases)('%s', …)` — the band table, row for row.
    #[test]
    fn decide_admission_threshold_bands() {
        let cases: Vec<(&str, Vec<AdmissionNeighbour>, AdmissionAction)> = vec![
            ("no neighbours → insert", vec![], AdmissionAction::Insert),
            (
                "all below related → insert",
                vec![n(mem("a", none()), 0.69)],
                AdmissionAction::Insert,
            ),
            (
                "related band → insert-linked",
                vec![n(mem("a", none()), 0.70)],
                insert_linked(&["a"], LinkReason::Band),
            ),
            (
                "just under reinforce → insert-linked",
                vec![n(mem("a", none()), 0.8499)],
                insert_linked(&["a"], LinkReason::Band),
            ),
            (
                "reinforce band → reinforce",
                vec![n(mem("a", none()), 0.85)],
                reinforce("a"),
            ),
            (
                "just under near-duplicate → reinforce",
                vec![n(mem("a", none()), 0.8999)],
                reinforce("a"),
            ),
            (
                "near-duplicate → absorb",
                vec![n(mem("a", none()), 0.90)],
                absorb("a"),
            ),
            (
                "several band rows all link, best first",
                vec![
                    n(mem("b", none()), 0.72),
                    n(mem("a", none()), 0.81),
                    n(mem("c", none()), 0.5),
                ],
                insert_linked(&["a", "b"], LinkReason::Band),
            ),
            (
                "closest eligible wins",
                vec![n(mem("a", none()), 0.87), n(mem("b", none()), 0.93)],
                absorb("b"),
            ),
        ];
        assert_eq!(cases.len(), 9);
        for (label, neighbours, expected) in cases {
            assert_eq!(decide(&neighbours, AUTO_SELF), expected, "{label}");
        }
    }

    // --- describe('decideAdmission — subject, kind and source') ---

    /// "a close neighbour about someone else is linked, never folded into"
    #[test]
    fn a_close_neighbour_about_someone_else_is_linked_never_folded_into() {
        assert_eq!(
            decide(
                &[n(mem("a", json!({ "aboutCharacterId": BOB })), 0.95)],
                AUTO_SELF
            ),
            insert_linked(&["a"], LinkReason::Mismatch)
        );
    }

    /// "reads a self memory the same whether aboutCharacterId is null or the holder"
    #[test]
    fn reads_a_self_memory_the_same_whether_about_character_id_is_null_or_the_holder() {
        assert_eq!(
            decide(
                &[n(mem("a", json!({ "aboutCharacterId": HOLDER })), 0.87)],
                AUTO_SELF
            ),
            reinforce("a")
        );
        assert_eq!(
            decide(
                &[n(mem("a", none()), 0.87)],
                AdmissionCandidateView {
                    about_id: Some(HOLDER),
                    ..AUTO_SELF
                }
            ),
            reinforce("a")
        );
    }

    /// "folds a candidate about Bob into a row about Bob"
    #[test]
    fn folds_a_candidate_about_bob_into_a_row_about_bob() {
        assert_eq!(
            decide(
                &[n(mem("a", json!({ "aboutCharacterId": BOB })), 0.87)],
                AdmissionCandidateView {
                    about_id: Some(BOB),
                    ..AUTO_SELF
                }
            ),
            reinforce("a")
        );
    }

    /// "a different kind is linked, not folded into"
    #[test]
    fn a_different_kind_is_linked_not_folded_into() {
        assert_eq!(
            decide(
                &[n(mem("a", json!({ "kind": "episodic" })), 0.95)],
                AUTO_SELF
            ),
            insert_linked(&["a"], LinkReason::Mismatch)
        );
    }

    /// "a MANUAL candidate never disappears into an AUTO row"
    #[test]
    fn a_manual_candidate_never_disappears_into_an_auto_row() {
        assert_eq!(
            decide(
                &[n(mem("a", none()), 0.97)],
                AdmissionCandidateView {
                    source: Some("MANUAL"),
                    ..AUTO_SELF
                }
            ),
            insert_linked(&["a"], LinkReason::Mismatch)
        );
    }

    /// "a MANUAL candidate may fold into a MANUAL row or a digest"
    #[test]
    fn a_manual_candidate_may_fold_into_a_manual_row_or_a_digest() {
        let manual = AdmissionCandidateView {
            source: Some("MANUAL"),
            ..AUTO_SELF
        };
        assert_eq!(
            decide(&[n(mem("a", json!({ "source": "MANUAL" })), 0.87)], manual),
            reinforce("a")
        );
        assert_eq!(
            decide(
                &[n(mem("d", json!({ "source": "CONSOLIDATED" })), 0.95)],
                manual
            ),
            absorb("d")
        );
    }

    /// "an AUTO candidate folds into a MANUAL row"
    #[test]
    fn an_auto_candidate_folds_into_a_manual_row() {
        assert_eq!(
            decide(
                &[n(mem("a", json!({ "source": "MANUAL" })), 0.95)],
                AUTO_SELF
            ),
            absorb("a")
        );
    }

    /// "skips an ineligible closest neighbour for an eligible second"
    #[test]
    fn skips_an_ineligible_closest_neighbour_for_an_eligible_second() {
        let action = decide(
            &[
                n(mem("bob", json!({ "aboutCharacterId": BOB })), 0.97),
                n(mem("self", none()), 0.88),
            ],
            AUTO_SELF,
        );
        assert_eq!(action, reinforce("self"));
    }

    // --- describe('decideAdmission — date guard over every close neighbour') ---

    const SPRING: &str = "2026-03-10T00:00:00.000Z";
    const SUMMER: &str = "2026-07-14T00:00:00.000Z";
    const SUMMER_RETOLD: &str = "2026-07-16T00:00:00.000Z";

    fn in_summer() -> AdmissionCandidateView<'static> {
        AdmissionCandidateView {
            occurred_at: Some(SUMMER),
            ..AUTO_SELF
        }
    }

    /// "a different occasion at rank 1 is linked"
    #[test]
    fn a_different_occasion_at_rank_1_is_linked() {
        assert_eq!(
            decide(
                &[n(mem("a", json!({ "occurredAt": SPRING })), 0.96)],
                in_summer()
            ),
            insert_linked(&["a"], LinkReason::DateGuard)
        );
    }

    /// "looks past a different occasion to a same-occasion row below it"
    #[test]
    fn looks_past_a_different_occasion_to_a_same_occasion_row_below_it() {
        let action = decide(
            &[
                n(mem("spring", json!({ "occurredAt": SPRING })), 0.96),
                n(mem("summer", json!({ "occurredAt": SUMMER_RETOLD })), 0.88),
            ],
            in_summer(),
        );
        assert_eq!(action, reinforce("summer"));
    }

    /// "links every distinct occasion and band row when none qualifies"
    #[test]
    fn links_every_distinct_occasion_and_band_row_when_none_qualifies() {
        let action = decide(
            &[
                n(mem("spring", json!({ "occurredAt": SPRING })), 0.96),
                n(
                    mem(
                        "autumn",
                        json!({ "occurredAt": "2025-10-01T00:00:00.000Z" }),
                    ),
                    0.91,
                ),
                n(mem("band", none()), 0.75),
            ],
            in_summer(),
        );
        assert_eq!(
            action,
            insert_linked(&["spring", "autumn", "band"], LinkReason::DateGuard)
        );
    }

    /// "never fires without an event time on both sides"
    #[test]
    fn never_fires_without_an_event_time_on_both_sides() {
        assert_eq!(
            decide(
                &[n(mem("a", json!({ "occurredAt": SPRING })), 0.96)],
                AUTO_SELF
            ),
            absorb("a")
        );
    }

    // --- describe('decideAdmission — digests and the cold tier') ---

    /// "a digest reached through a superseded member absorbs by count only"
    #[test]
    fn a_digest_reached_through_a_superseded_member_absorbs_by_count_only() {
        let digest = mem("digest", json!({ "source": "CONSOLIDATED" }));
        let via = |cosine: f64| AdmissionNeighbour {
            via: Some("member".into()),
            ..n(digest.clone(), cosine)
        };
        let expected = AdmissionAction::Absorb {
            into: "digest".into(),
            redirected_from: Some("member".into()),
        };
        assert_eq!(decide(&[via(0.87)], AUTO_SELF), expected);
        assert_eq!(decide(&[via(0.95)], AUTO_SELF), expected);
    }

    /// "a digest matched directly in the reinforce band is reinforced"
    #[test]
    fn a_digest_matched_directly_in_the_reinforce_band_is_reinforced() {
        assert_eq!(
            decide(
                &[n(mem("digest", json!({ "source": "CONSOLIDATED" })), 0.87)],
                AUTO_SELF
            ),
            reinforce("digest")
        );
    }

    /// "a cold row nothing supersedes is promoted, then reinforced"
    #[test]
    fn a_cold_row_nothing_supersedes_is_promoted_then_reinforced() {
        assert_eq!(
            decide(
                &[n(mem("cold", json!({ "tier": "cold" })), 0.87)],
                AUTO_SELF
            ),
            AdmissionAction::PromoteAndReinforce {
                into: "cold".into(),
                with_footnotes: true,
            }
        );
    }

    /// "a cold near-duplicate is promoted, then absorbed"
    #[test]
    fn a_cold_near_duplicate_is_promoted_then_absorbed() {
        assert_eq!(
            decide(
                &[n(mem("cold", json!({ "tier": "cold" })), 0.95)],
                AUTO_SELF
            ),
            AdmissionAction::PromoteAndReinforce {
                into: "cold".into(),
                with_footnotes: false,
            }
        );
    }

    /// "a cold row whose digest is gone is promoted"
    #[test]
    fn a_cold_row_whose_digest_is_gone_is_promoted() {
        let cold = mem("cold", json!({ "tier": "cold", "supersededById": "gone" }));
        let action = decide(
            &[AdmissionNeighbour {
                dangling_digest: true,
                ..n(cold, 0.95)
            }],
            AUTO_SELF,
        );
        // `toMatchObject({ kind: 'promote-and-reinforce', into: 'cold' })`.
        assert!(
            matches!(&action, AdmissionAction::PromoteAndReinforce { into, .. } if into == "cold"),
            "{action:?}"
        );
    }

    /// "a cold row in the band is linked, not promoted"
    #[test]
    fn a_cold_row_in_the_band_is_linked_not_promoted() {
        assert_eq!(
            decide(
                &[n(mem("cold", json!({ "tier": "cold" })), 0.75)],
                AUTO_SELF
            ),
            insert_linked(&["cold"], LinkReason::Band)
        );
    }

    // --- describe('decideAdmission — the trace') ---

    /// "records a verdict for every neighbour, best first"
    #[test]
    fn records_a_verdict_for_every_neighbour_best_first() {
        let AdmissionDecision { assessments, .. } = decide_admission(
            &AUTO_SELF,
            &[
                n(mem("low", none()), 0.4),
                n(mem("bob", json!({ "aboutCharacterId": BOB })), 0.96),
                n(mem("band", none()), 0.75),
                n(mem("ok", none()), 0.88),
            ],
            &ADMISSION_THRESHOLDS,
        );
        let row =
            |id: &str, cosine: f64, verdict: Verdict, excluded_by: Option<NeighbourExclusion>| {
                NeighbourAssessment {
                    id: id.into(),
                    cosine,
                    via: None,
                    verdict,
                    excluded_by,
                }
            };
        assert_eq!(
            assessments,
            vec![
                row(
                    "bob",
                    0.96,
                    Verdict::Excluded,
                    Some(NeighbourExclusion::Subject)
                ),
                row("ok", 0.88, Verdict::Eligible, None),
                row("band", 0.75, Verdict::Band, None),
                row("low", 0.4, Verdict::Below, None),
            ]
        );
        // `toEqual` over the literals: no `via` / `excludedBy` key where v4 has none.
        assert_eq!(
            serde_json::to_string(&assessments).unwrap(),
            r#"[{"id":"bob","cosine":0.96,"verdict":"excluded","excludedBy":"subject"},{"id":"ok","cosine":0.88,"verdict":"eligible"},{"id":"band","cosine":0.75,"verdict":"band"},{"id":"low","cosine":0.4,"verdict":"below"}]"#
        );
    }

    // --- gate-date-guard.test.ts: describe('occasionsAreDistinct') ---

    /// v4's local wrapper: `distinct(a, b, ADMISSION_THRESHOLDS.dateGuardDays)`.
    fn distinct(a: Option<&str>, b: Option<&str>) -> bool {
        occasions_are_distinct(a, b, ADMISSION_THRESHOLDS.date_guard_days)
    }

    /// "is true when occurredAt differ by more than the guard window"
    #[test]
    fn is_true_when_occurred_at_differ_by_more_than_the_guard_window() {
        assert!(distinct(
            Some("2026-03-10T00:00:00.000Z"),
            Some("2026-07-14T00:00:00.000Z")
        ));
    }

    /// "is false within the guard window (same occasion retold days later)"
    #[test]
    fn is_false_within_the_guard_window_same_occasion_retold_days_later() {
        assert!(!distinct(
            Some("2026-07-14T00:00:00.000Z"),
            Some("2026-07-16T00:00:00.000Z")
        ));
    }

    /// "treats exactly ${DATE_GUARD_DAYS} days as the same occasion (strictly-greater guard)"
    #[test]
    fn treats_exactly_date_guard_days_days_as_the_same_occasion_strictly_greater_guard() {
        let a = "2026-07-07T00:00:00.000Z";
        let b = format!(
            "2026-07-{}T00:00:00.000Z",
            7 + ADMISSION_THRESHOLDS.date_guard_days as i64
        );
        assert_eq!(b, "2026-07-14T00:00:00.000Z");
        assert!(!distinct(Some(a), Some(&b)));
    }

    /// "never fires when either side lacks an event time"
    #[test]
    fn never_fires_when_either_side_lacks_an_event_time() {
        assert!(!distinct(None, Some("2026-07-14T00:00:00.000Z")));
        assert!(!distinct(Some("2026-07-14T00:00:00.000Z"), None));
        assert!(!distinct(Some("garbage"), Some("2026-07-14T00:00:00.000Z")));
    }
}
