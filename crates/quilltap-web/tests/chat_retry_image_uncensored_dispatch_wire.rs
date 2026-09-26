//! P4.D228 (v4 `ce2f1dabf`, #77) — "Try uncensored" on a picture / the
//! Lantern's backdrop (`chatRetryImageUncensored { chatId, body }`) AT THE
//! DISPATCH WIRE.
//!
//! `retry_uncensored_tier3` proves the verb against v4's real route. What only
//! the wire can see:
//!
//! 1. **The union decode after serde.** `body` rides as ONE raw `Value` so the
//!    core decides v4's `z.union` itself — first branch wins, unknown keys
//!    stripped. `{toolMessageId: 'x', kind: 'background'}` must be a PICTURE
//!    retry (a missing tool row → 404, which a background retry can never
//!    answer); `{toolMessageId: '', kind: 'background'}` a BACKGROUND retry
//!    (→ the gate's 409); `{toolMessageId: 5}` and an absent body v4's 400
//!    sentence, byte for byte.
//! 2. **The 409 kinds** as the SPA reads them: `kind: 'conflict'`, the BARE
//!    `locked` / `no-understudy` token as the message (§S.3).
//! 3. **The REST edge answers the pointer**, not the action (the siblings'
//!    shape), and `retry-image-uncensored` is a KNOWN action there — an
//!    unknown one answers v4's `Unknown action` envelope instead.
//!
//! Run:
//!   cargo test -p quilltap-web --test chat_retry_image_uncensored_dispatch_wire

mod common;

use std::path::Path;

use quilltap_core::db::Writer;
use serde_json::{json, Value};

const POPULATED_CHAT_ID: &str = "c860cf74-128f-4a81-9a5c-6c2275f24302";
const MISSING_CHAT: &str = "99999999-9999-4999-8999-999999999999";
const RNG_TOOL_ID: &str = "f7ee0001-0000-4000-8000-00000000f7ee";
const BODY_REFUSAL: &str = "Expected { toolMessageId } or { kind: \"background\" }";

fn main_db(base: &Path) -> std::path::PathBuf {
    base.join("data").join("quilltap.db")
}

/// Plant: no uncensored-compatible image profile and no configured desk (so
/// the gate answers `no-understudy`), story backgrounds ON, one `rng` TOOL row;
/// optionally the chat Locked.
fn plant(base: &Path, locked: bool) {
    let w = Writer::open_writable(&main_db(base), common::TEST_PEPPER).unwrap();
    let c = w.connection();
    quilltap_core::test_support::ensure_p4d225_columns(c);
    let _ = c.execute("UPDATE image_profiles SET isDangerousCompatible = 0", []);
    c.execute(
        "UPDATE chat_settings SET conciergeSettings = '{\"enabled\":true}'",
        [],
    )
    .unwrap();
    c.execute_batch(&format!(
        "CREATE TEMP TABLE m AS SELECT * FROM chat_messages WHERE chatId = '{POPULATED_CHAT_ID}' LIMIT 1;
         UPDATE m SET id = '{RNG_TOOL_ID}', role = 'TOOL', systemSender = NULL,
           content = '{{\"toolName\":\"rng\",\"success\":true,\"result\":\"4\",\"arguments\":{{\"sides\":6}}}}';
         INSERT INTO chat_messages SELECT * FROM m;
         DROP TABLE m;"
    ))
    .unwrap();
    if locked {
        c.execute(
            "UPDATE chats SET conciergeMode = 'locked', conciergeModeSetBy = 'operator', conciergeModeReason = 'manual' WHERE id = ?1",
            [POPULATED_CHAT_ID],
        )
        .unwrap();
    }
}

struct Wire {
    client: reqwest::Client,
    base: String,
}
impl Wire {
    async fn dispatch(&self, body: Value) -> (u16, Value) {
        let r = self
            .client
            .post(format!("{}/api/dispatch", self.base))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = r.status().as_u16();
        (status, r.json::<Value>().await.unwrap_or(Value::Null))
    }
    async fn retry(&self, chat_id: &str, body: Option<Value>) -> (u16, Value) {
        let mut req = json!({ "type": "chatRetryImageUncensored", "chatId": chat_id });
        if let Some(b) = body {
            req["body"] = b;
        }
        self.dispatch(req).await
    }
}

