//! P4.67 — **the `?action=` query-shape family**: how every v5 REST edge that
//! reads an action answers the shapes no other family sends.
//!
//! Found by the P4.D143 §3 review. v5 reads its query through
//! `Query<HashMap<String, String>>`, which yields `Some("")` for a
//! present-but-empty `?action=` and keeps the **last** value of a repeated key.
//! v4 reads `searchParams.get('action')` — the **first** value — and then gates
//! on JS truthiness (`if (action)` in `lib/api/middleware/actions.ts:88`), so
//! `''` is *no action at all*. Every existing family sends one well-formed
//! action, so neither difference was visible anywhere.
//!
//! ## Two comparands, on purpose
//!
//! **Refusal rows** — shapes where v4 answers a 4xx from its dispatcher — are
//! compared across the trees byte-for-byte: status + whole body, key order
//! included (`preserve_order`).
//!
//! **Equality rows** — `fold`, `firstWins`, `emptyFirstWins` — are computed
//! *within each tree* and compared as BOOLEANS. They have to be: v4's
//! no-action leg is a payload over its own database (the chat list, the
//! profile, the tool library), so comparing bodies across trees would be
//! comparing fixtures rather than dispatch. What the lane claims is exactly
//! what these booleans say:
//!
//! - `fold` — `?action=` answers exactly what a bare request answers.
//! - `firstWins` — `?action=<a>&action=<b>` answers what `?action=<a>` answers.
//! - `emptyFirstWins` — `?action=&action=<a>` answers what `?action=` answers.
//!
//! ⚠ `fold` is **not** always true in v4, and asserting it blindly would be a
//! port bug of its own: the routes that hand-roll `isValidAction` with **no**
//! `!action` carve-out (`system/tools` GET+POST, `user/profile` PATCH) render
//! the action into their sentence, so an absent action reads `Unknown action:
//! null` and `?action=` reads `Unknown action: `. v4 distinguishes them and so
//! must v5. The oracle measures which, per endpoint; nothing here assumes.
//!
//! ## Recorded divergences
//!
//! - `character_item_post` — v4's `handlePost` runs `repos.characters.findById`
//!   → `notFound('Character')` BEFORE the action gate; v5's edge refuses
//!   without a lookup. The oracle mocks the character into existence so v4's
//!   *sentence* is on the record, but the two trees gate in different orders,
//!   so the rows are pinned on the v5 side rather than cross-compared. v5 also
//!   serves only `archive`/`rehydrate` here (the P4.D66 CLI edge); the other
//!   eleven v4 actions ride `/api/dispatch`.
//! - The SUBSET edges — `user_profile_get`/`_put` (v5 serves no action at all;
//!   `theme-preference` is a named non-port), `system_data_dir_post`
//!   (`?action=open` is a named refusal), `mount_point_action_post` (v5 serves
//!   only the multipart `write-file`) — keep v5's own answer for the SERVED
//!   shape, which is why `known` is never cross-compared. Their refusal and
//!   equality rows are.
//!
//! Regenerate the oracle (Node 24, from a worktree PINNED at the baseline —
//! the v4 checkout is past it; see the .test.ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   PIN=/tmp/qt-v4-pin-p4.67-6d2a50382
//!   TMPO=/tmp/qt-queryparam-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/query-param-semantics.test.ts" "$TMPO/cases/"
//!   cd "$PIN"
//!   QT_ORACLE_OUT=/tmp/oracle-query-param-semantics.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- query-param-semantics
//! Run:
//!   QT_ORACLE_QUERY_PARAM_SEMANTICS=/tmp/oracle-query-param-semantics.ndjson \
//!     cargo test -p quilltap-web --test query_param_semantics_equivalence -- --nocapture

mod common;

use std::collections::HashMap;

use serde_json::{json, Value};

const CHAT: &str = "bb000000-0000-4000-8000-0000000000bb";
const ITEM: &str = "cc000000-0000-4000-8000-0000000000cc";

