//! Episodic-spine helpers (the episodic recall overhaul, v4 `8bf3cb5f`,
//! `lib/memory/episodic.ts`).
//!
//! **Partly LANE-REMOVE shims (work order P4.D289, §R.10(o)).** In the
//! `83334b9a5` span v4 moved the WRITE half out of this module:
//! `resolveWhenPhrase` to `lib/memory/time/clock.ts` (`e0ac7493a` — the ONE
//! rule table, stamping the window START on the memory calendar; ported as
//! [`crate::memory_time::resolve_when_phrase`]), `buildMemoryEmbeddingText`
//! to `index-store/embedding-text.ts` (`af48f6cc2`; ported as
//! [`crate::memory_embedding_text::embedding_text_for`]), and the anchor
//! line onto the memory calendar (`c2795733f`, bug 212; ported as
//! [`crate::memory_embedding_text::memory_anchor_line`]). Their callers live
//! in other lanes' files (admission P4.D290, recall P4.D291, embeddings
//! P4.D292, extraction P4.D296, maintenance P4.D297), so the three old
//! functions keep their pre-refactor bodies VERBATIM (UTC, the pre stamps)
//! until their owners re-point (LR-1). `EpisodicAnchorView`, the event
//! clocks and `js_date_parse_ms` stay here as v4 keeps them (LR-12).
//!
//! Small, pure utilities shared by the write path (memory-service, memory-gate,
//! extraction) and the read path (injector, tools):
//!
//!  - [`build_memory_anchor_line`] / [`build_memory_embedding_text`]: the
//!    `(when: 2026-07-14 · place: Lighthouse Point)` anchor line appended to a
//!    memory's embedded text so the vector itself carries temporal/place signal.
//!  - [`resolve_when_phrase`]: deterministic server-side resolution of a
//!    model-emitted `when` phrase (absolute or relative) into an ISO
//!    `occurredAt`, anchored to the source turn's timestamp. Its callers (all
//!    landed in round 3): the memory processor's `resolve_candidate_anchors`,
//!    the fold-episode pass, and the create-path fallback anchors.
//!  - [`event_reference_time_ms`]: the event clock (`occurredAt ?? write
//!    clock`) used for age labels — distinct from the decay reference time in
//!    memory scoring, which stays on the write/reinforce clock.
//!
//! Pure + I/O-free, exactly as v4 keeps it importable from the forked job child.
//!
//! ## JS-fidelity notes
//!
//!  - `Date.parse` is V8's own parser ([`js_date_parse_ms`] →
//!    [`crate::v8_date_parse`]): the ES5 ISO grammar AND the legacy fallback
//!    (`"July 14, 2026"`, `2026/07/14`, …) — the ISO-subset port it replaced
//!    was P4.D289's `JS_DATE_PARSE_ISO_SUBSET` finding. V8's ISO quirks are
//!    reproduced: month must be 01–12 but a day of 01–31 **rolls** past the
//!    month end (`2026-02-31` → Mar 3 — and the date-only when-phrase branch
//!    then returns the RAW `2026-02-31T00:00:00.000Z`, v4's exact behavior);
//!    fraction digits truncate to milliseconds; lowercase `t`/`z` accepted.
//!  - A zone-less datetime parses as **UTC** here. In JS it parses in the
//!    process's LOCAL zone — the oracle is therefore generated with `TZ=UTC`
//!    (see `harness/oracle/cases/episodic.ts`). Production `occurredAt` inputs
//!    always carry a zone, so this seam is corpus-only.
//!  - JS `\s` / `String.trim` are the JS whitespace class ([`crate::jsstr`]).

/// Structural view of the episodic fields (keeps this module Memory-import-free —
/// v4 `EpisodicAnchorView`).
#[derive(Debug, Clone, Default)]
pub struct EpisodicAnchorView {
    pub occurred_at: Option<String>,
    pub narrative_time: Option<String>,
    pub entities: Vec<String>,
}

