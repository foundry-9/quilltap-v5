//! Pure port of v4's `rewriteLocalhostUrl` (`lib/host-rewrite.ts`).
//!
//! v4 rewrites `localhost` / `127.0.0.1` URLs to the host gateway when running
//! inside a VM/container so a user-configured `http://localhost:11434` (Ollama,
//! LM Studio) reaches the host. v4's function is impure — it reads the VM
//! environment (`isVMEnvironment()`) and resolves the gateway from
//! env/`/proc`/`/etc/hosts`. That environment probe + gateway resolution is a
//! HOST-tier concern; in the sans-IO core the **pure** part is the URL rewrite
//! itself, with the resolved gateway injected.
//!
//! [`rewrite_localhost_url`] takes `gateway: Option<&str>`:
//!   * `None` — v4's "not in a VM environment" OR "gateway resolution failed":
//!     both no-op, returning the URL unchanged.
//!   * `Some(host)` — the resolved gateway host: rewrite localhost URLs to it.
//!
//! **Reproducing `new URL(url).toString()`.** v4 does `new URL(url)`, swaps
//! `.hostname`, and re-serializes. That serialization normalizes: it lowercases
//! scheme + host, drops the default port for the scheme (80/http, 443/https),
//! makes an empty path `"/"`, and preserves userinfo / non-default port / query /
//! fragment. This module reproduces that for the bounded space the rewrite
//! actually sees (http/https/ws/wss/ftp URLs whose host is a localhost variant —
//! everything else passes through unchanged, exactly as v4 returns the URL
//! unchanged for a non-localhost host or an unparseable URL). Exotic URL shapes
//! (non-special schemes, IDNA hosts, percent-encoded authorities) are a
//! documented seam — the corpus stays on the real Ollama/LM-Studio shapes; a URL
//! this parser cannot confidently handle is returned unchanged (a safe no-op,
//! never a wrong rewrite).

/// Hostnames v4 treats as loopback (`LOCALHOST_HOSTS`).
const LOCALHOST_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]", "::1"];

/// The default port for a special scheme (dropped by `URL.toString()`), matching
/// the WHATWG special-scheme port table for the schemes the rewrite can see.
fn default_port(scheme: &str) -> Option<&'static str> {
    match scheme {
        "http" | "ws" => Some("80"),
        "https" | "wss" => Some("443"),
        "ftp" => Some("21"),
        _ => None,
    }
}

/// A parsed `scheme://[userinfo@]host[:port][/path][?query][#fragment]` for the
/// bounded rewrite space. Path/query/fragment are kept verbatim (not
/// re-normalized) — the only normalization v4 applies that we reproduce is the
/// authority (scheme/host lowercase, default-port drop) and the empty-path `/`.
struct SplitUrl<'a> {
    scheme: String,
    userinfo: Option<&'a str>,
    host: String,
    port: Option<&'a str>,
    rest: &'a str, // path + query + fragment, verbatim (leading char kept)
}

/// Parse the subset. Returns `None` if the URL is not of the recognized
/// `scheme://authority…` shape (→ caller passes it through unchanged).
fn split_url(url: &str) -> Option<SplitUrl<'_>> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end];
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
    {
        return None;
    }
    let after = &url[scheme_end + 3..];
    // authority ends at the first '/', '?' or '#'
    let authority_end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let authority = &after[..authority_end];
    let rest = &after[authority_end..];

    // userinfo (last '@' in the authority)
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
        None => (None, authority),
    };

    // host + optional port. An IPv6 literal host is bracketed `[...]`; a port
    // follows the closing bracket. Otherwise the port is after the last ':'.
    let (host, port) = if let Some(close) = hostport.find(']') {
        // bracketed IPv6
        let host = &hostport[..=close];
        let after_bracket = &hostport[close + 1..];
        let port = after_bracket.strip_prefix(':');
        (host, port)
    } else if let Some(colon) = hostport.rfind(':') {
        (&hostport[..colon], Some(&hostport[colon + 1..]))
    } else {
        (hostport, None)
    };

    if host.is_empty() {
        return None;
    }

    Some(SplitUrl {
        scheme: scheme.to_ascii_lowercase(),
        userinfo,
        host: host.to_ascii_lowercase(),
        port,
        rest,
    })
}

