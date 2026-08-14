//! P4.D73 web-edge leg, end-to-end over a live server: the three 4.8.2
//! `chat_settings` composer/typography settings (`composerEmoji`,
//! `composerUnicode`, `smartTypographySettings`) through `POST /api/dispatch`.
//!
//! The exact bodies of every arm are pinned against v4 by
//! `settings_routes_equivalence`, which drives the handler directly. What THIS
//! test pins is the plumbing the differential cannot see:
//!
//!   1. **The boot ensure.** The instance's `chat_settings` table is put back
//!      into its pre-4.8.2 shape (the columns dropped) before the server boots,
//!      so the ALTERs actually run. MEASURED with the ensure disabled: the read
//!      tolerates (the screen renders with the Zod defaults) but the PUT answers
//!      `500 sqlite error: no such column: composerEmoji` — `update_for_user`'s
//!      update branch is a plain `SET`. The re-read after the PUT is what proves
//!      the repair.
//!   2. **The raw-bag wire.** `Request::ChatSettingsUpdate` carries the settings
//!      object verbatim, so an explicit `null` must reach the handler as a
//!      present-and-invalid value rather than collapsing into an absent key
//!      (the Taboo §3 lesson — that defect was invisible to a dispatch-leg-only
//!      test). Pinned here at the wire with v4's byte-exact `ZodError.message`.

mod common;

use quilltap_core::db::Writer;
use serde_json::{json, Value};

/// v4's `ZodError.message` for `SmartTypographySettingsSchema.parse(null)` —
/// `JSON.stringify(issues, null, 2)`, recorded from v4's real route by the
/// `s_put_smart_typo_null` oracle case.
const ZOD_NULL_BAG_MESSAGE: &str = "[\n  {\n    \"expected\": \"object\",\n    \"code\": \
     \"invalid_type\",\n    \"path\": [],\n    \"message\": \"Invalid input: expected object, \
     received null\"\n  }\n]";

const COMPOSER_COLUMNS: [&str; 3] = [
    "composerEmoji",
    "composerUnicode",
    "smartTypographySettings",
];

fn column_present(conn: &rusqlite::Connection, col: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('chat_settings') WHERE name = ?1",
        [col],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

async fn dispatch(
    client: &reqwest::Client,
    addr: &std::net::SocketAddr,
    body: Value,
) -> (u16, Value) {
    let resp = client
        .post(format!("http://{addr}/api/dispatch"))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn composer_settings_web_edges() {
    let base = common::materialize_fixture_instance();
    let data = base.path().join("data");

    // Put the table back to its pre-4.8.2 shape regardless of the committed
    // fixture's vintage, so the ensure is genuinely under test (the
    // `p4.9h2a` vintage lesson: a fixture that already carries the columns
    // turns this into a vacuous pass).
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        for col in COMPOSER_COLUMNS {
            if column_present(w.connection(), col) {
                w.connection()
                    .execute_batch(&format!(
                        "ALTER TABLE \"chat_settings\" DROP COLUMN \"{col}\""
                    ))
                    .unwrap();
            }
            assert!(
                !column_present(w.connection(), col),
                "{col} must be absent before boot"
            );
        }
    }

    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // --- the ensure ran: the three columns exist on the booted instance ---
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        for col in COMPOSER_COLUMNS {
            assert!(
                column_present(w.connection(), col),
                "{col} must exist after the boot ensure"
            );
        }
    }

    // --- GET: the three keys carry v4's defaults ---
    let (status, body) = dispatch(&client, &addr, json!({ "type": "chatSettings" })).await;
    assert_eq!(status, 200, "settings GET");
    let s = &body["data"];
    assert_eq!(s["composerEmoji"], json!(true));
    assert_eq!(s["composerUnicode"], json!(true));
    assert_eq!(
        s["smartTypographySettings"],
        json!({"displayQuotes": false, "dashes": true, "ellipsis": true})
    );

    // --- PUT: single-key payloads, exactly as the SPA saves `composerSpellcheck` ---
    let (status, body) = dispatch(
        &client,
        &addr,
        json!({
            "type": "chatSettingsUpdate",
            "settings": { "composerEmoji": false }
        }),
    )
    .await;
    assert_eq!(status, 200, "composerEmoji PUT");
    assert_eq!(body["data"]["composerEmoji"], json!(false), "echo");

    let (status, body) = dispatch(
        &client,
        &addr,
        json!({
            "type": "chatSettingsUpdate",
            // A PARTIAL bag: the two absent keys take their Zod defaults.
            "settings": { "smartTypographySettings": { "displayQuotes": true } }
        }),
    )
    .await;
    assert_eq!(status, 200, "smartTypographySettings PUT");
    assert_eq!(
        body["data"]["smartTypographySettings"],
        json!({"displayQuotes": true, "dashes": true, "ellipsis": true}),
        "partial bag echo"
    );

    // --- the writes STUCK (the whole point of the ensure) ---
    let (_, body) = dispatch(&client, &addr, json!({ "type": "chatSettings" })).await;
    let s = &body["data"];
    assert_eq!(
        s["composerEmoji"],
        json!(false),
        "composerEmoji must persist — an un-ensured column 500s the PUT outright"
    );
    assert_eq!(s["composerUnicode"], json!(true), "untouched key");
    assert_eq!(
        s["smartTypographySettings"],
        json!({"displayQuotes": true, "dashes": true, "ellipsis": true}),
        "smartTypographySettings must persist"
    );

    // --- an EXPLICIT null bag reaches the handler and 400s with v4's bytes ---
    let (status, body) = dispatch(
        &client,
        &addr,
        json!({
            "type": "chatSettingsUpdate",
            "settings": { "smartTypographySettings": null }
        }),
    )
    .await;
    assert_eq!(
        status, 400,
        "an explicit null must not pass as an absent key"
    );
    assert_eq!(body["data"]["message"], json!(ZOD_NULL_BAG_MESSAGE));

    // --- a wrong-typed boolean 400s with v4's fixed sentence ---
    let (status, body) = dispatch(
        &client,
        &addr,
        json!({
            "type": "chatSettingsUpdate",
            "settings": { "composerUnicode": "yes" }
        }),
    )
    .await;
    assert_eq!(status, 400, "non-boolean composerUnicode");
    assert_eq!(
        body["data"]["message"],
        json!("Invalid composerUnicode value (must be boolean)")
    );

    // --- and a rejected PUT left the stored values untouched ---
    let (_, body) = dispatch(&client, &addr, json!({ "type": "chatSettings" })).await;
    assert_eq!(body["data"]["composerEmoji"], json!(false));
    assert_eq!(body["data"]["composerUnicode"], json!(true));
    assert_eq!(
        body["data"]["smartTypographySettings"],
        json!({"displayQuotes": true, "dashes": true, "ellipsis": true})
    );
}
