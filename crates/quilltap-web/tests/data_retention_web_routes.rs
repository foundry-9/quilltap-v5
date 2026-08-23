//! P4.56 web-edge leg, end-to-end over a live server: the instance-wide
//! data-retention window through `GET/PUT /api/v1/settings/data-retention` (v4's
//! own URL), which `quilltap-web` had no edge for at all until this lane.
//!
//! The exact bodies of every arm are pinned against v4 by
//! `settings_routes_equivalence`, which drives the handler. What THIS test pins
//! is the plumbing that differential cannot see:
//!
//!   1. **The tri-state survives the wire.** The edge decodes the body into
//!      `Request::DataRetentionSettingsUpdate` rather than hand-building the
//!      variant, so an explicit `null` must reach the handler as a
//!      present-and-invalid value (400) while an ABSENT key keeps the stored
//!      window (200). Those two bodies differ by one key; collapsing them is
//!      exactly the defect P4.56 fixed, and a dispatch-leg-only test cannot see
//!      the edge doing it.
//!   2. **The response variant is actually unwrapped.** `unwrap_to_http`'s
//!      success arm is a hand-maintained variant list, and `BrahmaConsole` had
//!      been missing from it since P4.D57 — both brahma-console edges answered
//!      500 `Unexpected core response` on every SUCCESS. So every settings pair
//!      served from this router is exercised here on its success path.
//!
//! P4.57 widened it to all THREE pairs on this router (data-retention, Taboo,
//! Brahma Console), each over the full tri-state, because they now share ONE
//! edge implementation and ONE decoder: a regression in either would move all
//! three at once, and the arm that proves the decoder is still keyed to the
//! right wire spelling is the explicit-`null` 400 (an absent key answers 200
//! with the stored value, so a decoder pointed at the wrong key looks healthy
//! on every other body).
//!
//! Run:
//!   cargo test -p quilltap-web --test data_retention_web_routes

mod common;

use serde_json::{json, Value};

const DR_URL: &str = "/api/v1/settings/data-retention";
const BC_URL: &str = "/api/v1/settings/brahma-console";
const TABOO_URL: &str = "/api/v1/settings/taboo";

async fn get(client: &reqwest::Client, addr: &std::net::SocketAddr, path: &str) -> (u16, Value) {
    let resp = client
        .get(format!("http://{addr}{path}"))
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap())
}

