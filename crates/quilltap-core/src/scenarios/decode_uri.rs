//! JavaScript's `decodeURIComponent`, accept-or-throw for accept-or-throw.
//!
//! v4's `resolveScenarioPath` (`lib/mount-index/scenarios-common.ts:332`)
//! decodes every scenario path it normalises — `decodeURIComponent(path).trim()`
//! — and since `545e299bb` every caller of the scenario location goes through
//! it: a `scenarioRef.path` from a JSON body, `?action=read`'s `path` query
//! parameter, the resolver's three path fields, the item routes' reads and
//! writes. So `Scenarios/a%20b.md` reads `Scenarios/a b.md`, and a malformed
//! escape THROWS (`URIError: URI malformed`) — a 500 at the route, not a
//! refusal (ruling R-Z).
//!
//! The decoding rules are V8's `Decode` (`src/strings/uri.cc`): a `%` must be
//! followed by two hex digits; a byte below `0x80` decodes alone; a lead byte
//! starts a two-, three- or four-byte UTF-8 run whose continuation bytes must
//! each be `%`-escaped and in `0x80..=0xBF`; the run must be the shortest
//! encoding of a scalar value (no overlongs, no surrogates `U+D800..=U+DFFF`,
//! nothing past `U+10FFFF`). Unlike `decodeURI`, the reserved set (`/`, `?`,
//! `#`, …) IS decoded — `%2F` becomes `/`. Literal characters pass through.
//!
//! `doc_edit/qtap_uri.rs` carries a private twin of this decoder; folding it
//! onto this one is recorded for a later lane (L-6 — a file this lane does not
//! own), not done here.
//!
//! Pinned tier-1 against V8 by `scenario_pure_equivalence`.

/// V8's `URIError` for a malformed escape. Its `Display` is the message V8
/// gives (`URI malformed`), which is what v4's middleware would log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UriMalformed;

impl std::fmt::Display for UriMalformed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("URI malformed")
    }
}

impl std::error::Error for UriMalformed {}

/// JavaScript `decodeURIComponent(component)`: the decoded string, or V8's
/// `URIError` on any malformed escape or invalid UTF-8 run.
pub fn js_decode_uri_component(component: &str) -> Result<String, UriMalformed> {
    let bytes = component.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    // One `%XX` escape starting at `at` (the '%'): its byte and the index after.
    let read_pct = |at: usize| -> Result<(u8, usize), UriMalformed> {
        if at + 3 > bytes.len() {
            return Err(UriMalformed);
        }
        let h1 = (bytes[at + 1] as char).to_digit(16).ok_or(UriMalformed)?;
        let h2 = (bytes[at + 2] as char).to_digit(16).ok_or(UriMalformed)?;
        Ok(((h1 * 16 + h2) as u8, at + 3))
    };
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let (lead, mut next) = read_pct(i)?;
        if lead < 0x80 {
            out.push(lead);
            i = next;
            continue;
        }
        // The run's length from the lead byte's leading 1-bits; a lone
        // continuation byte (0x80..=0xBF) or an 0xF8+ lead is malformed.
        let len = match lead {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => return Err(UriMalformed),
        };
        let mut run = vec![lead];
        for _ in 1..len {
            if next >= bytes.len() || bytes[next] != b'%' {
                return Err(UriMalformed);
            }
            let (cont, after) = read_pct(next)?;
            if !(0x80..=0xBF).contains(&cont) {
                return Err(UriMalformed);
            }
            run.push(cont);
            next = after;
        }
        // Rust's UTF-8 validation refuses exactly what V8's does here:
        // overlongs, surrogates, and code points past U+10FFFF.
        match std::str::from_utf8(&run) {
            Ok(s) if s.chars().count() == 1 => out.extend_from_slice(&run),
            _ => return Err(UriMalformed),
        }
        i = next;
    }
    // Every byte copied through is either a literal from a valid `&str` or a
    // validated run, so this cannot fail; the `Err` arm keeps it total.
    String::from_utf8(out).map_err(|_| UriMalformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_refuses_like_v8() {
        assert_eq!(js_decode_uri_component("a%20b").as_deref(), Ok("a b"));
        assert_eq!(js_decode_uri_component("a%2Fb").as_deref(), Ok("a/b"));
        assert_eq!(js_decode_uri_component("%E2%80%A6").as_deref(), Ok("…"));
        assert_eq!(js_decode_uri_component("plain é").as_deref(), Ok("plain é"));
        for bad in [
            "%",
            "%E0",
            "%ZZ",
            "%ED%A0%80",
            "%C0%80",
            "%F4%90%80%80",
            "%80",
        ] {
            assert_eq!(js_decode_uri_component(bad), Err(UriMalformed), "{bad}");
        }
    }
}
