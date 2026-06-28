//! Differential harness library: shared corpus + equivalence helpers.
//! The actual assertions live in tests/ (run via `cargo test -p quilltap-harness`).

use quilltap_core::memory_weighting::MemoryInputs;

/// Fixed reference clock — MUST equal NOW in harness/oracle/cases/memory-weighting.ts
/// (2026-06-27T12:00:00.000Z). Expressed as epoch millis.
/// (Verified: `new Date("2026-06-27T12:00:00.000Z").getTime()` === this.)
pub const NOW_MS: f64 = 1_782_561_600_000.0;

/// Parse an ISO-8601 UTC instant (the exact subset the corpus uses:
/// `YYYY-MM-DDTHH:MM:SS.sssZ`) to epoch millis, matching JS `new Date(s).getTime()`.
/// Deliberately tiny and total over the corpus's well-formed inputs; the real
/// core will use a date crate. Panics on malformed input (a corpus bug, not a
/// runtime case).
pub fn iso_to_ms(s: &str) -> f64 {
    // Split "YYYY-MM-DDTHH:MM:SS.sssZ"
    let (date, rest) = s.split_once('T').expect("iso: missing T");
    let time = rest.strip_suffix('Z').expect("iso: missing Z");
    let mut dparts = date.split('-');
    let y: i64 = dparts.next().unwrap().parse().unwrap();
    let mo: i64 = dparts.next().unwrap().parse().unwrap();
    let d: i64 = dparts.next().unwrap().parse().unwrap();
    let (hms, millis) = match time.split_once('.') {
        Some((a, b)) => (a, b.parse::<i64>().unwrap()),
        None => (time, 0),
    };
    let mut tparts = hms.split(':');
    let h: i64 = tparts.next().unwrap().parse().unwrap();
    let mi: i64 = tparts.next().unwrap().parse().unwrap();
    let se: i64 = tparts.next().unwrap().parse().unwrap();

    // Days since Unix epoch via a civil-from-days algorithm (Howard Hinnant's),
    // matching the proleptic Gregorian calendar JS Date uses for UTC.
    let days = days_from_civil(y, mo, d);
    let secs = days * 86_400 + h * 3_600 + mi * 60 + se;
    (secs as f64) * 1000.0 + (millis as f64)
}

/// Days from 1970-01-01 for a civil (y, m, d), proleptic Gregorian. m in 1..=12.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Build the corpus — MUST mirror CORPUS in the TS oracle case exactly
/// (same ids, same fields, same timestamps).
pub fn corpus() -> Vec<(&'static str, MemoryInputs)> {
    let m = |importance: f64,
             reinforced: Option<f64>,
             created: &str,
             last_reinforced: Option<&str>,
             last_accessed: Option<&str>,
             reinforcement_count: Option<u64>,
             graph_degree: usize| MemoryInputs {
        importance,
        reinforced_importance: reinforced,
        created_at_ms: iso_to_ms(created),
        last_reinforced_at_ms: last_reinforced.map(iso_to_ms),
        last_accessed_at_ms: last_accessed.map(iso_to_ms),
        reinforcement_count,
        graph_degree,
    };

    vec![
        (
            "fresh-high",
            m(0.9, None, "2026-06-27T00:00:00.000Z", None, None, None, 0),
        ),
        (
            "old-high-floor",
            m(0.9, None, "2025-06-27T00:00:00.000Z", None, None, None, 0),
        ),
        (
            "reinforced-recent",
            m(
                0.5,
                Some(0.8),
                "2026-01-01T00:00:00.000Z",
                Some("2026-06-20T00:00:00.000Z"),
                None,
                None,
                0,
            ),
        ),
        (
            "retrieval-doesnt-reset",
            m(
                0.6,
                None,
                "2026-03-01T00:00:00.000Z",
                None,
                Some("2026-06-26T00:00:00.000Z"),
                None,
                0,
            ),
        ),
        (
            "graph-heavy",
            m(0.4, None, "2026-05-01T00:00:00.000Z", None, None, None, 8),
        ),
        (
            "reinforce-saturate",
            m(
                0.5,
                None,
                "2026-06-10T00:00:00.000Z",
                None,
                None,
                Some(64),
                0,
            ),
        ),
        (
            "recent-access-bonus",
            m(
                0.3,
                None,
                "2026-04-01T00:00:00.000Z",
                None,
                Some("2026-06-01T00:00:00.000Z"),
                None,
                0,
            ),
        ),
        (
            "stale-access",
            m(
                0.3,
                None,
                "2026-01-01T00:00:00.000Z",
                None,
                Some("2026-01-15T00:00:00.000Z"),
                None,
                0,
            ),
        ),
        (
            "zero-importance",
            m(0.0, None, "2026-06-01T00:00:00.000Z", None, None, None, 0),
        ),
        (
            "content-cap",
            m(1.0, None, "2026-06-27T06:00:00.000Z", None, None, None, 0),
        ),
    ]
}

#[cfg(test)]
mod self_tests {
    use super::*;
    #[test]
    fn now_constant_matches_iso() {
        // The NOW_MS constant must equal the parsed oracle NOW.
        assert_eq!(NOW_MS, iso_to_ms("2026-06-27T12:00:00.000Z"));
    }
}
