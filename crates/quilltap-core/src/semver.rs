//! Port of v4's lib/utils/semver.ts — parse a `major.minor.patch` prefix and
//! compare two version strings. Implemented without a regex dependency (the v4
//! pattern is a simple anchored digit-dot-digit-dot-digit prefix).

/// A parsed semver triple. Pre-release / build / extra-segment suffixes are
/// ignored (only the leading `major.minor.patch` is captured).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedVersion {
    pub major: i64,
    pub minor: i64,
    pub patch: i64,
}

/// Take the run of leading ASCII digits as an integer, returning it and the
/// remainder. `None` if there is no leading digit. (`\d+` in v4's regex is
/// ASCII-only — no Unicode flag.)
fn leading_digits(s: &str) -> Option<(i64, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    let (num, rest) = s.split_at(end);
    num.parse::<i64>().ok().map(|n| (n, rest))
}

/// Parse a semver string into `{major, minor, patch}`, or `None` when it lacks a
/// `\d+\.\d+\.\d+` prefix (after stripping a single leading `v`). Anything after
/// the third number is ignored.
pub fn parse_version(version: &str) -> Option<ParsedVersion> {
    // v4's `version.replace(/^v/, '')` strips a single leading 'v' only.
    let cleaned = version.strip_prefix('v').unwrap_or(version);
    let (major, rest) = leading_digits(cleaned)?;
    let rest = rest.strip_prefix('.')?;
    let (minor, rest) = leading_digits(rest)?;
    let rest = rest.strip_prefix('.')?;
    let (patch, _rest) = leading_digits(rest)?;
    Some(ParsedVersion {
        major,
        minor,
        patch,
    })
}

/// Compare two semver strings: `-1` if `a < b`, `0` if equal, `1` if `a > b`,
/// comparing major then minor then patch.
///
/// When either input fails to parse, v4 falls back to `a.localeCompare(b)` —
/// true ICU collation, reproduced here via [`crate::collation::locale_compare_i32`]
/// (ICU4X en-US/tertiary). This fires only on *malformed* version strings, where
/// code-unit ordering and `localeCompare` genuinely diverge (e.g. `"a"` vs `"B"`);
/// the ICU fallback now handles them faithfully.
pub fn compare_versions(a: &str, b: &str) -> i32 {
    match (parse_version(a), parse_version(b)) {
        (Some(pa), Some(pb)) => {
            if pa.major != pb.major {
                if pa.major < pb.major {
                    -1
                } else {
                    1
                }
            } else if pa.minor != pb.minor {
                if pa.minor < pb.minor {
                    -1
                } else {
                    1
                }
            } else if pa.patch != pb.patch {
                if pa.patch < pb.patch {
                    -1
                } else {
                    1
                }
            } else {
                0
            }
        }
        _ => crate::collation::locale_compare_i32(a, b),
    }
}
