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
//! January letter an hour off when rendered in July). Production reads
//! [`system_display_zone`] once per entry point; every test and differential
//! passes `TimeZone::UTC` (or a named second zone) explicitly. Nothing here
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

/// The host's system zone as a VALUE, for display formatting — what v4's
/// zone-less `toLocale*` / `Intl.DateTimeFormat` calls resolve. UTC only when
/// the platform zone cannot be read at all (jiff's own `system()` would answer
/// `Etc/Unknown`, which behaves like UTC but prints a warning per call).
pub fn system_display_zone() -> TimeZone {
    TimeZone::try_system().unwrap_or(TimeZone::UTC)
}

/// The host's system zone as an IANA NAME, for the callers that need a name
/// (cron evaluation, the markdown-transcript export's `LocalOffset::Zone`, the
/// autonomous-room schedule). A zone with no IANA name (a fixed offset, a POSIX
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
}
