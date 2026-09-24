//! P4.67 — **the one query-parameter reader** for every v5 REST edge.
//!
//! v4 reads a query parameter three different ways, and the three disagree the
//! moment a key is REPEATED. Until this module every v5 edge extracted
//! `Query<HashMap<String, String>>`, which under `serde_urlencoded` keeps the
//! **last** occurrence — so `?limit=1&limit=5` answered `5` where v4's
//! `searchParams.get('limit')` answers `1`. The fix is not "make them all
//! first": the rule differs per site, so each read is classified by the v4
//! reader it mirrors and then spelled with the matching helper here.
//!
//! | v4 reader | wins | helper |
//! |---|---|---|
//! | `searchParams.get(k)` (the common case, incl. `getActionParam`) | FIRST | [`first`] |
//! | `getQueryParamsWithoutAction` — `searchParams.forEach` into a bag (`lib/api/middleware/actions.ts:180-193`) | LAST | *(no helper — see below)* |
//! | `searchParams.getAll(k)` (`app/api/v1/photos/route.ts:39`) | ALL, in order | [`all`] |
//!
//! There is deliberately **no LAST-wins helper**. `getQueryParamsWithoutAction`
//! is exported from v4's middleware and re-exported from its `index.ts`, but it
//! has **zero call sites** in `app/` or `lib/` at the oracle baseline — so no
//! v5 read mirrors it, and a helper for it would be a rule with nothing to
//! obey it. Add one, with its site, if v4 ever starts using it.
//!
//! Nothing but selection lives here, bar ONE rule: v4's `?action=` dispatch.
//! [`first`] PRESERVES the empty string for `?k=`, because `URLSearchParams.get`
//! does — any JS truthiness that turns `''` into "absent" belongs at the call
//! site, exactly where v4 spells it. The `?action=` rule is the exception
//! because v4 shares it too: since `ad1c4c37f` ("Consolidate API action
//! dispatch into single primitive") every v4 route reads its action through
//! ONE `dispatchAction`, and [`dispatch_action`] / [`dispatch_required_action`]
//! are its twin — absent → the route's default (or `Action parameter
//! required`), a known action → its handler, and **anything else, a bare
//! `?action=` included → `Unknown action: <x>`** with the route's
//! `availableActions`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use quilltap_core::content_disposition::Disposition;

/// The repeat-preserving query payload: `Query<QueryPairs>`.
///
/// `serde_urlencoded` deserializes a pair sequence in URL order, so repeats
/// survive — a `HashMap` silently collapses them.
pub(crate) type QueryPairs = Vec<(String, String)>;

/// v4 `request.nextUrl.searchParams.get(key)`.
///
/// The **first** occurrence of a repeated key; `Some("")` for `?key=` (present
/// but empty); `None` when the key is absent. The empty string is deliberately
/// preserved — see the module header.
pub(crate) fn first<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// v4 `wantsAttachment(request)` (`lib/api/content-disposition.ts:47`,
/// `86d59660c`) — whether a request asked for the bytes as a download rather
/// than inline.
///
/// The three routes that serve image bytes — `/files/{id}`,
/// `/files/proxy/{key}` and `/mount-points/{id}/blobs/{path}` — all answer
/// `inline` by default (the Salon embeds them in `<img>`) and all honour
/// `?download=1` by switching to `attachment`. `true` is accepted alongside
/// `1` so a hand-typed URL behaves the way a reader expects; **nothing else
/// is** — `?download=0`, `?download=yes` and a bare `?download=` are all
/// inline.
///
/// `download` is a `searchParams.get` read, so a repeated key takes the FIRST
/// occurrence ([`first`]).
///
/// **v4's malformed-URL arm has no counterpart and cannot.** v4 wraps
/// `new URL(request.url)` in a try/catch because its handler receives the URL
/// as a string; axum parses the request line before any handler runs, so a
/// request whose URL does not parse never reaches this predicate. Recorded as
/// a structural NO-COUNTERPART (P4.D174).
pub(crate) fn wants_attachment(pairs: &[(String, String)]) -> bool {
    matches!(first(pairs, "download"), Some("1") | Some("true"))
}

/// v4 `dispositionFor(request)` — `attachment` when the request asked for a
/// download, else `inline`.
pub(crate) fn disposition_for(pairs: &[(String, String)]) -> Disposition {
    if wants_attachment(pairs) {
        Disposition::Attachment
    } else {
        Disposition::Inline
    }
}