/// One URL+method a v5 REST edge serves. Mirrors the oracle's `ENDPOINTS`
/// entry for entry; the two lengths are compared so a row added on one side
/// only cannot pass silently.
struct Endpoint {
    key: &'static str,
    method: &'static str,
    path: String,
    /// An action v4 serves — the `<a>` of the first-wins probe.
    known: &'static str,
    body: Option<Value>,
}

fn ep(key: &'static str, method: &'static str, path: String, known: &'static str) -> Endpoint {
    Endpoint {
        key,
        method,
        path,
        known,
        body: None,
    }
}

fn ep_body(
    key: &'static str,
    method: &'static str,
    path: String,
    known: &'static str,
    body: Value,
) -> Endpoint {
    Endpoint {
        key,
        method,
        path,
        known,
        body: Some(body),
    }
}

fn endpoints() -> Vec<Endpoint> {
    vec![
        ep(
            "system_tools_get",
            "GET",
            "/api/v1/system/tools".into(),
            "tasks-queue",
        ),
        ep_body(
            "system_tools_post",
            "POST",
            "/api/v1/system/tools".into(),
            "tasks-queue",
            json!({}),
        ),
        ep_body(
            "system_data_dir_post",
            "POST",
            "/api/v1/system/data-dir".into(),
            "open",
            json!({}),
        ),
        ep(
            "user_profile_get",
            "GET",
            "/api/v1/user/profile".into(),
            "theme-preference",
        ),
        ep_body(
            "user_profile_put",
            "PUT",
            "/api/v1/user/profile".into(),
            "theme-preference",
            json!({"themePreference": "system"}),
        ),
        ep_body(
            "user_profile_patch",
            "PATCH",
            "/api/v1/user/profile".into(),
            "set-avatar",
            json!({}),
        ),
        ep_body(
            "brahma_item_patch",
            "PATCH",
            format!("/api/v1/brahma-console/{CHAT}"),
            "set-model",
            json!({}),
        ),
        ep(
            "chats_collection_get",
            "GET",
            "/api/v1/chats".into(),
            "has-dangerous",
        ),
        ep_body(
            "embedding_profile_item_post",
            "POST",
            format!("/api/v1/embedding-profiles/{ITEM}"),
            "refit",
            json!({}),
        ),
        ep_body(
            "mount_point_action_post",
            "POST",
            format!("/api/v1/mount-points/{ITEM}"),
            "scan",
            json!({}),
        ),
        ep(
            "custom_tools_collection_get",
            "GET",
            "/api/v1/custom-tools".into(),
            "destinations",
        ),
        ep_body(
            "custom_tools_collection_post",
            "POST",
            "/api/v1/custom-tools".into(),
            "preview",
            json!({}),
        ),
        ep_body(
            "chat_custom_tools_post",
            "POST",
            format!("/api/v1/chats/{CHAT}/custom-tools"),
            "run",
            json!({}),
        ),
        ep_body(
            "character_item_post",
            "POST",
            format!("/api/v1/characters/{ITEM}"),
            "favorite",
            json!({}),
        ),
    ]
}

const UNKNOWN_ACTION: &str = "zzz-not-an-action";

/// The six probes, named as the oracle names them.
const SHAPES: &[&str] = &[
    "bare",
    "empty",
    "unknown",
    "known",
    "known_then_unknown",
    "empty_then_known",
];

fn query_for(shape: &str, known: &str) -> String {
    match shape {
        "bare" => String::new(),
        "empty" => "?action=".into(),
        "unknown" => format!("?action={UNKNOWN_ACTION}"),
        "known" => format!("?action={known}"),
        "known_then_unknown" => format!("?action={known}&action={UNKNOWN_ACTION}"),
        "empty_then_known" => format!("?action=&action={known}"),
        other => panic!("unknown shape {other}"),
    }
}

