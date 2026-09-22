//! P4.D185 AVATAR-ROLLS REST edges: `GET /api/v1/characters/{id}/avatar-rolls`
//! and `POST|DELETE …/avatar-rolls/{fileId}`, driven with REAL HTTP against the
//! REAL router over the committed `avatar-rolls-{main,mount}.db` pair, and
//! diffed case-for-case against v4's OWN route handlers.
//!
//! The service differential (`quilltap-harness/tests/
//! avatar_rolls_tier2_equivalence.rs`) cannot see any of what this pins: the
//! Zod query gate's exact sentences (they are Zod's own, measured, never
//! transcribed), `withActionDispatch`'s two refusal envelopes with their
//! `availableActions` array, the error ladder's status mapping, and v4's
//! measured GUARD ORDER — a save for a character that does not exist answers
//! `Avatar roll not found`, not `Character not found`, because `requireRoll`
//! runs first and a tag cannot match a character with no rows.
//!
//! Every case gets its own instance directory and its own server, because the
//! POST and DELETE cases write.
//!
//! Run (the oracle is the shared one — see the harness family's header):
//!   QT_ORACLE_AVATAR_ROLLS=/tmp/oracle-avatar-rolls.ndjson \
//!     cargo test -p quilltap-web --test avatar_rolls_routes

mod common;

use quilltap_core::db::Writer;
use serde_json::Value;

const ROLF: &str = "a1000000-0000-4000-8000-000000000001";
/// The fixture's keyed roll with a BACKEND storage key and no mount link
/// (`build-avatar-rolls-fixture.ts`, `F_ROLL_NOLINK`).
const F_ROLL_NOLINK: &str = "f1000000-0000-4000-8000-000000000004";
const TALL: &str = "a1000000-0000-4000-8000-000000000003";
const NOBODY: &str = "a9000000-0000-4000-8000-0000000000ff";

const F_ROLL_NEW: &str = "f1000000-0000-4000-8000-000000000001";
const F_ROLL_ALBUM: &str = "f1000000-0000-4000-8000-000000000003";
const F_NOT_ROLL: &str = "f1000000-0000-4000-8000-000000000007";
const F_ROLL_TALL: &str = "f1000000-0000-4000-8000-000000000008";

/// Materialize an instance dir from the committed avatar-rolls pair.
fn materialize() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        common::fixtures_dir().join("avatar-rolls-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        common::fixtures_dir().join("avatar-rolls-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        // The fixture's own user id is not `SINGLE_USER_ID`; nothing this
        // surface reads filters on it (the established shape of every sibling
        // `files` read), but the engine's other reads do.
        common::rewrite_fixture_user_ids(w.connection());
    }
    base
}

/// Every id the oracle's bodies can name that the fixture bakes; anything else
/// was minted by the save under test.
fn baked_ids() -> std::collections::HashSet<String> {
    let meta: Value = serde_json::from_str(
        &std::fs::read_to_string(common::fixtures_dir().join("avatar-rolls-main.db.meta.json"))
            .expect("read meta sidecar"),
    )
    .unwrap();
    let mut out: std::collections::HashSet<String> = std::collections::HashSet::new();
    fn collect(v: &Value, out: &mut std::collections::HashSet<String>) {
        match v {
            Value::String(x) if is_uuid(x) => {
                out.insert(x.clone());
            }
            Value::Array(a) => a.iter().for_each(|x| collect(x, out)),
            Value::Object(o) => o.iter().for_each(|(_, x)| collect(x, out)),
            _ => {}
        }
    }
    collect(&meta, &mut out);
    out
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.as_bytes()[8] == b'-'
        && s.as_bytes()[13] == b'-'
        && s.as_bytes()[18] == b'-'
        && s.as_bytes()[23] == b'-'
        && s.bytes().all(|b| b == b'-' || b.is_ascii_hexdigit())
}