/// v4 `searchParams.getAll(key)` — **every** occurrence, in URL order.
pub(crate) fn all<'a>(pairs: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    pairs
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .collect()
}

/// Collapse the pair list to the map a handler already expects, keeping the
/// **first** occurrence of each key — i.e. `searchParams.get` for every key at
/// once.
///
/// Use this only on a route where EVERY query read mirrors `searchParams.get`
/// (the common case: v4 hand-rolled routes read all their params that way). A
/// route that mixes readers — one key through `getAll`, another through
/// `getQueryParamsWithoutAction` — must not use it; spell those out with
/// [`first`] / [`all`] so the per-key rule stays visible (there is no LAST-wins helper — see the module header).
pub(crate) fn first_map(pairs: &[(String, String)]) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::with_capacity(pairs.len());
    for (k, v) in pairs {
        out.entry(k.clone()).or_insert_with(|| v.clone());
    }
    out
}

/// v4 `getActionParam(request)` — `searchParams.get('action')`, raw: FIRST
/// wins, `None` when absent, and `Some("")` for BOTH a bare `?action=` and a
/// key-only `?action` (`lib/api/middleware/actions.ts:73-75` at `ad1c4c37f`).
///
/// **There is no folding reader any more.** Until `ad1c4c37f` v4 gated on
/// `if (action)`, so `''` took the no-action leg, and this module shipped an
/// `action()` helper that folded `Some("")` into `None` to match. v4's ONE
/// primitive (`dispatchAction`) now distinguishes them — absent runs the
/// fallback, a bare `?action=` is refused as an unknown action — so the fold
/// was deleted rather than renamed, and every edge now reads through
/// [`dispatch_action`] / [`dispatch_required_action`] (P4.D220). Read this raw
/// value directly only where the route has no action map at all.
pub(crate) fn action_param(pairs: &[(String, String)]) -> Option<&str> {
    first(pairs, "action")
}

/// v4 `dispatchAction(request, handlers, fallback)` — the gate half, for a
/// route that HAS a fallback (`ad1c4c37f`, `actions.ts:153-183`):
///
/// - **absent** → `Ok(None)`: the caller runs its default (list, create,
///   delete, download …);
/// - **known** → `Ok(Some(name))`, the matching entry of `available` — an
///   exact own-key match, so `?action=toString` is unknown exactly as v4's
///   `Object.prototype.hasOwnProperty.call` makes it;
/// - **bare `?action=` or unknown** → `Err`, v4's
///   `{"error":"Unknown action: <x>","availableActions":[…]}` 400 plus its
///   WARN. A bare action renders `"Unknown action: "` (trailing space) and
///   NEVER reaches the default — on a DELETE the default deletes, on the
///   restore POST it restores.
///
/// `available` is v4's `Object.keys(handlers)` — the thunk map's literal, in
/// insertion order — and is the route's FULL v4 list even where v5 serves only
/// some of those actions on REST (the caller answers its own loud pointer for
/// a known-but-unserved name). The `Err` is boxed: an `AxumResponse` dwarfs
/// the `Ok` (clippy `result_large_err`).
pub(crate) fn dispatch_action<'a>(
    pairs: &[(String, String)],
    available: &[&'a str],
    method: &str,
    path: &str,
) -> Result<Option<&'a str>, Box<AxumResponse>> {
    match action_param(pairs) {
        None => Ok(None),
        Some(action) => match available.iter().find(|a| **a == action) {
            Some(known) => Ok(Some(*known)),
            None => Err(Box::new(unknown_action_response(
                action, available, method, path,
            ))),
        },
    }
}

/// v4 `dispatchAction(request, handlers)` with NO fallback: the route takes
/// only actions, so an absent one is the `Action parameter required` envelope
/// (plus its WARN); bare and unknown refuse exactly as in [`dispatch_action`].
pub(crate) fn dispatch_required_action<'a>(
    pairs: &[(String, String)],
    available: &[&'a str],
    method: &str,
    path: &str,
) -> Result<&'a str, Box<AxumResponse>> {
    match dispatch_action(pairs, available, method, path)? {
        Some(action) => Ok(action),
        None => Err(Box::new(action_required_response(available, method, path))),
    }
}