async fn serve(locked: bool) -> (tempfile::TempDir, Wire) {
    let base = common::materialize_fixture_instance();
    plant(base.path(), locked);
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    (
        base,
        Wire {
            client: reqwest::Client::new(),
            base: format!("http://{addr}"),
        },
    )
}

fn err(v: &Value) -> (&str, &str) {
    (
        v["data"]["kind"].as_str().unwrap_or_default(),
        v["data"]["message"].as_str().unwrap_or_default(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_union_decodes_first_branch_wins_after_serde() {
    let (_base, wire) = serve(false).await;

    // `{toolMessageId: 5}` — neither branch parses.
    let (s, v) = wire
        .retry(POPULATED_CHAT_ID, Some(json!({ "toolMessageId": 5 })))
        .await;
    assert_eq!((s, err(&v)), (400, ("bad-request", BODY_REFUSAL)), "{v}");
    // The whole envelope's bytes (key order included).
    assert_eq!(
        v.to_string(),
        json!({ "type": "error", "data": { "kind": "bad-request", "message": BODY_REFUSAL } })
            .to_string()
    );

    // No body at all — v4's malformed-JSON `{}` — the same 400.
    let (s, v) = wire.retry(POPULATED_CHAT_ID, None).await;
    assert_eq!((s, err(&v)), (400, ("bad-request", BODY_REFUSAL)), "{v}");

    // `{toolMessageId: 'x', kind: 'background'}` — the PICTURE branch wins
    // (the unknown `kind` is stripped): a missing tool row is a 404, which a
    // background retry never answers.
    let (s, v) = wire
        .retry(
            POPULATED_CHAT_ID,
            Some(json!({ "toolMessageId": "f7ee0009-0000-4000-8000-00000000f7ee", "kind": "background" })),
        )
        .await;
    assert_eq!(
        (s, err(&v)),
        (404, ("not-found", "Tool message not found")),
        "first branch wins: {v}"
    );

    // `{toolMessageId: '', kind: 'background'}` — `.min(1)` fails the first
    // branch, so it is a BACKGROUND retry → the gate (nobody to send it to).
    let (s, v) = wire
        .retry(
            POPULATED_CHAT_ID,
            Some(json!({ "toolMessageId": "", "kind": "background" })),
        )
        .await;
    assert_eq!((s, err(&v)), (409, ("conflict", "no-understudy")), "{v}");

    // A TOOL row that is not `generate_image`.
    let (s, v) = wire
        .retry(
            POPULATED_CHAT_ID,
            Some(json!({ "toolMessageId": RNG_TOOL_ID })),
        )
        .await;
    assert_eq!(
        (s, err(&v)),
        (
            400,
            (
                "bad-request",
                "Only generate_image pictures can be retried uncensored"
            )
        ),
        "{v}"
    );

    // The chat first (v4's `handlePost` 404s before the body is parsed).
    let (s, v) = wire
        .retry(MISSING_CHAT, Some(json!({ "toolMessageId": 5 })))
        .await;
    assert_eq!((s, err(&v)), (404, ("not-found", "Chat not found")), "{v}");

    // A wrong-type chatId is refused at the decode.
    let (s, v) = wire
        .dispatch(json!({ "type": "chatRetryImageUncensored", "chatId": 7, "body": {} }))
        .await;
    assert_eq!(s, 400, "a typed `chat_id` refuses a number: {v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_locked_chat_is_refused_for_the_backdrop() {
    let (_base, wire) = serve(true).await;
    let (s, v) = wire
        .retry(POPULATED_CHAT_ID, Some(json!({ "kind": "background" })))
        .await;
    assert_eq!((s, err(&v)), (409, ("conflict", "locked")), "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_rest_edge_answers_the_dispatch_pointer() {
    let (_base, wire) = serve(false).await;
    let r = wire
        .client
        .post(format!(
            "{}/api/v1/chats/{POPULATED_CHAT_ID}?action=retry-image-uncensored",
            wire.base
        ))
        .json(&json!({ "kind": "background" }))
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    let body: Value = r.json().await.unwrap_or(Value::Null);
    assert_eq!(status, 400, "{body}");
    let text = body.to_string();
    assert!(
        text.contains("/api/dispatch"),
        "a KNOWN action the REST edge does not serve answers the pointer: {body}"
    );
    assert!(
        !text.contains("Unknown action"),
        "`retry-image-uncensored` must be in the 48-key list: {body}"
    );
}