/// Event-clock reference time for age labels: prefer `occurredAt` (when the
/// event happened) over the write/reinforce clock. Falls back to the supplied
/// write-clock milliseconds when `occurredAt` is absent or unparsable.
///
/// v4 `eventReferenceTimeMs` — note the JS truthiness gate: only an ABSENT or
/// EMPTY string skips the parse (whitespace-only is truthy, parses to NaN, and
/// falls back through the finiteness check).
pub fn event_reference_time_ms(occurred_at: Option<&str>, write_clock_ms: f64) -> f64 {
    event_time_ms(occurred_at).unwrap_or(write_clock_ms)
}

/// The Option form of [`event_reference_time_ms`] for constructors that carry
/// the event time as a pre-parsed field (`MemoryInputs::occurred_at_ms`):
/// `Some(ms)` only when `occurredAt` is present, non-empty (JS truthiness),
/// and parsable — exactly the cases where v4's event clock takes over.
pub fn event_time_ms(occurred_at: Option<&str>) -> Option<f64> {
    let s = occurred_at?;
    if s.is_empty() {
        return None;
    }
    js_date_parse_ms(s).map(|ms| ms as f64)
}

/// JS `Date.parse` — V8's own parser, ES5 ISO pass AND legacy fallback,
/// ported whole in [`crate::v8_date_parse`] (the human's ruling on P4.D289's
/// `JS_DATE_PARSE_ISO_SUBSET` finding, 2026-10-14). A string with no UTC
/// offset reads as UTC (JS: the process's local zone — the ruled
/// `ZONELESS_PARSE_AS_UTC`; every oracle pins `TZ=UTC`).
pub(crate) fn js_date_parse_ms(s: &str) -> Option<i64> {
    crate::v8_date_parse::date_parse_ms(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_date_parse_v8_quirks() {
        // Confirmed against Node 24 (see the doc comment).
        assert_eq!(js_date_parse_ms("2026-07-00"), None);
        assert_eq!(js_date_parse_ms("2026-00-10"), None);
        assert_eq!(js_date_parse_ms("2026-07-32"), None);
        assert_eq!(js_date_parse_ms("2026-13-05"), None);
        // Day rolls past the month end.
        assert_eq!(
            js_date_parse_ms("2026-02-31"),
            js_date_parse_ms("2026-03-03")
        );
        // Fraction truncation.
        assert_eq!(
            js_date_parse_ms("2026-07-14T09:30:00.123456Z"),
            js_date_parse_ms("2026-07-14T09:30:00.123Z")
        );
        assert_eq!(
            js_date_parse_ms("2026-07-14T09:30:00.5Z"),
            js_date_parse_ms("2026-07-14T09:30:00.500Z")
        );
        // Lowercase t/z; bad shapes.
        assert!(js_date_parse_ms("2026-07-10t18:45:00z").is_some());
        assert_eq!(js_date_parse_ms("2026-07-14t9:30"), None);
        assert_eq!(js_date_parse_ms("2026-07-14T23:60:00Z"), None);
        assert_eq!(js_date_parse_ms("not-a-date"), None);
    }
}

#[cfg(test)]
mod hour_24_tests {
    use super::js_date_parse_ms;

    /// V8's `Date.parse` takes hour 24 only as the day's end (P4.D289).
    #[test]
    fn hour_24_parses_only_at_the_end_of_the_day() {
        assert_eq!(
            js_date_parse_ms("2026-07-14T24:00Z"),
            js_date_parse_ms("2026-07-15T00:00:00.000Z")
        );
        assert!(js_date_parse_ms("2026-07-14T24:00:00.000Z").is_some());
        assert_eq!(js_date_parse_ms("2026-07-14T24:01Z"), None);
        assert_eq!(js_date_parse_ms("2026-07-14T24:00:01Z"), None);
        assert_eq!(js_date_parse_ms("2026-07-14T24:00:00.001Z"), None);
        assert_eq!(js_date_parse_ms("2026-07-14t24:30z"), None);
    }
}
