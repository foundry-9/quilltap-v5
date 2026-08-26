//! WebSocket upgrade authentication (v4 `lib/realtime/upgrade-auth.ts`,
//! `f3892158d`) — the one leg of it that applies to v5.
//!
//! A raw upgrade request never reaches the normal request pipeline, so nothing
//! has looked at it by the time a handler gets a socket. v4 gates BOTH its
//! WebSocket handlers on three checks; here is what each becomes:
//!
//! * **A live session — NO-PORT.** v4's own note explains why it is thin even
//!   there: "There is no session cookie: Quilltap is single-user and
//!   `getServerSession()` resolves the instance's one user straight out of the
//!   database". v5 has **no session auth at all, by design (D2)** —
//!   `terminal_routes.rs`'s header says exactly this.
//! * **Not locked — ALREADY COVERED, verified not duplicated.**
//!   `terminal_routes::manager_and_db` answers 503 before the upgrade whenever
//!   the host or DB is absent or locked, which is v4's check one step earlier
//!   (v4 refuses after upgrading; v5 never upgrades). Nothing to add.
//! * **Same origin — THE REAL PORT, and this module.** v5's terminal WS had no
//!   origin check at all. Browsers do not apply CORS to WebSocket upgrades, so
//!   without one, a page on any origin can open a socket against a localhost
//!   instance and read whatever it streams.
//!
//! v4's accept/refuse table, ported arm for arm and pinned against v4's real
//! `authenticateUpgrade` by `terminal_ws_origin_equivalence`.

/// v4 `WS_CLOSE_POLICY_VIOLATION`.
pub const WS_CLOSE_POLICY_VIOLATION: u16 = 1008;

/// Refuse an upgrade whose `Origin` names a different host than `Host`.
/// Returns `None` when the request is acceptable, else v4's reason sentence.
///
/// The arms, in v4's order:
///
/// 1. **No `Origin` at all — an EMPTY one included, v4's `!origin` being a
///    truthiness test — or the opaque `null` origin → ALLOWED.** Not a
///    browser page acting on behalf of another site, so the hijacking threat
///    does not apply — non-browser clients (`wscat`, integration tests, a
///    shell's own probes) are not the threat model.
/// 2. An `Origin` with no `Host` → refused.
/// 3. `URL(origin).host !== host` → refused. Note this compares HOST (host +
///    port), not hostname: a different port is a different origin.
/// 4. An unparseable `Origin` → refused.
pub fn check_origin(origin: Option<&str>, host: Option<&str>) -> Option<String> {
    // v4's `if (!origin || origin === 'null') return null` — JS TRUTHINESS, so
    // an EMPTY `Origin:` header is falsy and allowed, not an unparseable one.
    // (The differential caught this: `Option<&str>` alone reads an empty string
    // as present.)
    let origin = origin.filter(|o| !o.is_empty())?;
    if origin == "null" {
        return None;
    }

    let Some(host) = host else {
        return Some("request has an Origin but no Host".to_string());
    };

    match url::Url::parse(origin) {
        Ok(url) => {
            // WHATWG `URL.host` — hostname plus `:port` when the port is not
            // the scheme's default.
            let origin_host = match (url.host_str(), url.port()) {
                (Some(h), Some(p)) => format!("{h}:{p}"),
                (Some(h), None) => h.to_string(),
                (None, _) => {
                    return Some(format!("unparseable Origin header ({origin})"));
                }
            };
            if origin_host != host {
                Some(format!(
                    "cross-origin upgrade (origin {origin_host} vs host {host})"
                ))
            } else {
                None
            }
        }
        Err(_) => Some(format!("unparseable Origin header ({origin})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_null_origin_is_allowed() {
        assert_eq!(check_origin(None, Some("localhost:4319")), None);
        assert_eq!(check_origin(Some("null"), Some("localhost:4319")), None);
        // …even with no Host either.
        assert_eq!(check_origin(None, None), None);
        // An EMPTY header is falsy in v4's `!origin`, so it is ALLOWED — not
        // refused as unparseable. Found by the differential.
        assert_eq!(check_origin(Some(""), Some("localhost:4319")), None);
    }

    #[test]
    fn a_matching_origin_is_allowed() {
        assert_eq!(
            check_origin(Some("http://localhost:4319"), Some("localhost:4319")),
            None
        );
        assert_eq!(
            check_origin(Some("https://127.0.0.1:8443"), Some("127.0.0.1:8443")),
            None
        );
    }

    #[test]
    fn a_cross_origin_upgrade_is_refused() {
        assert_eq!(
            check_origin(Some("http://evil.example"), Some("localhost:4319")),
            Some("cross-origin upgrade (origin evil.example vs host localhost:4319)".to_string())
        );
    }

    /// A different PORT is a different origin — the localhost case that
    /// matters most for a desktop app.
    #[test]
    fn a_different_port_is_cross_origin() {
        assert!(check_origin(Some("http://localhost:9999"), Some("localhost:4319")).is_some());
    }

    #[test]
    fn an_origin_without_a_host_header_is_refused() {
        assert_eq!(
            check_origin(Some("http://localhost:4319"), None),
            Some("request has an Origin but no Host".to_string())
        );
    }

    #[test]
    fn an_unparseable_origin_is_refused() {
        assert_eq!(
            check_origin(Some("not a url"), Some("localhost:4319")),
            Some("unparseable Origin header (not a url)".to_string())
        );
    }
}
