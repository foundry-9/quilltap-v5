//! Pure port of v4's `rewriteLocalhostUrl` (`lib/host-rewrite.ts`).
//!
//! v4 rewrites `localhost` / `127.0.0.1` URLs to the host gateway when running
//! inside a container so a user-configured `http://localhost:11434` (Ollama,
//! LM Studio) reaches the host. v4's function is impure — it reads the
//! environment (`isVMEnvironment()`) and resolves the gateway. That environment
//! probe + gateway resolution is a HOST-tier concern; in the sans-IO core the
//! **pure** part is the URL rewrite itself, with the resolved gateway injected.
//!
//! [`rewrite_localhost_url`] takes `gateway: Option<&str>`:
//!   * `None` — v4's "no rewriting environment" OR "gateway resolution failed":
//!     both no-op, returning the URL unchanged.
//!   * `Some(host)` — the resolved gateway host: rewrite localhost URLs to it.
//!
//! ## What the injected gateway means since v4 `1560bd43b`
//!
//! v4 collapsed five gateway strategies to two, and redefined the environment
//! test with them:
//!   1. `QUILLTAP_HOST_IP` — the explicit override, and now ALSO the only
//!      supported route for a self-managed VM, which Quilltap has no reliable
//!      way to detect on its own.
//!   2. In Docker: `host.docker.internal` (Docker Desktop DNS, or `--add-host`
//!      on Linux, does the forwarding).
//!   3. Otherwise give up gracefully and return the URL unchanged.
//!
//! `isVMEnvironment()` is therefore `isDockerEnvironment() || QUILLTAP_HOST_IP
//! is set` — an env var is how a hand-rolled VM opts in.
//!
//! The three deleted strategies (the WSL2 `/etc/resolv.conf` nameserver, the
//! `/proc/net/route` default gateway, and the `/etc/hosts` lookup of
//! `host.docker.internal`) existed only for Lima and WSL2, and the middle one
//! was **actively wrong for Docker**: the bridge gateway it returns (e.g.
//! `172.17.0.1`) is just the bridge interface — services listening on the
//! host's own loopback are not reachable through it. That is why the collapsed
//! order has no `/proc/net/route` fallback under the Docker arm.
//!
//! **The resolver lives in the host** (P4.71): `quilltap_host::host_gateway`
//! ports v4's `isVMEnvironment()` + `resolveHostGateway()` — the environment
//! probe and the two-strategy ladder — and injects the result here through
//! `with_localhost_gateway` at every provider construction site. Until P4.71
//! that injection did not exist and the gateway was `None` on every production
//! path; the gap is closed, and `is_localhost_url` below exists so the host can
//! reproduce v4's LOG ORDER (v4 checks the host BEFORE it resolves, so a
//! non-localhost URL never emits a resolution line).
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

/// Whether `host` (already lowercased by [`split_url`]) is one of v4's
/// `LOCALHOST_HOSTS`. The single home for the test, shared by
/// [`rewrite_localhost_url`] and [`is_localhost_url`] so the two cannot drift.
fn is_localhost_host(host: &str) -> bool {
    LOCALHOST_HOSTS.contains(&host)
}

/// Whether `url` is a URL v4's `rewriteLocalhostUrl` would rewrite — i.e. it
/// parses AND its host is a localhost variant.
///
/// Exposed for the host's logged wrapper, which must reproduce v4's ORDER:
/// `rewriteLocalhostUrl` returns early for a non-localhost URL *before* calling
/// `resolveHostGateway()`, so resolution — and its `info`/`warn` line — never
/// happens for `https://api.openai.com/v1`. A wrapper that resolved first would
/// log where v4 is silent.
pub fn is_localhost_url(url: &str) -> bool {
    split_url(url).is_some_and(|p| is_localhost_host(&p.host))
}

/// Rewrite a localhost URL to point at the host gateway.
///
/// No-ops when `gateway` is `None` (no rewriting environment / resolution failed), when the
/// URL is not parseable, or when the URL's host is not a localhost variant —
/// returning the URL unchanged in every such case (v4's early-return semantics).
pub fn rewrite_localhost_url(url: &str, gateway: Option<&str>) -> String {
    let Some(gateway) = gateway else {
        // v4: `!isVMEnvironment()` OR gateway resolution failed → unchanged.
        // (`isVMEnvironment()` since `1560bd43b`: in Docker, or QUILLTAP_HOST_IP
        // is set — see the module header.)
        return url.to_string();
    };

    let Some(parts) = split_url(url) else {
        // Unparseable / out-of-subset → unchanged (v4's `try { new URL } catch`).
        return url.to_string();
    };

    // Only localhost variants are rewritten. The parsed host is lowercased; the
    // `::1` variant is compared unbracketed but a bare `::1` authority parses as
    // host `[` ... — so we compare against both the bracketed and bare forms.
    if !is_localhost_host(&parts.host) {
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
        // no rewriting environment / resolution failed
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
