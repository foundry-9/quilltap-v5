//! P4.9P UI-SEARCH route differential: `api::ui_search::ui_search` vs v4's REAL
//! `GET /api/v1/ui/search` handler. Both sides read the SAME /tmp-built fixture
//! (`build-ui-search-fixture.ts` — the order's choice over a committed pair: no
//! existing committed family carries all six search types richly enough).
//!
//! P4.D122 (v4 `b220999d`) added the sixth type, `documents`, and with it five
//! cases and a documents corpus in the fixture. The three corpus-shape
//! assertions below moved with it BY DESIGN — the case count, the all-types
//! type list (five → six), and the brass total (66 → 67, the one document whose
//! chunk text carries "brass fittings").
//!
//! Success cases diff status + body, PLUS the raw key ORDER of every object
//! (countsByType's insertion order — first-encounter order in the SORTED
//! results — and each result object's literal field order both ride on it).
//! Error cases diff status + `{error}` message.
//!
//! A corpus-shape gate runs before the diff (the P4.11/D24 lesson — a
//! truncated fixture must not pass silently): the oracle must carry EXACTLY the
//! 28 case names, the all-types case must have found all six types, the brass
//! cap case must total 67, and the documents cases must be non-vacuous (the
//! `documents_only` fan-out must carry both name-hit `matchedField` spellings
//! and both `storeType`s, and the zero-match case must omit the type entirely).
//!
//! ⚠ One arm is deliberately NOT here: the store-name **ambiguity** fallback to
//! the UUID. v4's `repairMountPointNameCollisions` runs inside the lazy
//! doc-mount-points table init that the search's own `findEnabled()` triggers,
//! and it keys on the IDENTICAL `name.trim().toLowerCase()` as
//! `collectAmbiguousStoreNames` — so v4 renames any collision before the
//! resolver can see it, and `nameIsAmbiguous` is unreachable for store names at
//! `b220999d`. (Recorded as a v4-side filing candidate.) The arm is pinned at
//! unit tier in `doc_edit::uri_producers::tests::ref_for_mount_arms`; the
//! RESERVED-name arm (`self`), which no repair touches, IS exercised here.
//!
//! Build fixture + generate the oracle (Node 24 — see the .ts headers), then:
//!   QT_ORACLE_UI_SEARCH=/tmp/oracle-ui-search.ndjson \
//!   QT_FIXTURE_UI_SEARCH_MAIN=/tmp/qt-ui-search-fixture/main.db \
//!   QT_FIXTURE_UI_SEARCH_MOUNT=/tmp/qt-ui-search-fixture/mount.db \
//!     cargo test -p quilltap-harness --test ui_search_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::api::ui_search::{ui_search, UiSearchParams};
use quilltap_core::db::runtime::{Db, DbPaths};
use serde::Deserialize;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    user_id: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/ui-search.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// One Db over a private copy of the /tmp fixture — the whole surface is
/// read-only, so a single copy serves every case.
fn fresh_db(main_src: &str, mount_src: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-ui-search-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(main_src, &main).unwrap();
    std::fs::copy(mount_src, &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

// ── JSON canonicalization ──────────────────────────────────────────────────
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// Every object's key sequence, depth-first — the wire-order claim `norm`'s
/// sorted compare deliberately throws away.
fn key_paths(v: &Value, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            out.push(format!(
                "{path}: {}",
                o.keys().cloned().collect::<Vec<_>>().join(",")
            ));
            for (k, x) in o.iter() {
                key_paths(x, &format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                key_paths(x, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

/// The 28 oracle cases, name → the same raw query params the oracle's URLs
/// decode to. (`%20` decodes to a space before `searchParams.get` hands it to
/// the handler, so the Rust side passes the decoded strings.)
fn case_params(name: &str) -> UiSearchParams<'static> {
    let p = |q: Option<&'static str>,
             types: Option<&'static str>,
             limit: Option<&'static str>,
             offset: Option<&'static str>| UiSearchParams {
        q,
        types,
        limit,
        offset,
    };
    match name {
        "all_types_default" => p(Some("airship"), None, None, None),
        "characters_only" => p(Some("airship"), Some("characters"), None, None),
        "chats_and_messages" => p(Some("airship"), Some("chats,messages"), None, None),
        "messages_only" => p(Some("airship"), Some("messages"), None, None),
        "memories_only" => p(Some("airship"), Some("memories"), None, None),
        "tags_only" => p(Some("airship"), Some("tags"), None, None),
        "types_unknown_filtered" => p(Some("airship"), Some("bogus,characters"), None, None),
        "types_all_unknown" => p(Some("airship"), Some("bogus,junk"), None, None),
        "missing_q" => p(None, None, None, None),
        "whitespace_q" => p(Some("  "), None, None, None),
        "short_q" => p(Some("a"), None, None, None),
        "limit_cap_50" => p(Some("brass"), None, Some("999"), None),
        "offset_past_cap" => p(Some("brass"), None, Some("999"), Some("50")),
        "offset_mid_page" => p(Some("brass"), None, Some("10"), Some("5")),
        "garbage_limit" => p(Some("airship"), None, Some("abc"), None),
        "garbage_offset" => p(Some("airship"), None, None, Some("abc")),
        "negative_limit" => p(Some("brass"), None, Some("-5"), None),
        "negative_offset" => p(Some("airship"), None, None, Some("-3")),
        "empty_limit_param" => p(Some("airship"), None, Some(""), None),
        "exact_name_priority" => p(Some("ottoline"), None, None, None),
        "uppercase_q" => p(Some("AIRSHIP"), None, None, None),
        "trimmed_q_echo" => p(Some("  airship  "), None, None, None),
        "overlong_q" => {
            let q: &'static str = Box::leak("x".repeat(1001).into_boxed_str());
            p(Some(q), None, None, None)
        }
        // ── The documents chip (P4.D122) ──
        "documents_only" => p(Some("manifesto"), Some("documents"), None, None),
        "documents_content_snippet" => p(Some("airship"), Some("documents"), None, None),
        "documents_zero_matches" => p(Some("goggles"), Some("documents,tags"), None, None),
        "documents_cross_type_tie" => p(Some("tie-token"), None, None, None),
        // The oracle's URL is `?q=50%25`; `searchParams.get` decodes it before
        // the handler sees it, so the Rust side passes the decoded `50%`.
        "documents_like_wildcard" => p(Some("50%"), Some("documents"), None, None),
        other => panic!("unknown oracle case {other:?} — regenerate both sides together"),
    }
}

const EXPECTED_CASES: [&str; 28] = [
    "all_types_default",
    "characters_only",
    "chats_and_messages",
    "messages_only",
    "memories_only",
    "tags_only",
    "types_unknown_filtered",
    "types_all_unknown",
    "missing_q",
    "whitespace_q",
    "short_q",
    "limit_cap_50",
    "offset_past_cap",
    "offset_mid_page",
    "garbage_limit",
    "garbage_offset",
    "negative_limit",
    "negative_offset",
    "empty_limit_param",
    "exact_name_priority",
    "uppercase_q",
    "trimmed_q_echo",
    "overlong_q",
    "documents_only",
    "documents_content_snippet",
    "documents_zero_matches",
    "documents_cross_type_tie",
    "documents_like_wildcard",
];

#[test]
fn ui_search_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_UI_SEARCH") else {
        return;
    };
    let Some(main_src) = env_or_skip("QT_FIXTURE_UI_SEARCH_MAIN") else {
        return;
    };
    let Some(mount_src) = env_or_skip("QT_FIXTURE_UI_SEARCH_MOUNT") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let raw = std::fs::read_to_string(&oracle_path).unwrap();
    let mut oracle: HashMap<String, Value> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).unwrap();
        let name = v["name"].as_str().unwrap().to_string();
        order.push(name.clone());
        oracle.insert(name, v);
    }

    // ── The corpus-shape gate (a truncated fixture must not pass silently) ──
    assert_eq!(
        order,
        EXPECTED_CASES.to_vec(),
        "oracle case list drifted — regenerate the oracle and update both sides together"
    );
    let all_types = &oracle["all_types_default"];
    let types_found: Vec<&str> = all_types["body"]["types"]
        .as_array()
        .expect("all_types_default.types")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        types_found,
        // The fan-out order, which `b220999d` did NOT change: the documents
        // branch sits between memories and tags.
        [
            "characters",
            "chats",
            "messages",
            "memories",
            "documents",
            "tags"
        ],
        "the all-types case must find every type (fixture richness gate)"
    );
    assert_eq!(
        oracle["limit_cap_50"]["body"]["totalCount"].as_i64(),
        Some(67),
        "the brass corpus must total 67 (61 messages — the 60 generated rows \
         plus CH1's long message, whose 'brass fittings' also match — + \
         1 character + 1 chat + 1 memory + 2 tags + 1 DOCUMENT, Bertram's \
         vault `description.md`, whose projected chunk text is that same \
         'brass fittings' sentence)"
    );
    assert_eq!(
        oracle["limit_cap_50"]["body"]["results"]
            .as_array()
            .map(Vec::len),
        Some(50),
        "limit=999 must cap at 50"
    );

    // ── The documents corpus must not be vacuous (P4.D122) ──────────────────
    // A green regen is not coverage: assert the SHAPES each documents case
    // exists to discriminate, so a fixture that silently lost its doc stores
    // reddens here rather than agreeing with an equally empty oracle.
    let docs = oracle["documents_only"]["body"]["results"]
        .as_array()
        .expect("documents_only.results");
    let field_of = |k: &str| docs.iter().any(|r| r["matchedField"].as_str() == Some(k));
    assert!(
        field_of("fileName") && field_of("relativePath"),
        "documents_only must carry BOTH name-hit spellings (the twice-bound \
         LIKE pattern is what makes a path-only hit reachable)"
    );
    assert!(
        docs.iter().any(|r| r["storeType"] == "character")
            && docs.iter().any(|r| r["storeType"] == "documents"),
        "documents_only must span an ordinary store AND a character vault"
    );
    assert!(
        docs.iter().any(|r| r["matchPriority"] == 0)
            && docs.iter().any(|r| r["matchPriority"] == 1),
        "documents_only must carry the whole-name priority-0 arm AND a \
         substring priority-1 arm"
    );
    assert!(
        oracle["documents_content_snippet"]["body"]["results"]
            .as_array()
            .expect("documents_content_snippet.results")
            .iter()
            .any(|r| r["matchedField"] == "content"),
        "documents_content_snippet must carry at least one CONTENT hit"
    );
    let zero_types: Vec<&str> = oracle["documents_zero_matches"]["body"]["types"]
        .as_array()
        .expect("documents_zero_matches.types")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        zero_types,
        ["tags"],
        "documents_zero_matches must find a TAG and no document (typesFound \
         .add lives inside the loop)"
    );
    assert_eq!(
        oracle["documents_like_wildcard"]["body"]["results"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the LIKE-escape arm must match the literal `50%` file and NOT its \
         `50-plans.md` decoy"
    );

    let db = fresh_db(&main_src, &mount_src);
    let mut failed: Vec<String> = Vec::new();

    for name in EXPECTED_CASES {
        let want = &oracle[name];
        let want_status = want["status"].as_u64().unwrap() as u16;
        let resp = ui_search(&db, &spec.user_id, &case_params(name));

        if (200..300).contains(&want_status) {
            let Response::UiSearch(body) = &resp else {
                eprintln!("[{name}] expected success, got {resp:?}");
                failed.push(name.to_string());
                continue;
            };
            if norm(body) != norm(&want["body"]) {
                eprintln!(
                    "[{name}] BODY MISMATCH:\n got {}\n want {}",
                    norm(body),
                    norm(&want["body"])
                );
                failed.push(name.to_string());
                continue;
            }
            // The wire-order claim: the Rust body's key sequence must match
            // v4's JSON.stringify bytes, not merely carry the same key set.
            let mut want_paths = Vec::new();
            key_paths(&want["body"], "", &mut want_paths);
            let mut got_paths = Vec::new();
            key_paths(body, "", &mut got_paths);
            if got_paths != want_paths {
                eprintln!(
                    "[{name}] KEY ORDER MISMATCH:\n got {got_paths:#?}\n want {want_paths:#?}"
                );
                failed.push(format!("{name}:keyOrder"));
            } else {
                eprintln!("[{name}] OK (body + key order).");
            }
        } else {
            match &resp {
                Response::Error(e) => {
                    let want_msg = want["body"]["error"].as_str().unwrap_or("");
                    if status_of(e.kind) == want_status && e.message == want_msg {
                        eprintln!("[{name}] OK ({want_status}).");
                    } else {
                        eprintln!(
                            "[{name}] MISMATCH: got {} {:?} / want {want_status} {want_msg:?}",
                            status_of(e.kind),
                            e.message
                        );
                        failed.push(name.to_string());
                    }
                }
                other => {
                    eprintln!("[{name}] expected error, got {other:?}");
                    failed.push(name.to_string());
                }
            }
        }
    }

    assert!(failed.is_empty(), "cases failed: {failed:?}");
}
