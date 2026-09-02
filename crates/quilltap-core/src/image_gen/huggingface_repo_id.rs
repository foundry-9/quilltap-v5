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
/// **Scope, said out loud.** Reproduced from WHATWG's special-scheme parse:
/// `\` is a path separator and terminates the authority; single- and
/// double-dot path segments (raw or percent-encoded, case-insensitive) are
/// resolved; the host is percent-DECODED before the forbidden-code-point check
/// and lowercased; a non-bracketed port must be empty or decimal ≤ 65535 or the
/// parse throws. NOT reproduced: percent-ENCODING of the path (a raw space
/// becomes `%20` there — the pattern rejects both spellings) and IDNA on a
/// non-ASCII host (the pattern rejects the candidate either way; the host is
/// compared against an ASCII suffix). The first four DO change candidates the
/// pattern accepts — `/./owner/name`, `/a/../owner/name`, `\owner\name`,
/// `huggingface%2Eco` all resolve in v4, and `:abc` / `:99999` throw — which is
/// why they are arms here and rows in the corpus (the P4.D138 follow-up
/// review's catch: the first draft called them unreachable).
fn parse_http_url(url: &str) -> Option<HttpUrl> {
    let after_scheme = url.split_once("://")?.1;
    // A special scheme treats `\` exactly as `/`.
    let after_scheme = after_scheme.replace('\\', "/");
    let after_scheme = after_scheme.as_str();
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
            Some(i) => {
                // The port must be empty or decimal ≤ 65535, else `new URL` throws.
                let port = &host_port[i + 1..];
                if !port.is_empty()
                    && (!port.bytes().all(|b| b.is_ascii_digit())
                        || port.parse::<u32>().map(|p| p > 65535).unwrap_or(true))
                {
                    return None;
                }
                &host_port[..i]
            }
            None => host_port,
        }
    };
    if host.is_empty() {
        return None; // a special scheme demands a host
    }
    // The host is percent-DECODED before validation (`huggingface%2Eco` is
    // `huggingface.co` to `new URL`).
    let host = percent_decode(host);
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
            resolve_dot_segments(pathname)
        },
    })
}

/// WHATWG path-state dot resolution: a single-dot segment (`.`, `%2e`) is
/// dropped; a double-dot segment (`..`, `.%2e`, `%2e.`, `%2e%2e`) pops the
/// previous one. A trailing dot segment keeps the trailing slash.
fn resolve_dot_segments(pathname: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let raw: Vec<&str> = pathname.split('/').collect();
    let n = raw.len();
    for (i, seg) in raw.iter().enumerate() {
        if i == 0 {
            continue; // the empty segment before the leading `/`
        }
        let lower = seg.to_ascii_lowercase();
        let last = i == n - 1;
        match lower.as_str() {
            "." | "%2e" => {
                if last {
                    out.push("");
                }
            }
            ".." | ".%2e" | "%2e." | "%2e%2e" => {
                out.pop();
                if last {
                    out.push("");
                }
            }
            _ => out.push(seg),
        }
    }
    format!("/{}", out.join("/"))
}

/// `%XX` → the byte, for the host only (a bad or partial escape is kept as-is,
/// as the percent-decoder does).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
