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