async fn put_raw(
    client: &reqwest::Client,
    addr: &std::net::SocketAddr,
    path: &str,
    body: &str,
) -> (u16, Value) {
    let resp = client
        .put(format!("http://{addr}{path}"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn data_retention_web_edges() {
    let base = common::materialize_fixture_instance();

    // The committed `chat-send-*` fixture carries no `instance_settings` table:
    // it was hand-built, and v4 creates that table LAZILY (its version guard,
    // `lib/startup/version-guard.ts:201-206`), so a real instance always has one
    // but a seeded fixture may not. v5 has no version-guard port, so nothing
    // creates it at boot here — without this the read tolerates (a missing table
    // reads as "never written", i.e. the default 30) while every PUT dies on
    // `no such table`. Same repair, and the same reason, as
    // `build-context-tier3-fixture.ts` (P4.D50).
    {
        let w = quilltap_core::db::Writer::open_writable(
            &base.path().join("data/quilltap.db"),
            common::TEST_PEPPER,
        )
        .unwrap();
        w.connection()
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS \"instance_settings\" (\n\
                   \"key\" TEXT PRIMARY KEY,\n\
                   \"value\" TEXT NOT NULL\n\
                 )",
            )
            .unwrap();
    }

    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // --- GET: the documented default on an instance that never wrote it ---
    let (status, body) = get(&client, &addr, DR_URL).await;
    assert_eq!(status, 200, "data-retention GET");
    assert_eq!(body, json!({ "staleChatDays": 30 }), "the default window");

    // --- PUT a value: v4 echoes the PARSED settings, and it sticks ---
    let (status, body) = put_raw(&client, &addr, DR_URL, r#"{"staleChatDays":120}"#).await;
    assert_eq!(status, 200, "data-retention PUT");
    assert_eq!(body, json!({ "staleChatDays": 120 }), "the parsed echo");
    let (_, body) = get(&client, &addr, DR_URL).await;
    assert_eq!(body, json!({ "staleChatDays": 120 }), "the window stuck");

    // --- PUT with the key ABSENT: v4's partial body keeps the stored window ---
    let (status, body) = put_raw(&client, &addr, DR_URL, "{}").await;
    assert_eq!(status, 200, "empty-body PUT");
    assert_eq!(
        body,
        json!({ "staleChatDays": 120 }),
        "kept, not reset to 30"
    );

    // --- PUT an explicit `null`: Zod's `.default(30)` does NOT fire for a
    //     PRESENT null, so v4 answers 400 — and nothing is written. This is the
    //     arm the pre-P4.56 wire collapsed into the one above.
    let (status, body) = put_raw(&client, &addr, DR_URL, r#"{"staleChatDays":null}"#).await;
    assert_eq!(
        status, 400,
        "explicit-null PUT must be a 400, not a silent keep"
    );
    assert_eq!(body, json!({ "error": "Validation error" }));
    let (_, body) = get(&client, &addr, DR_URL).await;
    assert_eq!(
        body,
        json!({ "staleChatDays": 120 }),
        "the refusal wrote nothing"
    );

    // --- PUT out of range: the same 400, still writing nothing ---
    let (status, body) = put_raw(&client, &addr, DR_URL, r#"{"staleChatDays":5000}"#).await;
    assert_eq!(status, 400, "out-of-range PUT");
    assert_eq!(body, json!({ "error": "Validation error" }));
    let (_, body) = get(&client, &addr, DR_URL).await;
    assert_eq!(body, json!({ "staleChatDays": 120 }));

    // --- A NON-object body: v4's `{...current, ...body}` spread of a string
    //     contributes no `staleChatDays`, so the stored window survives at 200.
    let (status, body) = put_raw(&client, &addr, DR_URL, r#""ninety""#).await;
    assert_eq!(status, 200, "non-object PUT");
    assert_eq!(body, json!({ "staleChatDays": 120 }));

    // --- The sibling pair on the same router: `BrahmaConsole` was absent from
    //     `unwrap_to_http`'s success arm, so BOTH of these answered 500
    //     `Unexpected core response` before P4.56. The error path always worked
    //     (it leaves through `CoreResponse::Error`), which is why only a
    //     success-path assertion catches it.
    let (status, body) = get(&client, &addr, BC_URL).await;
    assert_eq!(status, 200, "brahma-console GET");
    assert_eq!(body, json!({ "maxAgentTurns": 50 }), "the default budget");
    let (status, body) = put_raw(&client, &addr, BC_URL, r#"{"maxAgentTurns":80}"#).await;
    assert_eq!(status, 200, "brahma-console PUT");
    assert_eq!(body, json!({ "maxAgentTurns": 80 }));

    // --- P4.57: the brahma-console tri-state over the WIRE. The dispatch-leg
    //     differential drives the decoder directly; only these three bodies
    //     prove the EDGE still routes through it.
    let (status, body) = put_raw(&client, &addr, BC_URL, "{}").await;
    assert_eq!(status, 200, "brahma-console empty-body PUT");
    assert_eq!(
        body,
        json!({ "maxAgentTurns": 80 }),
        "kept, not reset to 50"
    );
    let (status, body) = put_raw(&client, &addr, BC_URL, r#"{"maxAgentTurns":null}"#).await;
    assert_eq!(status, 400, "brahma-console explicit-null PUT");
    assert_eq!(body, json!({ "error": "Validation error" }));
    let (status, body) = put_raw(&client, &addr, BC_URL, r#"{"maxAgentTurns":"fifty"}"#).await;
    assert_eq!(status, 400, "brahma-console wrong-type PUT");
    assert_eq!(body, json!({ "error": "Validation error" }));
    let (status, body) = put_raw(&client, &addr, BC_URL, r#""eighty""#).await;
    assert_eq!(status, 200, "brahma-console non-object PUT");
    assert_eq!(body, json!({ "maxAgentTurns": 80 }));
    let (_, body) = get(&client, &addr, BC_URL).await;
    assert_eq!(
        body,
        json!({ "maxAgentTurns": 80 }),
        "the refusals wrote nothing"
    );

    // --- P4.57: the third pair on this router — the Taboo list, whose edge had
    //     never been walked live at all (its `CoreResponse::Taboo` success arm
    //     was covered only by inspection, which is precisely how the sibling
    //     `BrahmaConsole` omission survived two rounds).
    let (status, body) = get(&client, &addr, TABOO_URL).await;
    assert_eq!(status, 200, "taboo GET");
    assert_eq!(body, json!({ "phrases": [] }), "never written");

    // The echo is the NORMALIZED list (trimmed, case-insensitive duplicates
    // dropped keeping the first), not the submission. (A blank entry never
    // reaches the normalizer — the schema refuses a phrase that trims to
    // nothing; that arm is `taboo_put_whitespace_only_entry` in the family.)
    let (status, body) = put_raw(
        &client,
        &addr,
        TABOO_URL,
        r#"{"phrases":["  weight-bearing  ","WEIGHT-BEARING","tapestry"]}"#,
    )
    .await;
    assert_eq!(status, 200, "taboo PUT");
    assert_eq!(
        body,
        json!({ "phrases": ["weight-bearing", "tapestry"] }),
        "the normalized echo"
    );
    let (_, body) = get(&client, &addr, TABOO_URL).await;
    assert_eq!(body, json!({ "phrases": ["weight-bearing", "tapestry"] }));

    let (status, body) = put_raw(&client, &addr, TABOO_URL, "{}").await;
    assert_eq!(status, 200, "taboo empty-body PUT");
    assert_eq!(
        body,
        json!({ "phrases": ["weight-bearing", "tapestry"] }),
        "kept, not wiped"
    );

    let (status, body) = put_raw(&client, &addr, TABOO_URL, r#"{"phrases":null}"#).await;
    assert_eq!(status, 400, "taboo explicit-null PUT");
    assert_eq!(body, json!({ "error": "Validation error" }));

    let (status, body) = put_raw(&client, &addr, TABOO_URL, r#"{"phrases":"tapestry"}"#).await;
    assert_eq!(status, 400, "taboo non-array PUT");
    assert_eq!(body, json!({ "error": "Validation error" }));

    let (status, body) = put_raw(&client, &addr, TABOO_URL, r#""tapestry""#).await;
    assert_eq!(status, 200, "taboo non-object PUT");
    assert_eq!(body, json!({ "phrases": ["weight-bearing", "tapestry"] }));

    // An explicit empty array IS the clear gesture — the one present value that
    // must NOT be mistaken for the absent key.
    let (status, body) = put_raw(&client, &addr, TABOO_URL, r#"{"phrases":[]}"#).await;
    assert_eq!(status, 200, "taboo clear PUT");
    assert_eq!(body, json!({ "phrases": [] }));
    let (_, body) = get(&client, &addr, TABOO_URL).await;
    assert_eq!(body, json!({ "phrases": [] }), "the list was cleared");
}
