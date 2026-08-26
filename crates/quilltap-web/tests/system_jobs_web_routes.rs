//! P4.D123 web-edge leg, end-to-end over a live server:
//! `GET /api/v1/system/jobs`' query-parameter decoding.
//!
//! The response bodies themselves are pinned against v4 by
//! `system_jobs_collection_equivalence`, which direct-drives `jobs_list`. What
//! THIS test pins is the one thing that differential structurally cannot see:
//! the edge turning a QUERY STRING into the two booleans. The differential's
//! `…_junk` case passes `false` because the edge is *supposed* to decode
//! `?includeByType=1` that way — reasoning about the edge, not driving it, which
//! is exactly the paired-corpus blind spot
//! (`blinded-comparand-hides-the-new-arm.md`). Here the real URL goes over the
//! wire.
//!
//! Run:
//!   cargo test -p quilltap-web --test system_jobs_web_routes

mod common;

use serde_json::Value;

const JOBS_URL: &str = "/api/v1/system/jobs";

async fn get(client: &reqwest::Client, addr: &std::net::SocketAddr, query: &str) -> (u16, Value) {
    let resp = client
        .get(format!("http://{addr}{JOBS_URL}{query}"))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap())
}

fn keys(body: &Value) -> Vec<String> {
    body.as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn system_jobs_query_decoding() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // --- no query: the four always-present keys, in v4's order, and NOTHING
    //     else. `activeByType` became opt-in in 664cfca84.
    let (status, body) = get(&client, &addr, "").await;
    assert_eq!(status, 200, "bare jobs GET");
    assert_eq!(
        keys(&body),
        vec!["stats", "activeByKind", "startedByKind", "processor"],
        "the always-present keys, in v4's insertion order"
    );

    // Both kind maps carry exactly the five kinds, in order, as integers ≥ 0
    // (§Shared contract §A.2 — the server↔client join).
    for key in ["activeByKind", "startedByKind"] {
        let map = body[key].as_object().unwrap_or_else(|| panic!("{key}"));
        assert_eq!(
            map.keys().cloned().collect::<Vec<_>>(),
            vec!["memory", "embedding", "summary", "danger", "image"],
            "{key} keys"
        );
        for (k, v) in map {
            let n = v
                .as_i64()
                .unwrap_or_else(|| panic!("{key}.{k} not an integer"));
            assert!(n >= 0, "{key}.{k} = {n}");
        }
    }
    // A fresh server has started no spans, so the delta base is all zeros.
    assert!(
        body["startedByKind"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v == 0),
        "a fresh process answers zeros"
    );

    // --- ?includeByType=true opts in ---
    let (_, body) = get(&client, &addr, "?includeByType=true").await;
    assert_eq!(
        keys(&body),
        vec![
            "stats",
            "activeByKind",
            "startedByKind",
            "processor",
            "activeByType"
        ],
    );

    // --- anything but the literal 'true' is not an opt-in (v4 gates on
    //     `=== 'true'`); this is the arm the differential can only assume.
    for junk in ["?includeByType=1", "?includeByType=TRUE", "?includeByType="] {
        let (_, body) = get(&client, &addr, junk).await;
        assert!(
            !body.as_object().unwrap().contains_key("activeByType"),
            "{junk} must not opt in"
        );
    }

    // --- THE QUIRK: includeJobs implies includeByType ---
    let (_, body) = get(&client, &addr, "?includeJobs=true").await;
    assert_eq!(
        keys(&body),
        vec![
            "stats",
            "activeByKind",
            "startedByKind",
            "processor",
            "activeByType",
            "jobs"
        ],
        "includeJobs=true implies the per-type breakdown"
    );

    // --- …and only in that direction ---
    let (_, body) = get(&client, &addr, "?includeByType=true&includeJobs=1").await;
    assert!(
        !body.as_object().unwrap().contains_key("jobs"),
        "byType must not imply jobs"
    );
}