/// Rewrite a localhost URL to point at the host gateway.
///
/// No-ops when `gateway` is `None` (not in a VM / resolution failed), when the
/// URL is not parseable, or when the URL's host is not a localhost variant —
/// returning the URL unchanged in every such case (v4's early-return semantics).
pub fn rewrite_localhost_url(url: &str, gateway: Option<&str>) -> String {
    let Some(gateway) = gateway else {
        // v4: `!isVMEnvironment()` OR gateway resolution failed → unchanged.
        return url.to_string();
    };

    let Some(parts) = split_url(url) else {
        // Unparseable / out-of-subset → unchanged (v4's `try { new URL } catch`).
        return url.to_string();
    };

    // Only localhost variants are rewritten. The parsed host is lowercased; the
    // `::1` variant is compared unbracketed but a bare `::1` authority parses as
    // host `[` ... — so we compare against both the bracketed and bare forms.
    if !LOCALHOST_HOSTS.contains(&parts.host.as_str()) {
        return url.to_string();
    }

    // Re-serialize with the gateway host, reproducing `URL.toString()`:
    //   scheme://[userinfo@]gateway[:port-if-not-default]rest(empty → "/")
    let mut out = String::with_capacity(url.len() + gateway.len());
    out.push_str(&parts.scheme);
    out.push_str("://");
    if let Some(ui) = parts.userinfo {
        out.push_str(ui);
        out.push('@');
    }
    out.push_str(gateway);
    if let Some(port) = parts.port {
        // Drop the default port for the scheme, keep otherwise.
        if default_port(&parts.scheme) != Some(port) {
            out.push(':');
            out.push_str(port);
        }
    }
    if parts.rest.is_empty() {
        out.push('/');
    } else {
        out.push_str(parts.rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_gateway_passthrough() {
        // not in a VM / resolution failed
        assert_eq!(
            rewrite_localhost_url("http://localhost:11434", None),
            "http://localhost:11434"
        );
    }

    #[test]
    fn non_localhost_passthrough() {
        assert_eq!(
            rewrite_localhost_url("https://api.openai.com/v1", Some("gw")),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn invalid_url_passthrough() {
        assert_eq!(rewrite_localhost_url("not a url", Some("gw")), "not a url");
    }

    #[test]
    fn rewrites_localhost_and_adds_trailing_slash() {
        assert_eq!(
            rewrite_localhost_url("http://localhost:11434", Some("192.168.1.1")),
            "http://192.168.1.1:11434/"
        );
    }

    #[test]
    fn rewrites_and_preserves_path_query_fragment() {
        assert_eq!(
            rewrite_localhost_url("http://localhost:11434/api/chat?x=1#frag", Some("gw")),
            "http://gw:11434/api/chat?x=1#frag"
        );
        assert_eq!(
            rewrite_localhost_url(
                "http://localhost:11434/api/chat",
                Some("host.docker.internal")
            ),
            "http://host.docker.internal:11434/api/chat"
        );
    }

    #[test]
    fn rewrites_127_0_0_1() {
        assert_eq!(
            rewrite_localhost_url("http://127.0.0.1:8080/v1", Some("10.0.0.1")),
            "http://10.0.0.1:8080/v1"
        );
    }

    #[test]
    fn rewrites_ipv6_loopback() {
        assert_eq!(
            rewrite_localhost_url("https://[::1]:443/x", Some("gw")),
            "https://gw/x"
        );
    }

    #[test]
    fn drops_default_port_keeps_nondefault() {
        assert_eq!(
            rewrite_localhost_url("http://localhost:80/x", Some("gw")),
            "http://gw/x"
        );
        assert_eq!(
            rewrite_localhost_url("https://localhost:443/x", Some("gw")),
            "https://gw/x"
        );
        assert_eq!(
            rewrite_localhost_url("http://localhost:443/x", Some("gw")),
            "http://gw:443/x"
        );
    }

    #[test]
    fn preserves_userinfo() {
        assert_eq!(
            rewrite_localhost_url("http://user:pass@localhost:11434/x", Some("gw")),
            "http://user:pass@gw:11434/x"
        );
    }

    #[test]
    fn case_normalizes_scheme_and_host() {
        assert_eq!(
            rewrite_localhost_url("HTTP://LOCALHOST:11434/API", Some("gw")),
            "http://gw:11434/API"
        );
    }

    #[test]
    fn no_path_no_port() {
        assert_eq!(
            rewrite_localhost_url("http://localhost", Some("gw")),
            "http://gw/"
        );
    }
}