/// v4 `unknownActionResponse` (`actions.ts:83-99` at `ad1c4c37f`):
/// `{"error":"Unknown action: <x>","availableActions":[…]}` at **400** — `error`
/// first, `availableActions` second, at the top level (`preserve_order`) —
/// plus `actionLogger.warn('Unknown action requested', {action,
/// availableActions, method, path})` at v4's field NAMES.
///
/// Reached for a bare `?action=` too (`action == ""`). `path` is the route's
/// PATTERN (`/api/v1/mount-points/[id]`), where v4 logs
/// `request.nextUrl.pathname` — a recorded value-only divergence on the log
/// line (the body carries no path).
pub(crate) fn unknown_action_response(
    action: &str,
    available: &[&str],
    method: &str,
    path: &str,
) -> AxumResponse {
    tracing::warn!(
        action,
        availableActions = ?available,
        method,
        path,
        "Unknown action requested"
    );
    action_envelope(&format!("Unknown action: {action}"), available)
}

/// v4 `actionRequiredResponse` (`actions.ts:104-118` at `ad1c4c37f`):
/// `{"error":"Action parameter required","availableActions":[…]}` at **400**,
/// plus `actionLogger.warn('No action param and no default handler',
/// {method, path, availableActions})`.
///
/// Reached only where the v4 route passes NO fallback — a route with one
/// answers its default here instead.
pub(crate) fn action_required_response(
    available: &[&str],
    method: &str,
    path: &str,
) -> AxumResponse {
    tracing::warn!(
        method,
        path,
        availableActions = ?available,
        "No action param and no default handler"
    );
    action_envelope("Action parameter required", available)
}

/// The one 400 body both refusals share.
fn action_envelope(error: &str, available: &[&str]) -> AxumResponse {
    (
        StatusCode::BAD_REQUEST,
        [("content-type", "application/json")],
        serde_json::json!({
            "error": error,
            "availableActions": available,
        })
        .to_string(),
    )
        .into_response()
}

/// **P4.72 (P4.67's Tier 3) — v4's two `actionLogger.warn` lines**, moved to
/// `ad1c4c37f`'s `actions.ts:88,108` by P4.D220.
///
/// `dispatchAction` writes one line beside each of its two refusals:
/// `Unknown action requested` with `{action, availableActions, method, path}`,
/// and `No action param and no default handler` with `{method, path,
/// availableActions}`. Both are emitted above; nothing but a capture layer can
/// see them, because a differential compares bodies and a log line is not one
/// (memory note `differential-blind-to-a-log-only-fix`).
///
/// **P4.D220 closed the recorded field-NAME divergence:** P4.72 spelled the
/// list field `available_actions` for the tree's snake_case convention; it is
/// now v4's `availableActions`, so a `combined.log` grep written against v4
/// matches v5 too. The remaining difference is the `path` VALUE (v5 logs the
/// route pattern, v4 the concrete pathname — see [`unknown_action_response`]).
#[cfg(test)]
mod action_warn_pins {
    use super::*;
    // The capture rig is `quilltap_core::test_support` (P4.77), reached
    // across the crate boundary via the `test-support` feature (enabled only
    // under `[dev-dependencies]`); `set_default` is THREAD-scoped, so
    // parallel tests cannot steal each other's subscriber (memory note
    // `a-process-global-test-seam-must-be-thread-scoped`).
    use quilltap_core::test_support::captured;

