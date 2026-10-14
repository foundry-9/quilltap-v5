//! Port of v4 `lib/memory/time/anchors.ts` (`e0ac7493a`; = the pre
//! `episodic-anchors.ts` plus a zone) — ONE policy for turning a model's
//! free-text `when` phrase into the `occurredAt` / `narrativeTime` pair
//! stamped on a memory row.
//!
//! v4's why, carried: called once, by the extraction pass runner, with the
//! anchors each pass names — the turn passes resolve against the source
//! message's timestamp and fall back to the same stamp; the fold passes
//! resolve against the stretch's newest message (the episode pass falling
//! back to its oldest). Pure: no I/O, no clock — callers supply every
//! timestamp.
//!
//! v5 shape: the zone is REQUIRED (work order P4.D289 LR-12's principle).
//! v4's `timeZone` is optional and, when absent (or empty), falls back to
//! `resolveWhenPhrase`'s default — the cached instance zone; core never reads
//! an ambient zone, so the caller passes the memory-calendar zone it rendered
//! the extractor's clock in (v4's sole caller passes `ctx.clock.timeZone`).

use super::clock::resolve_when_phrase;

/// v4 `EpisodicAnchorInput`.
#[derive(Clone, Copy, Debug)]
pub struct EpisodicAnchorInput<'a> {
    /// The model's free-text time phrase; empty/absent means "not stated".
    pub when: Option<&'a str>,
    /// The ISO instant the phrase is relative to ("yesterday" counts back
    /// from here).
    pub reference_iso: Option<&'a str>,
    /// `occurredAt` when the phrase is absent, unresolvable, or there is
    /// nothing to resolve against.
    pub fallback_iso: Option<&'a str>,
    /// `"realtime"` | `"narrative"` (only `"narrative"` is tested).
    pub timeline_mode: &'a str,
    /// An explicit in-story stamp the caller already holds; outranks `when`
    /// as `narrativeTime`.
    pub narrative_time: Option<&'a str>,
    /// The memory-calendar zone to resolve `when` in.
    pub time_zone: &'a str,
}

/// v4 `EpisodicAnchors` — `occurred_at` is only `None` when the fallback was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodicAnchors {
    pub occurred_at: Option<String>,
    pub narrative_time: Option<String>,
}

/// Resolve a memory's episodic anchors (v4 `resolveEpisodicAnchors`).
///
///  - `occurred_at` — `when` resolved against `reference_iso`; `fallback_iso`
///    when the phrase is absent or does not resolve.
///  - `narrative_time` — in narrative mode the explicit stamp if given, else
///    the raw `when` phrase (the in-story time IS the phrase); in realtime
///    mode only the explicit stamp survives.
///
/// JS truthiness, carried: an EMPTY `when` / `reference_iso` is "not
/// stated"; an empty explicit `narrative_time` is still given (`??` only
/// skips null/undefined).
pub fn resolve_episodic_anchors(input: &EpisodicAnchorInput<'_>) -> EpisodicAnchors {
    let mut occurred_at = input.fallback_iso.map(str::to_string);
    let when = input.when.filter(|w| !w.is_empty());
    if let (Some(when), Some(reference)) = (when, input.reference_iso.filter(|r| !r.is_empty())) {
        if let Some(resolved) = resolve_when_phrase(Some(when), reference, input.time_zone) {
            // v4 `if (resolved)` — a resolved stamp is always a non-empty ISO.
            occurred_at = Some(resolved);
        }
    }
    let explicit = input.narrative_time;
    let narrative_time = if input.timeline_mode == "narrative" {
        explicit.or(when)
    } else {
        explicit
    };
    EpisodicAnchors {
        occurred_at,
        narrative_time: narrative_time.map(str::to_string),
    }
}

