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
//! Nothing but selection lives here. In particular [`first`] PRESERVES the
//! empty string for `?k=`, because `URLSearchParams.get` does — the JS
//! truthiness that turns `''` into "absent" belongs at the call site, exactly
//! where v4 spells it. [`action`] is the one place that truthiness is shared,
//! because v4 shares it too (one `if (action)` inside `withActionDispatch`).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};

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

/// v4 `withActionDispatch`'s gate, whole: `getActionParam(request)` — which is
/// `searchParams.get('action')`, so FIRST-wins — followed by `if (action)`.
///
/// That `if` is JS truthiness, so a **present-but-empty** `?action=` is falsy
/// and takes the SAME no-action leg as an absent parameter. Returning `None`
/// for both is what makes a v5 edge answer `?action=` the way v4 does; reading
/// the raw [`first`] value at a call site would resurrect the bug.
pub(crate) fn action(pairs: &[(String, String)]) -> Option<&str> {
    match first(pairs, "action") {
        None | Some("") => None,
        Some(a) => Some(a),
    }
}

/// v4 `withActionDispatch`'s unknown-action refusal, byte-shaped:
/// `{"error":"Unknown action: <x>","availableActions":[…]}` at **400**, plus
/// v4's `actionLogger.warn('Unknown action requested', …)`.
///
/// `available` is v4's `Object.keys(actions)` — the route file's handler-map
/// literal, in **insertion order**. Only a TRUTHY unknown action may reach
/// here; `?action=` belongs on the no-action leg (see [`action`]).
pub(crate) fn unknown_action_response(
    action: &str,
    available: &[&str],
    method: &str,
    path: &str,
) -> AxumResponse {
    // `available_actions`, not v4's `availableActions` — the tree's snake_case
    // log-field convention, a recorded divergence (see the test module's doc).
    tracing::warn!(
        action,
        available_actions = ?available,
        method,
        path,
        "Unknown action requested"
    );
    (
        StatusCode::BAD_REQUEST,
        [("content-type", "application/json")],
        serde_json::json!({
            "error": format!("Unknown action: {action}"),
            "availableActions": available,
        })
        .to_string(),
    )
        .into_response()
}

/// v4 `withActionDispatch`'s no-action-and-no-default refusal:
/// `{"error":"Action parameter required","availableActions":[…]}` at **400**,
/// plus `actionLogger.warn('No action param and no default handler', …)`.
///
/// Reached only where the v4 route passes NO `defaultHandler` — a route with a
/// default answers the default here instead.
pub(crate) fn action_required_response(
    available: &[&str],
    method: &str,
    path: &str,
) -> AxumResponse {
    // Same recorded field-spelling divergence as the unknown-action warn.
    tracing::warn!(
        available_actions = ?available,
        method,
        path,
        "No action param and no default handler"
    );
    (
        StatusCode::BAD_REQUEST,
        [("content-type", "application/json")],
        serde_json::json!({
            "error": "Action parameter required",
            "availableActions": available,
        })
        .to_string(),
    )
        .into_response()
}

/// **P4.72 (P4.67's Tier 3) — v4's two `actionLogger.warn` lines.**
///
/// `withActionDispatch` writes one line beside each of its two refusals
/// (`lib/api/middleware/actions.ts:103,124`): `Unknown action requested`
/// with `{action, availableActions, method, path}`, and `No action param and
/// no default handler` with `{method, path, availableActions}`. Both are
/// emitted above; nothing but a capture layer can see them, because a
/// differential compares bodies and a log line is not one (memory note
/// `differential-blind-to-a-log-only-fix`).
///
/// **One recorded difference, deliberate:** the field is spelled
/// `available_actions`, not v4's `availableActions`. Every ported warn in this
/// tree spells its fields in snake_case (`chat_id`, `profile_id`, … — see
/// `image_gen/lora_support.rs`), and `combined.log` is read as a whole; making
/// this one line camelCase would buy v4 parity on one line at the cost of
/// consistency across every other. The SENTENCES are byte-exact, which is what
/// an operator greps for.
#[cfg(test)]
mod action_warn_pins {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct CaptureLayer(Arc<Mutex<Vec<String>>>);

    struct FieldVisitor(String);
    impl tracing::field::Visit for FieldVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0.push_str(&format!(" {value:?}"));
            } else {
                self.0.push_str(&format!(" {}={value:?}", field.name()));
            }
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = event.metadata();
            let mut visitor = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.0);
        }
    }

    /// `set_default` is THREAD-scoped, so parallel tests cannot steal each
    /// other's subscriber (memory note
    /// `a-process-global-test-seam-must-be-thread-scoped`).
    fn captured(f: impl FnOnce()) -> Vec<String> {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        {
            let _guard = tracing::subscriber::set_default(subscriber);
            f();
        }
        let out = logs.lock().unwrap().clone();
        out
    }

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
            r#"available_actions=["scan", "convert"]"#,
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
            r#"available_actions=["scan", "convert"]"#,
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

    /// The silence half: a SERVED action writes neither line. Without this a
    /// warn moved to the wrong branch would still pass both tests above.
    #[test]
    fn a_served_action_writes_neither_line() {
        let lines = captured(|| {
            assert_eq!(action(&[("action".into(), "scan".into())]), Some("scan"));
        });
        assert!(
            !lines.iter().any(|l| l.contains("Unknown action requested")
                || l.contains("No action param and no default handler")),
            "a served action must be silent; got {lines:#?}"
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

    /// The whole point of [`action`]: `?action=` is JS-falsy, so it takes the
    /// no-action leg — indistinguishable from an absent parameter.
    #[test]
    fn action_folds_empty_into_absent() {
        assert_eq!(action(&pairs(&[("action", "")])), None);
        assert_eq!(action(&pairs(&[])), None);
        assert_eq!(action(&pairs(&[("action", "export")])), Some("export"));
    }

    /// `getActionParam` is `searchParams.get`, so a repeated `action` resolves
    /// to the FIRST value — including when the first one is the empty string,
    /// which still takes the no-action leg.
    #[test]
    fn action_is_first_wins() {
        assert_eq!(
            action(&pairs(&[("action", "export"), ("action", "bogus")])),
            Some("export")
        );
        assert_eq!(
            action(&pairs(&[("action", ""), ("action", "export")])),
            None
        );
    }
}
