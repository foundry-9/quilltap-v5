//! `Content-Disposition` construction — v4 `lib/api/content-disposition.ts`
//! (`b3ee00f1`, which deduplicated two byte-identical copies out of
//! `app/api/v1/files/[id]/shared.ts` and the file-proxy route into one shared
//! helper).
//!
//! RFC 5987: a plain `filename="…"` for ASCII names; an ASCII fallback plus
//! `filename*=UTF-8''…` when the name carries anything else. The Markdown
//! transcript export is the first v5 caller that can actually REACH the second
//! arm — a chat title survives into the filename with its non-ASCII intact,
//! where the SillyTavern export's name is built from ASCII furniture.
//!
//! It lives in the core (not at the web edge) for the same reason v4 put it in
//! `lib/api/`: it is a pure string function, its two consumers are transports,
//! and the differential can only drive the REAL helper — rather than a copy of
//! it living beside the assertion — from a crate the harness depends on.

/// The `disposition` parameter — v4's `'inline' | 'attachment'` (default
/// `inline`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    Inline,
    Attachment,
}

impl Disposition {
    fn as_str(self) -> &'static str {
        match self {
            Disposition::Inline => "inline",
            Disposition::Attachment => "attachment",
        }
    }
}

/// v4 `buildContentDisposition`.
///
/// The ASCII fallback replaces each non-ASCII **UTF-16 code unit** with `_`,
/// because that is what JS `String.replace(/[^\x00-\x7F]/g, '_')` does without
/// the `u` flag: an astral character (one Rust `char`, two code units) becomes
/// TWO underscores. A `chars()`-based port emits one and diverges on the first
/// emoji in a chat title.
pub fn build_content_disposition(filename: &str, disposition: Disposition) -> String {
    let d = disposition.as_str();
    if filename.is_ascii() {
        return format!("{d}; filename=\"{filename}\"");
    }
    let ascii: String = filename
        .encode_utf16()
        .map(|u| if u < 0x80 { u as u8 as char } else { '_' })
        .collect();
    format!(
        "{d}; filename=\"{ascii}\"; filename*=UTF-8''{}",
        encode_ext_value(filename)
    )
}

/// The RFC 8187 ext-value encoding — [`encode_uri_component`] plus the stray
/// `attr-char` violators `encodeURIComponent` leaves behind.
///
/// `encodeURIComponent` is the right base but too permissive: it keeps
/// `' ( ) * !`, and every one of those sits outside RFC 8187's `attr-char`. The
/// apostrophe is the worst offender — it is the **delimiter** in
/// `charset'language'value`, so an unescaped `'` inside the value makes the whole
/// `filename*` parameter ungrammatical and browsers discard it, falling back to
/// the underscored ASCII `filename` (found on a chat called
/// `Wings Over Suparṇā's Quiet Governance`, dogfood #46). Escape the full set,
/// byte-for-byte with v4's `encodeExtValue` (`ea4dc011`): v5 originally escaped
/// only `'` (the dogfood-#46 fix), then v4 went further and widened the set — so
/// this now MATCHES v4 rather than diverging from it, and the carve-out in
/// `markdown_transcript_equivalence` is retired.
///
/// Scoped deliberately to this one call: [`encode_uri_component`] is a faithful
/// `encodeURIComponent` port and has another consumer (`api::ui_search`) that
/// wants exactly JS's behaviour, so the widening lives here rather than in it.
fn encode_ext_value(s: &str) -> String {
    // v4's `/['()*!]/g → %XX-upper`. The five sets are disjoint and none
    // introduces a char in another's set, so the chained replaces are
    // order-independent and byte-identical to the single-pass regex.
    encode_uri_component(s)
        .replace('\'', "%27")
        .replace('(', "%28")
        .replace(')', "%29")
        .replace('*', "%2A")
        .replace('!', "%21")
}

/// JS `encodeURIComponent` over the unreserved set it keeps
/// (`A–Z a–z 0–9 - _ . ! ~ * ' ( )`).
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(*b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_takes_the_plain_arm() {
        assert_eq!(
            build_content_disposition("The Test Salon_transcript.md", Disposition::Attachment),
            "attachment; filename=\"The Test Salon_transcript.md\""
        );
    }

    #[test]
    fn astral_characters_cost_two_underscores() {
        // JS counts UTF-16 code units: 🎩 is a surrogate pair.
        let got = build_content_disposition("a🎩b.md", Disposition::Inline);
        assert!(got.contains("filename=\"a__b.md\""), "{got}");
        assert!(got.ends_with("filename*=UTF-8''a%F0%9F%8E%A9b.md"), "{got}");
    }

    /// Dogfood #46 / bug 41: an apostrophe must not reach the ext-value
    /// unescaped — it is RFC 8187's `charset'language'value` delimiter, so
    /// browsers discard the whole parameter and fall back to the underscored
    /// ASCII name. v4 now escapes it too (`ea4dc011`), so this matches v4.
    #[test]
    fn an_apostrophe_is_percent_encoded_in_the_ext_value() {
        let got = build_content_disposition(
            "Wings Over Suparṇā's Quiet Governance.md",
            Disposition::Attachment,
        );
        // The ASCII fallback is unchanged: two non-ASCII chars, two underscores,
        // and the apostrophe stays literal inside the quoted string.
        assert!(
            got.contains(r#"filename="Wings Over Supar__'s Quiet Governance.md""#),
            "{got}"
        );
        // The ext-value carries exactly ONE apostrophe pair — the delimiter after
        // `UTF-8` — and none inside the value.
        let ext = got
            .split("filename*=UTF-8''")
            .nth(1)
            .expect("ext-value present");
        assert!(
            !ext.contains('\''),
            "ext-value must not contain a raw apostrophe: {ext}"
        );
        assert!(ext.contains("%27"), "{ext}");
        assert_eq!(
            got.matches('\'').count(),
            3,
            "two delimiter quotes + the ASCII fallback's: {got}"
        );
    }

    /// Bug 41 (`ea4dc011`): the full RFC 8187 stray set `' ( ) * !` is escaped,
    /// byte-for-byte with v4's own test vector.
    #[test]
    fn stray_attr_chars_are_all_escaped_beside_non_ascii() {
        // v4's test: `a'()*!—b.txt` → `filename*=UTF-8''a%27%28%29%2A%21%E2%80%94b.txt`
        // (the em dash — U+2014 — is what takes the filename* branch).
        let got = build_content_disposition("a'()*!—b.txt", Disposition::Attachment);
        assert!(
            got.contains("filename*=UTF-8''a%27%28%29%2A%21%E2%80%94b.txt"),
            "{got}"
        );
        // The ASCII fallback leaves `' ( ) * !` literal (only non-ASCII → `_`).
        assert!(got.contains(r#"filename="a'()*!_b.txt""#), "{got}");
    }

    /// The scoping half: `encode_uri_component` itself stays a faithful
    /// `encodeURIComponent` (its other consumer wants JS behaviour exactly) — the
    /// widening lives in `encode_ext_value`, not here.
    #[test]
    fn encode_uri_component_still_keeps_the_stray_chars() {
        assert_eq!(
            encode_uri_component("it's (a) *test*!"),
            "it's%20(a)%20*test*!"
        );
    }
}
