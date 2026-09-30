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
//! threads the result down** (P4.127): a `TimeZone` VALUE on the tool executor
//! (which both production builders fill from the threaded NAME), `CoreConfig`
//! and the render-job handler, and the already-threaded `server_tz` NAME through
//! [`display_zone_named`] at the Salon turn, `build_context` and the greeting.
//! Nothing else in core reads the environment for a DISPLAY zone (the
//! `host_zone_sites_census` pins it); the two remaining NAME readers
//! ([`system_zone_name`]'s callers — the autonomous-room schedule and the
//! markdown-transcript export) are the recorded Tier-3 leftovers of P4.127. Every test and differential passes
//! `TimeZone::UTC` (or a named second zone) explicitly. Nothing here
//! consults a process-global override — `std::env::set_var("TZ")` in a test
//! races every other test thread, and a test that needs a second zone passes
//! it by argument (or spawns a child process with `TZ` set on the `Command`).
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

/// The display zone for a zone NAME already threaded to the caller (the
/// `server_tz` the composition root hands the Salon turn, `build_context` and the
/// greeting — P4.127). An unknown or absent name resolves UTC.
///
/// **Ruled divergence (P4.127, recorded for the human):** the name is
/// `iana_name().unwrap_or("UTC")` of the host's zone, so a host whose zone has no
/// IANA name (a POSIX `TZ` string, a fixed offset) displays UTC at the name-fed
/// entries where v4 displays the host zone — the case the module doc's "a zone
/// VALUE, not a name" rule refuses. The name-fed entries are the Salon turn's
/// tool runner AND the spine-built runner (so every executor tool and Carina —
/// the executor's `TimeZone` VALUE is name-resolved by BOTH production
/// builders), `build_context` and the greeting; the value-fed entries
/// (`CoreConfig`'s Compose reply preface, the render job, the Almanack) keep
/// the real offsets, so on such a host the two persisted mail prefaces (Compose
/// vs the `send_mail` tool) render in different zones. It is the same fallback every
/// calendar path (`server_tz`, cron, the autonomous rooms, the export) already
/// lives with; threading a `TimeZone` VALUE through `ProcessMessageInput` /
/// `BuildContextInput` would honour the rule and is the recorded alternative.
pub fn display_zone_named(name: Option<&str>) -> TimeZone {
    name.and_then(|n| TimeZone::get(n).ok())
        .unwrap_or(TimeZone::UTC)
}

/// The host's system zone as an IANA NAME, for the two callers that still need
/// a name (the markdown-transcript export's `LocalOffset::Zone`, the
/// autonomous-room schedule; cron reads `HostConfig.tz`, derived from the value,
/// since P4.127). A zone with no IANA name (a fixed offset, a POSIX
/// `TZ` string, an unnamed TZif) falls back to `"UTC"` — the long-standing
/// behaviour of the three readers this consolidates.
pub fn system_zone_name() -> String {
    system_display_zone()
        .iana_name()
        .unwrap_or("UTC")
        .to_string()
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

    /// The named helper resolves an IANA name to THAT zone (never the
    /// environment's) and anything else to UTC.
    #[test]
    fn display_zone_named_resolves_the_name_or_utc() {
        // 2026-01-15T12:00Z — Chicago is on CST (UTC-6), Tokyo on JST (UTC+9).
        let ts = jiff::Timestamp::from_second(1_768_478_400).unwrap();
        let off = |z: &TimeZone| z.to_offset(ts).seconds();
        assert_eq!(off(&display_zone_named(Some("America/Chicago"))), -6 * 3600);
        assert_eq!(off(&display_zone_named(Some("Asia/Tokyo"))), 9 * 3600);
        assert_eq!(off(&display_zone_named(Some("UTC"))), 0);
        assert_eq!(off(&display_zone_named(None)), 0);
        assert_eq!(off(&display_zone_named(Some("Not/AZone"))), 0);
        assert_eq!(off(&display_zone_named(Some(""))), 0);
    }
}
