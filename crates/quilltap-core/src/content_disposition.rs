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
        encode_uri_component(filename)
    )
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
}
