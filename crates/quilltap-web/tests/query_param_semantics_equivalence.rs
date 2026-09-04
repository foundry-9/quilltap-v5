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
//! ## What P4.72 added
//!
//! P4.67 covered fourteen of the ~31 `?action=`-reading edges. P4.72 brings in
//! the other seventeen (the restore POST, both character reads + the character
//! collection POST, the character wardrobe GET, the embedding-profiles
//! collection GET, the files item GET, BOTH chat-files arms, the
//! text-replacements POST, both conversation-summaries edges, the jobs item
//! POST, `system/unlock`, both wardrobe collection edges, and the chat item
//! GET + POST), each with the same six shapes.
//!
//! Three of them hand-roll their gate rather than using the middleware, and
//! their fixed sentences joined [`is_dispatcher_refusal`] so their rows are
//! byte-compared like the middleware's: `system/unlock`'s
//! `Missing action parameter…`, `system/jobs/[id]`'s
//! `Invalid action. Available actions: pause, resume`, and
//! `system/conversation-summaries`' `Unknown or missing action.`.
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
//! - The other four [`V5_PINNED_ENDPOINTS`] (P4.72) — `character_item_get`,
//!   `characters_collection_post`, `chat_item_get`, `chat_item_post` — are the
//!   same class one route wider: v5 hosts a strict subset of the v4 route (the
//!   rest ride `POST /api/dispatch`) and answers one loud pointer for every
//!   shape v4 sends to a handler. Their rows are pinned v5-side; the reasons
//!   sit beside them in [`RECORDED_DIVERGENCES`].
//! - The SUBSET edges — `user_profile_get`/`_put` (v5 serves no action at all;
//!   `theme-preference` is a named non-port), `system_data_dir_post`
//!   (`?action=open` is a named refusal), `mount_point_action_post` (v5 serves
//!   only the multipart `write-file`) — keep v5's own answer for the SERVED
//!   shape, which is why `known` is never cross-compared. Their refusal and
//!   equality rows are.
//!
//! Regenerate the oracle (Node 24). While v4 HEAD is past the oracle baseline
//! this needs a PINNED worktree — but the recipe below names the CHECKOUT on
//! purpose: the pin is the sweep driver's job (`recipe_sweep.py --run
//! query_param_semantics_equivalence --v4 "$PIN"` rewrites the `cd`), and a
//! committed recipe that names a `/tmp` pin is dead the round after it was
//! written (the driver refuses it as `stale_v4_pin_path`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-queryparam-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/query-param-semantics.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
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
        // ================================================================
        // P4.72 — the other seventeen `?action=`-reading edges (P4.67's
        // Tier 1 item 3 remainder). Mirrors the oracle's block entry for
        // entry; the two lengths are compared.
        // ================================================================
        ep_body(
            "system_restore_post",
            "POST",
            "/api/v1/system/restore".into(),
            "preview",
            json!({}),
        ),
        ep(
            "characters_wardrobe_get",
            "GET",
            format!("/api/v1/characters/{ITEM}/wardrobe"),
            "instructions",
        ),
        ep(
            "character_item_get",
            "GET",
            format!("/api/v1/characters/{ITEM}"),
            "stats",
        ),
        ep_body(
            "characters_collection_post",
            "POST",
            "/api/v1/characters".into(),
            "import",
            json!({}),
        ),
        ep(
            "embedding_profiles_collection_get",
            "GET",
            "/api/v1/embedding-profiles".into(),
            "list-providers",
        ),
        ep(
            "files_item_get",
            "GET",
            format!("/api/v1/files/{ITEM}"),
            "thumbnail",
        ),
        ep_body(
            "chat_files_post_link",
            "POST",
            format!("/api/v1/chats/{CHAT}/files"),
            "link",
            json!({}),
        ),
        ep_body(
            "chat_files_post_attach",
            "POST",
            format!("/api/v1/chats/{CHAT}/files"),
            "attach-mount-file",
            json!({}),
        ),
        ep_body(
            "text_replacements_post",
            "POST",
            "/api/v1/settings/text-replacements".into(),
            "bulk-replace",
            json!({}),
        ),
        ep(
            "conversation_summaries_get",
            "GET",
            "/api/v1/system/conversation-summaries".into(),
            "regenerate",
        ),
        ep_body(
            "conversation_summaries_post",
            "POST",
            "/api/v1/system/conversation-summaries".into(),
            "regenerate",
            json!({}),
        ),
        ep_body(
            "system_job_post",
            "POST",
            format!("/api/v1/system/jobs/{ITEM}"),
            "pause",
            json!({}),
        ),
        ep_body(
            "system_unlock_post",
            "POST",
            "/api/v1/system/unlock".into(),
            "lock",
            json!({}),
        ),
        ep(
            "wardrobe_collection_get",
            "GET",
            "/api/v1/wardrobe".into(),
            "instructions",
        ),
        ep_body(
            "wardrobe_collection_post",
            "POST",
            "/api/v1/wardrobe".into(),
            "instructions",
            json!({}),
        ),
        ep(
            "chat_item_get",
            "GET",
            format!("/api/v1/chats/{CHAT}"),
            "get-background",
        ),
        ep_body(
            "chat_item_post",
            "POST",
            format!("/api/v1/chats/{CHAT}"),
            "equip",
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
/// with one of the sentences below — the middleware's two envelopes, plus the
/// three hand-rolled gates P4.72 brought in (`system/unlock`'s
/// `Missing action parameter`, `system/jobs/[id]`'s `Invalid action.` and
/// `system/conversation-summaries`' `Unknown or missing action.`). All five are
/// FIXED strings decided before any repository call, so they are cross-tree
/// comparable. Anything else is a handler's own answer — the chat list, the
/// profile, the tool library, a Zod complaint — i.e. a payload over that tree's
/// database, which no cross-tree byte compare could fairly make. Those rows are
/// carried by the equality booleans instead.
fn is_dispatcher_refusal(v4_body: &Value) -> bool {
    let Some(err) = v4_body.get("error").and_then(Value::as_str) else {
        return false;
    };
    err.starts_with("Unknown action:")
        || err.starts_with("Action parameter required")
        || err.starts_with("Missing action parameter")
        || err.starts_with("Invalid action. Available actions:")
        || err == "Unknown or missing action."
}

/// The five endpoints whose v5 edge serves a strict SUBSET of the v4 route and
/// answers its own loud "this rides /api/dispatch" sentence for everything
/// else. There is no cross-tree comparison to be made for their non-`known`
/// shapes — v4 runs a handler v5 does not host — so every one of those rows is
/// pinned v5-side in [`RECORDED_DIVERGENCES`] instead of compared.
const V5_PINNED_ENDPOINTS: &[&str] = &[
    "character_item_post",
    "character_item_get",
    "characters_collection_post",
    "chat_item_get",
    "chat_item_post",
];

/// Shapes never cross-compared regardless: `known` runs the endpoint's real
/// work (a payload over each tree's own database — and on the SUBSET edges,
/// where v5 does not serve the action v4 dispatches, a RECORDED divergence
/// pinned v5-side by [`UNSERVED_KNOWN_ACTIONS`]), and the
/// [`V5_PINNED_ENDPOINTS`] gate in a different order or host a different action
/// set on the two sides — see the header's "Recorded divergences".
fn cross_comparable_shape(key: &str, shape: &str) -> bool {
    shape != "known" && shape != "known_then_unknown" && !V5_PINNED_ENDPOINTS.contains(&key)
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
    // --- P4.72 ---
    // `GET /api/v1/characters/{id}` — v4 has NO refusal leg here at all:
    // `handlers/get.ts:44` falls through `!action || !isValidAction(...)` to the
    // full character payload, so v4 answers 200 for bare, `?action=`, AND an
    // unknown action. v5 serves only the byte-out `?action=export` leg (the
    // JSON reads ride `POST /api/dispatch`, the P4.D66 narrowing), so all four
    // shapes fold onto its loud pointer. Recorded, not chased: closing it means
    // hosting v4's whole character GET at this URL.
    (
        "character_item_get__bare",
        400,
        "This route serves ?action=export only",
    ),
    (
        "character_item_get__empty",
        400,
        "This route serves ?action=export only",
    ),
    (
        "character_item_get__unknown",
        400,
        "This route serves ?action=export only",
    ),
    (
        "character_item_get__empty_then_known",
        400,
        "This route serves ?action=export only",
    ),
    // `POST /api/v1/characters` — same shape: v4's `handlers/post.ts:592` falls
    // through to `handleCreate` for bare / `?action=` / unknown (a Zod 400 over
    // the empty body); v5 serves only the multipart `import` and
    // `reset-builtins` legs, creation being a dispatch verb.
    (
        "characters_collection_post__bare",
        400,
        "This route serves ?action=import and ?action=reset-builtins only",
    ),
    (
        "characters_collection_post__empty",
        400,
        "This route serves ?action=import and ?action=reset-builtins only",
    ),
    (
        "characters_collection_post__unknown",
        400,
        "This route serves ?action=import and ?action=reset-builtins only",
    ),
    (
        "characters_collection_post__empty_then_known",
        400,
        "This route serves ?action=import and ?action=reset-builtins only",
    ),
    // `GET /api/v1/chats/{id}` — v4's if-chain falls through to the whole chat
    // payload; v5 hosts only the four byte/JSON legs the dispatch channel
    // cannot carry (`outfit`, `outfit-summary`, `export`, `export-markdown`)
    // plus `get-background` / `cost`.
    (
        "chat_item_get__bare",
        400,
        "Only the get-background and cost actions are served on this route",
    ),
    (
        "chat_item_get__empty",
        400,
        "Only the get-background and cost actions are served on this route",
    ),
    (
        "chat_item_get__unknown",
        400,
        "Only the get-background and cost actions are served on this route",
    ),
    (
        "chat_item_get__empty_then_known",
        400,
        "Only the get-background and cost actions are served on this route",
    ),
    // `POST /api/v1/chats/{id}` — v4 DOES refuse here, but hand-rolled and after
    // its chat-404: `Unknown action: ${action}. Available actions: <all 30>`
    // (`chats/[id]/handlers/post.ts:120`), with the action INTERPOLATED — so v4
    // says `Unknown action: null` for a bare request and `Unknown action: ` for
    // `?action=`, which is why its `fold` is FALSE (see [`RECORDED_EQUALITIES`]).
    // v5 serves two of the thirty here and answers one sentence for all four
    // shapes, without a chat lookup.
    (
        "chat_item_post__bare",
        400,
        "Only the equip and regenerate-avatar actions are served on this route",
    ),
    (
        "chat_item_post__empty",
        400,
        "Only the equip and regenerate-avatar actions are served on this route",
    ),
    (
        "chat_item_post__unknown",
        400,
        "Only the equip and regenerate-avatar actions are served on this route",
    ),
    (
        "chat_item_post__empty_then_known",
        400,
        "Only the equip and regenerate-avatar actions are served on this route",
    ),
];

/// v4-KNOWN actions this edge does NOT serve (they ride `/api/dispatch`): v4
/// dispatches them (the oracle's `known` rows are v4's handlers running), v5
/// answers a loud refusal that names the fact — NEVER v4's `Unknown action:`
/// envelope, which would list the action as available in the sentence that
/// refuses it. `(method, path, action, status, exact error)`. The §3
/// unification review of the follow-ups round put these back after the lane
/// had replaced them with the envelope.
const UNSERVED_KNOWN_ACTIONS: &[(&str, &str, &str, u16, &str)] = &[
    (
        "POST",
        "/api/v1/mount-points/00000000-0000-4000-8000-000000000001",
        "scan",
        400,
        "Only the multipart 'write-file' action is served on this route; JSON mount actions ride POST /api/dispatch",
    ),
    (
        "GET",
        "/api/v1/system/tools",
        "capabilities-report-progress",
        400,
        "The 'capabilities-report-progress' action is not served on this route; it rides POST /api/dispatch",
    ),
    (
        "POST",
        "/api/v1/system/tools",
        "ai-import-stream",
        400,
        "The 'ai-import-stream' action is not served on this route; it rides POST /api/dispatch",
    ),
    // --- P4.72: `system/unlock`'s four v4-known siblings ---
    // v4's `isUnlockAction` (`system/unlock/route.ts:102`) knows five; v5
    // aliases only `change-passphrase` at this URL because the other four have
    // dispatch verbs the SPA uses. Until this lane they answered v4's UNKNOWN
    // sentence — `Unknown action: lock` for an action v4 dispatches — which is
    // the same lie the §3 unification review of P4.67 removed from the
    // mount-point and system/tools edges. They now carry the mount-point
    // precedent's loud pointer instead, pinned here.
    (
        "POST",
        "/api/v1/system/unlock",
        "setup",
        400,
        "Only the 'change-passphrase' action is served on this route; the other database-key actions ride POST /api/dispatch",
    ),
    (
        "POST",
        "/api/v1/system/unlock",
        "unlock",
        400,
        "Only the 'change-passphrase' action is served on this route; the other database-key actions ride POST /api/dispatch",
    ),
    (
        "POST",
        "/api/v1/system/unlock",
        "store",
        400,
        "Only the 'change-passphrase' action is served on this route; the other database-key actions ride POST /api/dispatch",
    ),
    (
        "POST",
        "/api/v1/system/unlock",
        "lock",
        400,
        "Only the 'change-passphrase' action is served on this route; the other database-key actions ride POST /api/dispatch",
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
        // The oracle records a stub gap as a NEGATIVE status (-1 no export, -2
        // the handler threw). Such a row is not a measurement — every equality
        // computed over it is vacuously true — so it fails here by name
        // rather than counting toward `handler_rows` (the §3 unification
        // review).
        if let Some(st) = v.get("status").and_then(Value::as_i64) {
            assert!(
                st > 0,
                "oracle row {} recorded status {st} — a stub gap, not v4's answer: {}",
                v["name"],
                v["body"]
            );
        }
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
        const RECORDED_EQUALITIES: &[(&str, &str, bool)] = &[
            ("character_item_post", "fold", true),
            // Same cause, measured on a second route (P4.72): v4's chat POST
            // interpolates the action into its refusal, so bare reads
            // `Unknown action: null` and `?action=` reads `Unknown action: `
            // — v4 does NOT fold. v5's subset pointer names neither, so it
            // does. Pinned rather than chased: matching v4 would mean hosting
            // all thirty chat POST actions at this URL.
            ("chat_item_post", "fold", true),
        ];
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
        refusal_rows >= 40 && handler_rows >= 40,
        "the refusal/handler classification collapsed ({refusal_rows} refusal, {handler_rows} handler) \
         — a change to v4's refusal wording would silently stop this family comparing bytes"
    );
    assert!(
        failed.is_empty(),
        "{} row(s) failed: {failed:?}",
        failed.len()
    );
}

/// The v5-side pins no oracle row can carry: the subset edges' unserved
/// actions (`UNSERVED_KNOWN_ACTIONS`), the custom-tools roster's empty action
/// map (v4 `withActionDispatch({}, handleList)` — a truthy action refuses with
/// an EMPTY `availableActions`; the endpoint is not in the oracle's list), and
/// (the duplicate-key class for a NON-action key — `?limit=1&limit=2` — is
/// pinned in `chats_collection_route.rs`, whose venue seeds chats). Runs
/// without an oracle so `cargo test --workspace` always sees it.
#[tokio::test]
async fn unserved_known_actions_are_pinned_v5_side() {
    let base = materialize_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    for (method, path, action, want_status, want_error) in UNSERVED_KNOWN_ACTIONS {
        let url = format!("http://{addr}{path}?action={action}");
        let req = match *method {
            "GET" => client.get(&url),
            "POST" => client.post(&url).body("{}"),
            m => panic!("unhandled method {m}"),
        }
        .header("content-type", "application/json");
        let resp = req.send().await.unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap();
        assert_eq!(
            status, *want_status,
            "{method} {path}?action={action}: {body}"
        );
        assert_eq!(
            body["error"].as_str(),
            Some(*want_error),
            "{method} {path}?action={action}"
        );
        assert!(
            body.get("availableActions").is_none(),
            "an unserved-but-known action must not be refused with v4's envelope: {body}"
        );
    }

    // custom-tools GET: v4's EMPTY map — `Unknown action: <x>` with `[]`.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/chats/00000000-0000-4000-8000-000000000002/custom-tools?action=foo"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Unknown action: foo");
    assert_eq!(body["availableActions"], json!([]));

    // The duplicate-key class on a NON-action key is pinned where the venue
    // can discriminate it: `chats_collection_route.rs` (`?limit=1&limit=2`
    // answers `?limit=1`'s count) — this family's `system-data-*` venue lists
    // no chats for the fixture user. P4.72's other per-site rows live with
    // their venues too; see [`duplicate_non_action_keys_read_the_first`].
}

/// **P4.72 — the per-site DUPLICATED-key rows for the non-action keys**
/// (P4.67's Tier 2 remainder). v4 reads most query keys with
/// `searchParams.get`, the FIRST occurrence; v5's old `Query<HashMap>`
/// extractor kept the LAST. Two of the classified sites can be discriminated
/// on this family's `system-data-*` venue without any seeded row, because both
/// have a value that REFUSES:
///
/// - `kind` on `GET /api/v1/system/image-aesthetics`
///   (`llm_logs_routes.rs:110`) — `lantern`/`aurora` serve, anything else is
///   v4's fixed `Query param "kind" must be "lantern" or "aurora"` 400.
/// - `filePath` on `GET /api/v1/chats/{id}/qtap-target`
///   (`qtap_target_route.rs:60`) — a present-but-EMPTY value is JS-falsy in
///   v4's `querySchema` (`filePath` has `min(1)`), so it refuses; a non-empty
///   one goes on to the chat lookup.
///
/// Each row asserts BOTH halves: that the repeat answers what the FIRST value
/// answers, and that the second value answers something ELSE — without the
/// second assertion the row would pass whichever value won (memory note
/// `a-green-mutation-means-a-non-discriminating-arm`).
///
/// **The rest of the classified sites, and where they are pinned:**
///
/// | site | reader | pinned in |
/// |---|---|---|
/// | `limit` on `GET /api/v1/chats` | FIRST | `chats_collection_route.rs` (P4.67) |
/// | `tag` on `GET /api/v1/photos` | ALL (`getAll`) | `photos_web_routes.rs` |
/// | `q` / `limit` / `offset` on `GET /api/v1/photos` | FIRST | `photos_web_routes.rs` (P4.72) |
/// | `force` on `DELETE /api/v1/files/{id}` | FIRST | `files_write_routes.rs` (P4.72) |
///
/// **Deferred loudly:** `scope` and `mountPoint` on the qtap-target route.
/// Both are FIRST-wins reads through `crate::query::first` (the source is the
/// pin), but neither has a value that answers differently until a chat AND a
/// resolvable mount exist — the route's chat-404 precedes any use of them, and
/// no committed fixture carries the pair. A row over either would be vacuous,
/// so none is added; the class is recorded here and in the lane record.
#[tokio::test]
async fn duplicate_non_action_keys_read_the_first() {
    let base = materialize_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    async fn answer(client: &reqwest::Client, url: String) -> (u16, Value) {
        let resp = client.get(url).send().await.unwrap();
        let status = resp.status().as_u16();
        let raw = resp.text().await.unwrap();
        let body = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
        (status, body)
    }

    // --- `kind` on the image-aesthetics GET ---
    let base_url = format!("http://{addr}/api/v1/system/image-aesthetics");
    let good = answer(&client, format!("{base_url}?kind=lantern")).await;
    let bad = answer(&client, format!("{base_url}?kind=zzz")).await;
    assert_ne!(
        good, bad,
        "the `kind` pair must discriminate: {good:?} vs {bad:?}"
    );
    assert_eq!(
        answer(&client, format!("{base_url}?kind=lantern&kind=zzz")).await,
        good,
        "`?kind=lantern&kind=zzz` must read the FIRST occurrence (v4 `searchParams.get`)"
    );

    // --- `filePath` on the qtap-target GET ---
    let base_url = format!("http://{addr}/api/v1/chats/{CHAT}/qtap-target");
    let present = answer(&client, format!("{base_url}?filePath=notes.md")).await;
    let empty = answer(&client, format!("{base_url}?filePath=")).await;
    assert_eq!(
        empty.1["error"], "Invalid query: filePath is required",
        "an empty `filePath` is v4's `min(1)` refusal: {empty:?}"
    );
    // Both answers are 400 here — the chat is absent from this venue, so the
    // present-value leg gets past the query gate and dies on the chat lookup.
    // The BODIES are what discriminate, so the whole answer is the comparand.
    assert_ne!(
        present, empty,
        "the `filePath` pair must discriminate: {present:?} vs {empty:?}"
    );
    assert_eq!(
        answer(&client, format!("{base_url}?filePath=notes.md&filePath=")).await,
        present,
        "`?filePath=notes.md&filePath=` must read the FIRST occurrence"
    );
}