/// Replace a minted uuid with `<minted-N>` in first-sight order (the harness
/// family's normalizer, same rule).
fn normalize_minted(
    v: &mut Value,
    baked: &std::collections::HashSet<String>,
    seen: &mut Vec<String>,
) {
    match v {
        Value::String(s) => {
            if is_uuid(s) && !baked.contains(s.as_str()) {
                let idx = match seen.iter().position(|x| x == s) {
                    Some(i) => i,
                    None => {
                        seen.push(s.clone());
                        seen.len() - 1
                    }
                };
                *s = format!("<minted-{idx}>");
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| normalize_minted(x, baked, seen)),
        Value::Object(o) => o
            .iter_mut()
            .for_each(|(_, x)| normalize_minted(x, baked, seen)),
        _ => {}
    }
}

#[tokio::test]
async fn avatar_roll_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_AVATAR_ROLLS") else {
        eprintln!("SKIP: set QT_ORACLE_AVATAR_ROLLS (see test header).");
        return;
    };
    let mut oracle: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    let route_cases = oracle.keys().filter(|k| k.starts_with("route_")).count();
    assert!(
        route_cases >= 24,
        "the oracle is stale: {route_cases} route cases, expected at least 24"
    );

    let baked = baked_ids();
    let client = reqwest::Client::new();
    let mut failed: Vec<String> = Vec::new();

    // (name, method, path) — the path is everything after `/api/v1/characters`.
    let cases: Vec<(&str, &str, String)> = vec![
        ("route_list_ok", "GET", format!("/{ROLF}/avatar-rolls")),
        (
            "route_list_paginated",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit=2&offset=1"),
        ),
        (
            "route_list_limit_zero",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit=0"),
        ),
        (
            "route_list_limit_empty",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit="),
        ),
        (
            "route_list_limit_nan",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit=abc"),
        ),
        (
            "route_list_limit_float",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit=1.5"),
        ),
        (
            "route_list_limit_too_big",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit=201"),
        ),
        (
            "route_list_offset_negative",
            "GET",
            format!("/{ROLF}/avatar-rolls?offset=-1"),
        ),
        (
            "route_list_limit_infinity",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit=Infinity"),
        ),
        (
            "route_list_offset_huge",
            "GET",
            format!("/{ROLF}/avatar-rolls?offset=1e30"),
        ),
        (
            "route_list_limit_negative_huge",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit=-1e30"),
        ),
        (
            "route_list_both_bad",
            "GET",
            format!("/{ROLF}/avatar-rolls?limit=0&offset=-1"),
        ),
        (
            "route_list_missing_character",
            "GET",
            format!("/{NOBODY}/avatar-rolls"),
        ),
        (
            "route_post_no_action",
            "POST",
            format!("/{ROLF}/avatar-rolls/{F_ROLL_NEW}"),
        ),
        (
            "route_post_unknown_action",
            "POST",
            format!("/{ROLF}/avatar-rolls/{F_ROLL_NEW}?action=bogus"),
        ),
        (
            "route_post_empty_action",
            "POST",
            format!("/{ROLF}/avatar-rolls/{F_ROLL_NEW}?action="),
        ),
        (
            "route_post_save_to_album",
            "POST",
            format!("/{ROLF}/avatar-rolls/{F_ROLL_NEW}?action=save-to-album"),
        ),
        (
            "route_post_set_avatar",
            "POST",
            format!("/{ROLF}/avatar-rolls/{F_ROLL_ALBUM}?action=set-avatar"),
        ),
        (
            "route_post_not_a_roll",
            "POST",
            format!("/{ROLF}/avatar-rolls/{F_NOT_ROLL}?action=save-to-album"),
        ),
        (
            "route_post_missing_character",
            "POST",
            format!("/{NOBODY}/avatar-rolls/{F_ROLL_NEW}?action=save-to-album"),
        ),
        (
            "route_post_no_vault",
            "POST",
            format!("/{TALL}/avatar-rolls/{F_ROLL_TALL}?action=save-to-album"),
        ),
        (
            "route_delete_ok",
            "DELETE",
            format!("/{ROLF}/avatar-rolls/{F_ROLL_NEW}"),
        ),
        (
            "route_delete_miss",
            "DELETE",
            format!("/{ROLF}/avatar-rolls/{F_NOT_ROLL}"),
        ),
        (
            "route_delete_missing_character",
            "DELETE",
            format!("/{NOBODY}/avatar-rolls/{F_ROLL_NEW}"),
        ),
    ];

    for (name, method, path) in cases {
        // A fresh instance per case: the POST/DELETE cases write.
        let base = materialize();
        // The PRODUCTION spine, as every deployment shell boots it. Without it
        // `save_image_bytes` is None, `ready_save_image` falls back to
        // `NotConfiguredBytes`, and the one case that actually copies bytes
        // answers `has empty bytes` — so this is both the seam the save needs
        // and a WIRING probe for it (the venue's documented default has no
        // spine at all).
        let base_dir = base.path().to_path_buf();
        let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
            c.terminal = false;
            c.spine = Some(std::sync::Arc::new(
                quilltap_host::ProductionSpineFactory::new(
                    base_dir,
                    c.version.clone(),
                    c.tz.clone(),
                ),
            ));
            c
        })
        .await;
        let url = format!("http://{addr}/api/v1/characters{path}");
        let resp = match method {
            "GET" => client.get(&url).send().await,
            "POST" => client.post(&url).send().await,
            _ => client.delete(&url).send().await,
        }
        .expect("request");
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap_or(Value::Null);

        let want_row = oracle
            .get(name)
            .unwrap_or_else(|| panic!("the oracle has no case {name}"));
        let want_status = want_row["result"]["status"].as_u64().unwrap_or(0) as u16;
        let mut want_body = want_row["result"]["body"].clone();
        let mut got_body = body;
        normalize_minted(&mut got_body, &baked, &mut Vec::new());
        normalize_minted(&mut want_body, &baked, &mut Vec::new());

        if status != want_status || got_body != want_body {
            eprintln!(
                "[{name}] MISMATCH:\n  GOT : {status} {}\n  WANT: {want_status} {}",
                serde_json::to_string(&got_body).unwrap(),
                serde_json::to_string(&want_body).unwrap()
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK ({status}).");
        }
        drop(base);
    }

    assert!(failed.is_empty(), "avatar-roll routes FAILED: {failed:?}");
}

