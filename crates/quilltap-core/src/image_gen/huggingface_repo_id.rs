//! Reading a HuggingFace repository id out of a LoRA source (v4
//! `lib/image-gen/huggingface-repo-id.ts`, `2ece98c90`).
//!
//! Split out of the lookup for one reason, which v4 states: the LoRA editor
//! needs to know whether a source is even askable-about before it offers a
//! Query button, and that decision runs in the browser. v4's module is
//! therefore pure and dependency-free, and so is this one — the SPA carries its
//! own transcription (`apps/web/.../huggingface-repo-id.ts`, P4.D139) and this
//! is the host-side twin the lookup uses.

/// v4 `HUGGINGFACE_SITE_BASE`.
const HUGGINGFACE_SITE_BASE: &str = "https://huggingface.co";

/// v4 `REPO_ID_PATTERN` —
/// `/^[A-Za-z0-9][A-Za-z0-9._-]*\/[A-Za-z0-9][A-Za-z0-9._-]*$/`, hand-rolled
/// because every class in it is ASCII: owner and repository name as HuggingFace
/// spells them, in exactly two segments.
fn is_repo_id(s: &str) -> bool {
    fn segment(seg: &str) -> bool {
        let mut bytes = seg.bytes();
        match bytes.next() {
            Some(b) if b.is_ascii_alphanumeric() => {}
            _ => return false,
        }
        bytes.all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
    }
    let mut parts = s.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    segment(owner) && segment(name)
}

/// The `owner/name` inside a LoRA source, or `None` when there isn't one (v4
/// `extractHuggingFaceRepoId`).
///
/// Accepts a bare repo id and any huggingface.co URL — including the
/// `/resolve/main/weights.safetensors` form, which is how the fal-hosted models
/// usually want their adapters named. A weights URL on some other host has no
/// repository behind it and yields `None`, which is the editor's signal not to
/// offer the button at all.
pub fn extract_huggingface_repo_id(source: &str) -> Option<String> {
    // v4 `source.trim()` then `if (!trimmed)` — JS whitespace, JS falsy.
    let trimmed = crate::jsstr::js_trim(source);
    if trimmed.is_empty() {
        return None;
    }

    // `/^https?:\/\//i.test(trimmed)`.
    let lower_head: String = trimmed
        .chars()
        .take(8)
        .flat_map(char::to_lowercase)
        .collect();
    if lower_head.starts_with("http://") || lower_head.starts_with("https://") {
        let parsed = parse_http_url(trimmed)?; // v4's `new URL` throw → null
                                               // `/(^|\.)huggingface\.co$/i.test(parsed.hostname)`; `URL` has already
                                               // lowercased the hostname, so this compares lowercase.
        if parsed.hostname != "huggingface.co" && !parsed.hostname.ends_with(".huggingface.co") {
            return None;
        }
        // `pathname.split('/').filter(Boolean)` — the percent-ENCODED text, as
        // `URL.pathname` preserves it, so an encoded slash never becomes a
        // segment boundary and `%` fails the pattern (pinned by the corpus).
        let segments: Vec<&str> = parsed
            .pathname
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        if segments.len() < 2 {
            return None;
        }
        let candidate = format!("{}/{}", segments[0], segments[1]);
        return is_repo_id(&candidate).then_some(candidate);
    }

    is_repo_id(trimmed).then(|| trimmed.to_string())
}

/// The public model-card URL for a repository id (v4 `huggingFaceCardUrl`).
pub fn huggingface_card_url(repo_id: &str) -> String {
    format!("{HUGGINGFACE_SITE_BASE}/{repo_id}")
}

/// The two fields v4 reads off `new URL(...)`: the lowercased `hostname` and
/// the raw `pathname`.
struct HttpUrl {
    hostname: String,
    pathname: String,
}

/// A bounded stand-in for `new URL(trimmed)` over the space this function can
/// reach — the caller has already matched `^https?://`, so the scheme is
/// special and WHATWG's special-scheme rules apply.
///
/// `None` is v4's `catch { return null }`. The arms that throw and are pinned
/// by the corpus: an empty host (`https://`), an unterminated IPv6 literal
/// (`http://[bad`), and a host carrying a forbidden code point (a space).
///
/// **Scope, said out loud.** This does not percent-encode the path the way
/// `URL` would (a raw space in a path becomes `%20` there), does not apply
/// IDNA to a non-ASCII host, and does not resolve `.`/`..` segments. Every one
/// of those would change a candidate that the repo-id pattern rejects anyway —
/// the pattern admits only ASCII alphanumerics, dot, underscore and hyphen —
/// so the answer is `None` either way. The corpus pins the reachable arms.
fn parse_http_url(url: &str) -> Option<HttpUrl> {
    let after_scheme = url.split_once("://")?.1;
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    let path_and_rest = &after_scheme[authority_end..];

    // userinfo is everything before the LAST '@'.
    let host_port = match authority.rfind('@') {
        Some(i) => &authority[i + 1..],
        None => authority,
    };
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        // A bracketed IPv6 literal must close; `http://[bad` throws.
        let close = rest.find(']')?;
        &host_port[..close + 2]
    } else {
        match host_port.rfind(':') {
            Some(i) => &host_port[..i],
            None => host_port,
        }
    };
    if host.is_empty() {
        return None; // a special scheme demands a host
    }
    // WHATWG's forbidden host code points (the reachable subset): whitespace and
    // the delimiters that cannot appear in a host.
    if host.chars().any(|c| {
        c.is_whitespace() || matches!(c, '\0' | '<' | '>' | '"' | '#' | '%' | '?' | '^' | '|')
    }) {
        return None;
    }

    // The path is everything up to '?' or '#'; an empty path is `/`.
    let path_end = path_and_rest
        .find(['?', '#'])
        .unwrap_or(path_and_rest.len());
    let pathname = &path_and_rest[..path_end];
    Some(HttpUrl {
        hostname: host.to_lowercase(),
        pathname: if pathname.is_empty() {
            "/".to_string()
        } else {
            pathname.to_string()
        },
    })
}