    fn line_with<'a>(lines: &'a [String], sentence: &str) -> &'a String {
        lines
            .iter()
            .find(|l| l.contains(sentence))
            .unwrap_or_else(|| panic!("no line carried {sentence:?}; got {lines:#?}"))
    }

    const AVAILABLE: &[&str] = &["scan", "convert"];

    #[test]
    fn unknown_action_warns_with_v4s_context() {
        let lines = captured(|| {
            let _ = unknown_action_response("zzz", AVAILABLE, "POST", "/api/v1/mount-points/[id]");
        });
        let line = line_with(&lines, "Unknown action requested");
        assert!(line.starts_with("WARN "), "v4 logs this at warn: {line}");
        for field in [
            "action=zzz",
            r#"availableActions=["scan", "convert"]"#,
            "method=POST",
            "path=/api/v1/mount-points/[id]",
        ] {
            assert!(line.contains(field), "missing {field} in {line}");
        }
    }

    #[test]
    fn action_required_warns_with_v4s_context() {
        let lines = captured(|| {
            let _ = action_required_response(AVAILABLE, "POST", "/api/v1/mount-points/[id]");
        });
        let line = line_with(&lines, "No action param and no default handler");
        assert!(line.starts_with("WARN "), "v4 logs this at warn: {line}");
        for field in [
            r#"availableActions=["scan", "convert"]"#,
            "method=POST",
            "path=/api/v1/mount-points/[id]",
        ] {
            assert!(line.contains(field), "missing {field} in {line}");
        }
        // v4's no-action line carries NO `action` field — there is no action to
        // name. Asserting the absence keeps the two lines distinguishable.
        assert!(
            !line.contains(" action="),
            "the no-action line must not invent an `action` field: {line}"
        );
    }

    /// The silence half: an absent action on a route WITH a default, and a
    /// SERVED action, write neither line. Without this a warn moved to the
    /// wrong branch would still pass both tests above.
    #[test]
    fn a_served_action_writes_neither_line() {
        let lines = captured(|| {
            let known = [("action".to_string(), "scan".to_string())];
            assert!(matches!(
                dispatch_action(&known, AVAILABLE, "POST", "/p"),
                Ok(Some("scan"))
            ));
            assert!(matches!(
                dispatch_action(&[], AVAILABLE, "POST", "/p"),
                Ok(None)
            ));
        });
        assert!(
            !lines.iter().any(|l| l.contains("Unknown action requested")
                || l.contains("No action param and no default handler")),
            "a served action must be silent; got {lines:#?}"
        );
    }

    /// The bare `?action=` takes the UNKNOWN line (v4 `actions.ts:182`), never
    /// the no-action one — the leg `ad1c4c37f` moved.
    #[test]
    fn a_bare_action_warns_as_unknown() {
        let lines = captured(|| {
            let bare = [("action".to_string(), String::new())];
            assert!(dispatch_action(&bare, AVAILABLE, "POST", "/p").is_err());
        });
        let line = line_with(&lines, "Unknown action requested");
        assert!(
            line.contains("action= "),
            "the empty action is named: {line}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("No action param and no default handler")),
            "a bare action is not an absent one: {lines:#?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(raw: &[(&str, &str)]) -> QueryPairs {
        raw.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn first_reads_the_first_occurrence() {
        let p = pairs(&[("limit", "1"), ("limit", "5")]);
        assert_eq!(first(&p, "limit"), Some("1"));
    }

    #[test]
    fn all_reads_every_occurrence_in_order() {
        let p = pairs(&[("tag", "a"), ("x", "1"), ("tag", "b")]);
        assert_eq!(all(&p, "tag"), vec!["a", "b"]);
        assert!(all(&p, "missing").is_empty());
    }

    #[test]
    fn first_preserves_the_empty_string() {
        let p = pairs(&[("includeArchived", "")]);
        assert_eq!(first(&p, "includeArchived"), Some(""));
        assert_eq!(first(&p, "absent"), None);
    }

    /// The status + parsed body of a refusal, for the vectors below.
    async fn refusal(resp: Box<AxumResponse>) -> (u16, serde_json::Value) {
        let status = resp.status().as_u16();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    const MAP: &[&str] = &["favorite", "export", "avatar"];

    /// `getActionParam` is raw `searchParams.get`: `?action=` and a key-only
    /// `?action` both read `''`, never `null` (v4 `actions.test.ts`).
    #[test]
    fn action_param_preserves_the_bare_action() {
        assert_eq!(action_param(&pairs(&[("action", "")])), Some(""));
        assert_eq!(action_param(&pairs(&[])), None);
        assert_eq!(
            action_param(&pairs(&[("action", "first"), ("action", "second")])),
            Some("first")
        );
        assert_eq!(
            action_param(&pairs(&[("action", ""), ("action", "export")])),
            Some("")
        );
    }

    /// v4 `dispatchAction`'s three-way rule, vector for vector
    /// (`__tests__/unit/lib/api/middleware/actions.test.ts` at `ad1c4c37f`).
    #[tokio::test]
    async fn dispatch_action_is_v4s_three_way_rule() {
        // absent → the fallback (`Ok(None)`); known → that action.
        assert!(matches!(
            dispatch_action(&pairs(&[]), MAP, "GET", "/p"),
            Ok(None)
        ));
        assert!(matches!(
            dispatch_action(&pairs(&[("action", "export")]), MAP, "GET", "/p"),
            Ok(Some("export"))
        ));
        // first wins: `?action=export&action=zzz` is `export`.
        assert!(matches!(
            dispatch_action(
                &pairs(&[("action", "export"), ("action", "zzz")]),
                MAP,
                "GET",
                "/p"
            ),
            Ok(Some("export"))
        ));
        // unknown → the envelope with the map's keys in insertion order.
        let err = dispatch_action(&pairs(&[("action", "unknown")]), MAP, "GET", "/p").unwrap_err();
        assert_eq!(
            refusal(err).await,
            (
                400,
                serde_json::json!({"error": "Unknown action: unknown", "availableActions": ["favorite", "export", "avatar"]})
            )
        );
        // bare → `Unknown action: ` (trailing space), NEVER the fallback.
        let err = dispatch_action(&pairs(&[("action", "")]), MAP, "GET", "/p").unwrap_err();
        let (status, body) = refusal(err).await;
        assert_eq!(status, 400);
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"error":"Unknown action: ","availableActions":["favorite","export","avatar"]}"#
        );
        // `?action=&action=export` reads the FIRST — the bare one — and refuses.
        assert!(dispatch_action(
            &pairs(&[("action", ""), ("action", "export")]),
            MAP,
            "GET",
            "/p"
        )
        .is_err());
        // own-property only: an inherited `Object.prototype` name is unknown.
        let err = dispatch_action(&pairs(&[("action", "toString")]), MAP, "GET", "/p").unwrap_err();
        assert_eq!(refusal(err).await.1["error"], "Unknown action: toString");
        // an EMPTY map (v4 `dispatchAction(req, {}, list)`): `[]`.
        let err = dispatch_action(&pairs(&[("action", "any")]), &[], "GET", "/p").unwrap_err();
        assert_eq!(
            refusal(err).await.1,
            serde_json::json!({"error": "Unknown action: any", "availableActions": []})
        );
    }

    /// No fallback: absent → `Action parameter required` with the list; the
    /// other two arms as above.
    #[tokio::test]
    async fn dispatch_required_action_refuses_the_absent_action() {
        let err = dispatch_required_action(&pairs(&[]), MAP, "POST", "/p").unwrap_err();
        assert_eq!(
            serde_json::to_string(&refusal(err).await.1).unwrap(),
            r#"{"error":"Action parameter required","availableActions":["favorite","export","avatar"]}"#
        );
        assert!(matches!(
            dispatch_required_action(&pairs(&[("action", "avatar")]), MAP, "POST", "/p"),
            Ok("avatar")
        ));
        let err =
            dispatch_required_action(&pairs(&[("action", "")]), MAP, "POST", "/p").unwrap_err();
        assert_eq!(refusal(err).await.1["error"], "Unknown action: ");
    }

    /// v4's `wantsAttachment` truth table, whole: only the exact strings `'1'`
    /// and `'true'` download. Everything else — a `0`, a word, a
    /// present-but-empty value, an absent key, the wrong CASE — is inline.
    #[test]
    fn wants_attachment_accepts_only_one_and_true() {
        for value in ["1", "true"] {
            assert!(
                wants_attachment(&pairs(&[("download", value)])),
                "download={value}"
            );
        }
        for value in ["0", "", "yes", "TRUE", "True", "on", "2", "1 "] {
            assert!(
                !wants_attachment(&pairs(&[("download", value)])),
                "download={value}"
            );
        }
        assert!(!wants_attachment(&pairs(&[])));
        assert!(!wants_attachment(&pairs(&[("downloads", "1")])));
    }

    /// `download` is a `searchParams.get` read — FIRST wins.
    #[test]
    fn wants_attachment_is_first_wins() {
        assert!(!wants_attachment(&pairs(&[
            ("download", "0"),
            ("download", "1")
        ])));
        assert!(wants_attachment(&pairs(&[
            ("download", "1"),
            ("download", "0")
        ])));
    }

    #[test]
    fn disposition_for_maps_the_predicate() {
        assert_eq!(
            disposition_for(&pairs(&[("download", "1")])),
            Disposition::Attachment
        );
        assert_eq!(disposition_for(&pairs(&[])), Disposition::Inline);
    }
}