/// A roll whose bytes cannot be READ is a 500, not a 400 (the `31436bae4`
/// round's §3 review). v4's `readImageBuffer` throws on a failed read and the
/// item route's ladder (`[fileId]/route.ts:44-57`) maps none of its four
/// `includes` arms onto that message, so v4 answers `serverError(message)`
/// beside `[Characters/AvatarRolls v1] Avatar roll action failed`; only a read
/// that SUCCEEDS with nothing in it is the `has empty bytes` 400. Pinned here
/// on the STATUS only: v4's body carries its storage manager's own sentence for
/// the backend key (`<userId>/no-link.webp` names a backend this venue does
/// not configure), and v5's carries its backend's — two engines' error prose,
/// a recorded text divergence over one status class. `F_ROLL_NOLINK` is the
/// fixture's keyed roll with a backend storage key and no mount link.
#[tokio::test]
async fn a_roll_whose_bytes_cannot_be_read_answers_500_not_400() {
    let base = materialize();
    let base_dir = base.path().to_path_buf();
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(std::sync::Arc::new(
            quilltap_host::ProductionSpineFactory::new(base_dir, c.version.clone(), c.tz.clone()),
        ));
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = format!(
        "http://{addr}/api/v1/characters/{ROLF}/avatar-rolls/{F_ROLL_NOLINK}?action=save-to-album"
    );
    let resp = client.post(&url).send().await.expect("request");
    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    assert_eq!(
        status, 500,
        "a failed byte read is v4's 500, not the empty-bytes 400: {body}"
    );
    assert!(
        body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "the 500 carries the read failure's sentence: {body}"
    );
    assert!(
        !body["error"]
            .as_str()
            .unwrap_or("")
            .contains("has empty bytes"),
        "a read that FAILED must not be reported as one that answered nothing: {body}"
    );
    drop(base);
}