/// Is this v4 answer a **dispatcher refusal** — produced before any handler
/// ran — and therefore comparable across the trees byte-for-byte?
///
/// Derived from the recorded row rather than declared per endpoint, so it
/// cannot drift out of step with v4: every refusal v4's dispatchers emit opens
/// with `Unknown action:`, `Action parameter required` or (the `system/unlock`
/// hand-roll) `Missing action parameter`. Anything else is a handler's own
/// answer — the chat list, the profile, the tool library, a Zod complaint —
/// i.e. a payload over that tree's database, which no cross-tree byte compare
/// could fairly make. Those rows are carried by the equality booleans instead.
fn is_dispatcher_refusal(v4_body: &Value) -> bool {
    let Some(err) = v4_body.get("error").and_then(Value::as_str) else {
        return false;
    };
    err.starts_with("Unknown action:")
        || err.starts_with("Action parameter required")
        || err.starts_with("Missing action parameter")
}

/// Shapes never cross-compared regardless: `known` runs the endpoint's real
/// work (and §A of the round's shared contract pins it unchanged anyway), and
/// `character_item_post` gates ownership in a different order on the two sides
/// — see the header's "Recorded divergences".
fn cross_comparable_shape(key: &str, shape: &str) -> bool {
    shape != "known" && shape != "known_then_unknown" && key != "character_item_post"
}

/// v5's pinned answers for the rows that cannot be cross-compared, so they
/// cannot drift unnoticed. `(key__shape, status, error-prefix)`.
const RECORDED_DIVERGENCES: &[(&str, u16, &str)] = &[
    (
        "character_item_post__bare",
        400,
        "This route serves ?action=archive and ?action=rehydrate only",
    ),
    (
        "character_item_post__empty",
        400,
        "This route serves ?action=archive and ?action=rehydrate only",
    ),
    (
        "character_item_post__unknown",
        400,
        "This route serves ?action=archive and ?action=rehydrate only",
    ),
    (
        "character_item_post__empty_then_known",
        400,
        "This route serves ?action=archive and ?action=rehydrate only",
    ),
];

/// The venue: the committed `system-data-*` family — the richest of the
/// committed instances, and the one the system-data routes are diffed over
/// everywhere else. Every endpoint here either refuses in its dispatcher or
/// looks up an id that resolves to nothing, so no arm depends on a seeded row.
fn materialize_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for (fixture, name) in [
        ("system-data-main.db", "quilltap.db"),
        ("system-data-mount.db", "quilltap-mount-index.db"),
        ("system-data-llmlogs.db", "quilltap-llm-logs.db"),
    ] {
        std::fs::copy(common::fixtures_dir().join(fixture), data.join(name))
            .unwrap_or_else(|e| panic!("copy {fixture}: {e}"));
    }
    base
}

#[derive(Clone, PartialEq)]
struct Answer {
    status: u16,
    body: Value,
}