#[cfg(test)]
mod jest_twins {
    //! Case-for-case port of v4 `lib/memory/time/__tests__/anchors.test.ts`
    //! (pin `83334b9a5`). v4's cases pass no `timeZone`, so `when` resolves on
    //! `getMemoryTimeZone()` — with no cached instance zone and no
    //! `QUILLTAP_TIMEZONE`, the runner's system zone, `UTC` under v4's jest
    //! config. v5's zone is required, so these pass `"UTC"` explicitly.

    use super::*;

    const ANCHOR: &str = "2026-07-20T09:05:00.000Z"; // a Monday
    const WINDOW_START: &str = "2026-07-19T18:00:00.000Z";
    const TZ: &str = "UTC";

    fn input<'a>(
        when: Option<&'a str>,
        reference_iso: Option<&'a str>,
        fallback_iso: Option<&'a str>,
        timeline_mode: &'a str,
    ) -> EpisodicAnchorInput<'a> {
        EpisodicAnchorInput {
            when,
            reference_iso,
            fallback_iso,
            timeline_mode,
            narrative_time: None,
            time_zone: TZ,
        }
    }

    /// `it('falls back to fallbackIso when no phrase is given')`
    #[test]
    fn falls_back_to_fallback_iso_when_no_phrase_is_given() {
        assert_eq!(
            resolve_episodic_anchors(&input(None, Some(ANCHOR), Some(ANCHOR), "realtime")),
            EpisodicAnchors {
                occurred_at: Some(ANCHOR.to_string()),
                narrative_time: None
            }
        );
    }

    /// `it('resolves the phrase against referenceIso and keeps the fallback
    /// otherwise')`
    #[test]
    fn resolves_the_phrase_against_reference_iso_and_keeps_the_fallback_otherwise() {
        let resolved = resolve_episodic_anchors(&input(
            Some("last Tuesday"),
            Some(ANCHOR),
            Some(WINDOW_START),
            "realtime",
        ));
        let occurred = resolved.occurred_at.expect("not null");
        assert_eq!(&occurred[..10], "2026-07-14");
        assert_eq!(resolved.narrative_time, None);

        assert_eq!(
            resolve_episodic_anchors(&input(
                Some("when the moons align"),
                Some(ANCHOR),
                Some(WINDOW_START),
                "realtime",
            )),
            EpisodicAnchors {
                occurred_at: Some(WINDOW_START.to_string()),
                narrative_time: None
            }
        );
    }

    /// `it('never resolves without a reference (turn-extractor shape with no
    /// anchor)')`
    #[test]
    fn never_resolves_without_a_reference_turn_extractor_shape_with_no_anchor() {
        assert_eq!(
            resolve_episodic_anchors(&input(Some("yesterday"), None, None, "realtime")),
            EpisodicAnchors {
                occurred_at: None,
                narrative_time: None
            }
        );
    }

    /// `it('preserves the raw phrase as narrativeTime on fictional timelines')`
    #[test]
    fn preserves_the_raw_phrase_as_narrative_time_on_fictional_timelines() {
        assert_eq!(
            resolve_episodic_anchors(&input(
                Some("the third night of the siege"),
                Some(ANCHOR),
                Some(ANCHOR),
                "narrative",
            )),
            EpisodicAnchors {
                occurred_at: Some(ANCHOR.to_string()),
                narrative_time: Some("the third night of the siege".to_string())
            }
        );
    }

    /// `it('lets an explicit narrativeTime outrank the phrase, and survive
    /// realtime mode')`
    #[test]
    fn lets_an_explicit_narrative_time_outrank_the_phrase_and_survive_realtime_mode() {
        for mode in ["narrative", "realtime"] {
            let got = resolve_episodic_anchors(&EpisodicAnchorInput {
                narrative_time: Some("Midwinter Eve"),
                ..input(Some("yesterday"), Some(ANCHOR), Some(ANCHOR), mode)
            });
            assert_eq!(
                got.narrative_time.as_deref(),
                Some("Midwinter Eve"),
                "{mode}"
            );
        }
    }
}
