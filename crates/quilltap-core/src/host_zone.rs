//! The host's display zone — ONE home (P4.119, dogfood #121, ruled (a)
//! 2026-09-29).
//!
//! v4 formats every human-readable date (the Post Office's letter dates and
//! reply prefaces, the Scriptorium transcript render, the web-search dates, the
//! Almanack, the progressions fallback) with `toLocaleString` /
//! `toLocaleDateString` / `Intl.DateTimeFormat` and **no `timeZone`**, i.e. in
//! the Node process's system zone. The harness runs v4 under `TZ=UTC`, so the
//! port once read "matches v4 under `TZ=UTC`" as "always format in UTC" — and
//! on a real instance that re-embedded every chat v5 re-rendered and persisted
//! wrong-zone times into v5-written letters. The ruling: format in the HOST
//! zone in production; `TZ=UTC` stays the differential's pin on both sides.
//!
//! **Zones are arguments.** Every display formatter takes a
//! `&jiff::tz::TimeZone` — a zone VALUE, not a name (a POSIX `TZ` string has no
//! IANA name, and `iana_name().unwrap_or("UTC")` would silently fall back) —
//! and renders each instant with that instant's own offset
//! (`Timestamp::to_zoned`), never one offset taken at `now` (which puts a
//! January letter an hour off when rendered in July). **Production reads
//! [`system_display_zone`] ONCE, at the composition root (`quilltap-host`), and
//! threads the VALUE down** (P4.127, completed by P4.140's Option V): the tool
//! executor, `CoreConfig`, the render-job handler, the Almanack, AND the Salon
//! turn (`ProcessMessageInput`), `build_context` on the turn and the swipe, the
//! autonomous step, the greeting and the spine's own runner and Scenario
//! Builder clock. Nothing re-derives a display zone from a NAME any more (the
//! P4.127 `display_zone_named` helper is retired: on a POSIX-`TZ` host it
//! rendered UTC wherever it ran, while the value — jiff 0.2.31 parses the rule —
//! showed the rule's offset). Nothing else in core reads the environment for a
//! DISPLAY zone (the `host_zone_sites_census` pins it); the remaining reader
//! of a NAME ([`system_zone_name`]'s callers — the autonomous-room schedule and
//! the markdown-transcript export) is P4.127's recorded Tier-3 leftover. Every
//! test and differential passes `TimeZone::UTC` (or a named second zone)
//! explicitly. Nothing here consults a process-global override —
//! `std::env::set_var("TZ")` in a test races every other test thread, and a
//! test that needs a second zone passes it by argument (or spawns a child
//! process with `TZ` set on the `Command`).
//!
//! **The calendar NAME residue (P4.140 Tier 3, recorded):** the host's `tz`
//! NAME ([`zone_name`] of the value — `"UTC"` for a POSIX rule) still drives
//! every CALENDAR path: cron (the schedule tick and the manual autonomous
//! routes, which must agree), the memory distill's TODAY line and day-reference
//! scan (`server_tz`), the zone-less timestamp offset (`js_local_offset_minutes`),
//! the story-zone fallback, the LLM-log cleanup and the Almanack's `timezone`
//! fact. Under a POSIX `TZ` those run on UTC while every rendered date honours
//! the rule; a value-taking cron + day-reference seam is its own order.
//!
//! **v4 does not honour a POSIX DST rule (recorded, not ported):** Node 24's
//! ICU ignores a `TZ` rule with DST and shows `/etc/localtime`'s zone (it
//! honours only a DST-less rule such as `XST-9`); v5 honours the rule, the ruled
//! shape. An unparseable `TZ` (`CDT`, `XST5XDT`) is a jiff `Err` and every zone
//! here falls to UTC where ICU falls to the system zone — jiff exposes no
//! "system zone ignoring `TZ`" constructor, so that is a ruling for the human.
//!
//! `quilltap-web`'s `main.rs` reconciles `QUILLTAP_TIMEZONE` / `TZ` before
//! boot, so Docker's setting reaches `TimeZone::try_system()` here.

/// Re-exported so a crate without its own `jiff` dependency (the harness —
/// this round moves no dependency blocks) can name and build zones for the
/// explicit-zone siblings: `TimeZone::UTC`, `TimeZone::get("America/Chicago")`.
pub use jiff::tz::TimeZone;
/// Re-exported for the same reason: a harness test that asserts a zone's offset
/// at an instant (`TimeZone::to_offset`) needs to build one.
pub use jiff::Timestamp;

/// The host's system zone as a VALUE, for display formatting — what v4's
/// zone-less `toLocale*` / `Intl.DateTimeFormat` calls resolve. UTC only when
/// the platform zone cannot be read at all (jiff's own `system()` would answer
/// `Etc/Unknown`, which behaves like UTC but prints a warning per call).
pub fn system_display_zone() -> TimeZone {
    TimeZone::try_system().unwrap_or(TimeZone::UTC)
}

/// The zone's IANA NAME, for the calendar paths that still take a name (cron,
/// the distill's `server_tz`, the cleanup — the module doc's residue): a zone
/// with no IANA name (a POSIX `TZ` rule, a fixed offset) answers `"UTC"`. ONE
/// home for the derivation (`HostConfig::new`, `HostConfig::set_display_zone`).
pub fn zone_name(zone: &TimeZone) -> &str {
    zone.iana_name().unwrap_or("UTC")
}

/// The host's system zone as an IANA NAME, for the two callers that still need
/// a name (the markdown-transcript export's `LocalOffset::Zone`, the
/// autonomous-room schedule; cron reads `HostConfig.tz`, derived from the value,
/// since P4.127). A zone with no IANA name (a fixed offset, a POSIX
/// `TZ` string, an unnamed TZif) falls back to `"UTC"` — the long-standing
/// behaviour of the three readers this consolidates.
pub fn system_zone_name() -> String {
    zone_name(&system_display_zone()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The helper IS the system zone (the only environment read in the
    /// display path) — compared against jiff's own reading rather than a
    /// literal, so the test is green on any machine and under any `TZ`.
    #[test]
    fn display_zone_is_the_system_zone() {
        let expected = TimeZone::try_system().unwrap_or(TimeZone::UTC);
        let ts = jiff::Timestamp::from_millisecond(1_767_225_600_000).unwrap();
        assert_eq!(system_display_zone().to_offset(ts), expected.to_offset(ts));
        assert_eq!(system_zone_name(), expected.iana_name().unwrap_or("UTC"));
    }

    /// The name derivation: an IANA zone names itself; a POSIX rule (no IANA
    /// name) and UTC answer `"UTC"`.
    #[test]
    fn zone_name_is_the_iana_name_or_utc() {
        let chicago = TimeZone::get("America/Chicago").unwrap();
        assert_eq!(zone_name(&chicago), "America/Chicago");
        let rule = TimeZone::posix("XST6XDT,M3.2.0,M11.1.0").unwrap();
        assert_eq!(rule.iana_name(), None);
        assert_eq!(zone_name(&rule), "UTC");
        assert_eq!(zone_name(&TimeZone::UTC), "UTC");
    }
}