#[tokio::test(flavor = "multi_thread")]
async fn query_param_semantics_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_QUERY_PARAM_SEMANTICS") else {
        eprintln!("SKIP: set QT_ORACLE_QUERY_PARAM_SEMANTICS (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let all = endpoints();
    let oracle_endpoints = oracle
        .keys()
        .filter(|k| k.ends_with("__equalities"))
        .count();
    assert_eq!(
        oracle_endpoints,
        all.len(),
        "the oracle covers {oracle_endpoints} endpoints, the v5 list {} — a row was added on one side only",
        all.len()
    );

    let base = materialize_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let mut failed: Vec<String> = Vec::new();
    // Guards against the classifier silently swallowing the whole family: both
    // buckets must be non-empty, and the refusal bucket is what proves v4's
    // envelope bytes are actually being compared.
    let mut refusal_rows = 0usize;
    let mut handler_rows = 0usize;

    for e in &all {
        let mut answers: HashMap<&str, Answer> = HashMap::new();
        for shape in SHAPES {
            let url = format!("http://{addr}{}{}", e.path, query_for(shape, e.known));
            let req = match e.method {
                "GET" => client.get(&url),
                "POST" => client.post(&url),
                "PUT" => client.put(&url),
                "PATCH" => client.patch(&url),
                m => panic!("unhandled method {m}"),
            }
            .header("content-type", "application/json");
            let req = match &e.body {
                Some(v) => req.body(serde_json::to_string(v).unwrap()),
                None => req,
            };
            let resp = req.send().await.unwrap();
            let status = resp.status().as_u16();
            let raw = resp.text().await.unwrap();
            let body: Value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
            let name = format!("{}__{}", e.key, shape);

            if let Some((_, want_status, want_prefix)) =
                RECORDED_DIVERGENCES.iter().find(|(n, ..)| *n == name)
            {
                let got_error = body.get("error").and_then(Value::as_str).unwrap_or("");
                if status != *want_status || !got_error.starts_with(want_prefix) {
                    eprintln!("[{name}] RECORDED-DIVERGENCE DRIFTED: {status} {body}");
                    failed.push(format!("{name}_recorded"));
                } else {
                    eprintln!("[{name}] recorded divergence intact ({status}).");
                }
            } else if cross_comparable_shape(e.key, shape) {
                let want = oracle
                    .get(&name)
                    .unwrap_or_else(|| panic!("oracle missing row '{name}'"));
                let want_status = want["status"].as_u64().unwrap() as u16;
                let want_body = &want["body"];
                if !is_dispatcher_refusal(want_body) {
                    eprintln!("[{name}] handler leg — equality-only (v4 {want_status}).");
                    handler_rows += 1;
                } else if status != want_status {
                    eprintln!("[{name}] STATUS {status} != {want_status} (v5 body {body})");
                    failed.push(format!("{name}_status"));
                } else if &body != want_body {
                    eprintln!("[{name}] BODY\n  v5: {body}\n  v4: {want_body}");
                    failed.push(format!("{name}_body"));
                } else {
                    eprintln!("[{name}] ok ({status}).");
                    refusal_rows += 1;
                }
            }
            answers.insert(shape, Answer { status, body });
        }

        // The within-tree equalities. For the endpoint whose gate ORDER
        // differs (see the header), v5's own fold is pinned instead: v4
        // distinguishes absent from `?action=` because it renders the action
        // into its sentence, while v5's subset refusal names neither — so v5
        // folds where v4 does not, and that is the recorded divergence, not a
        // drift to chase.
        const RECORDED_EQUALITIES: &[(&str, &str, bool)] = &[("character_item_post", "fold", true)];
        let want = oracle
            .get(&format!("{}__equalities", e.key))
            .unwrap_or_else(|| panic!("oracle missing equalities for '{}'", e.key));
        for (label, a, b) in [
            ("fold", "empty", "bare"),
            ("firstWins", "known_then_unknown", "known"),
            ("emptyFirstWins", "empty_then_known", "empty"),
        ] {
            let got = answers[a] == answers[b];
            let expected = match RECORDED_EQUALITIES
                .iter()
                .find(|(k, l, _)| *k == e.key && *l == label)
            {
                Some((_, _, pinned)) => *pinned,
                None => want[label].as_bool().unwrap(),
            };
            if got != expected {
                eprintln!(
                    "[{}] {label} = {got}, v4 says {expected}\n  {a}: {} {}\n  {b}: {} {}",
                    e.key, answers[a].status, answers[a].body, answers[b].status, answers[b].body
                );
                failed.push(format!("{}_{label}", e.key));
            } else {
                eprintln!("[{}] {label} = {got} (matches v4).", e.key);
            }
        }
    }

    eprintln!(
        "cross-compared refusal rows: {refusal_rows}; equality-only handler rows: {handler_rows}"
    );
    assert!(
        refusal_rows >= 20 && handler_rows >= 8,
        "the refusal/handler classification collapsed ({refusal_rows} refusal, {handler_rows} handler) \
         — a change to v4's refusal wording would silently stop this family comparing bytes"
    );
    assert!(
        failed.is_empty(),
        "{} row(s) failed: {failed:?}",
        failed.len()
    );
}
