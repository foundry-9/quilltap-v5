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
//!
//! P4.D179 added a second test in the same shape for v4 4.10 `686954937`'s
//! `impersonationVoiceRewrite`; P4.D251 (v4 `07b8f0209`) RE-SHAPED it for the
//! `impersonationVoiceMode` enum that replaced the boolean — the same two
//! plumbing claims, with the boot ensure now a three-step migration (ADD the
//! mode, translate the retired column's `1` → `'ask'`, DROP it), so the boot
//! arm plants the OLD column with `1` and no mode column and reads `'ask'`
//! back through the wire. Its refusal arm is v4's fixed SENTENCE (`must be
//! one of off, ask, always`), not a Zod envelope, because the route guards it
//! by hand; a body carrying the RETIRED key is ignored (200, unchanged). A
//! third test plants BOTH columns — the §R.13 ping-pong shape a v5 still
//! running the P4.D179 ensure would leave on a v4-migrated file — and proves
//! the boot heals it (the old column gone, the mode untouched).
//!
//! Run:
//!   cargo test -p quilltap-web --test chat_settings_composer_web_routes

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

const OLD_COL: &str = "impersonationVoiceRewrite";
const MODE_COL: &str = "impersonationVoiceMode";
const MODE_SENTENCE: &str =
    "Invalid impersonationVoiceMode value (must be one of off, ask, always)";

fn table_sql(conn: &rusqlite::Connection) -> String {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE name = 'chat_settings'",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

/// Put the instance's `chat_settings` into a chosen pre-`07b8f0209` shape:
/// the mode column DROPPED (if the committed fixture's vintage carries it),
/// the retired column ADDED through v4's own add-field DDL (if absent) and
/// every row's value set to `old_value`.
fn plant_old_column(data: &std::path::Path, old_value: i64, keep_mode: Option<&str>) {
    let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
    let conn = w.connection();
    if column_present(conn, MODE_COL) && keep_mode.is_none() {
        conn.execute_batch(&format!(
            "ALTER TABLE \"chat_settings\" DROP COLUMN \"{MODE_COL}\""
        ))
        .unwrap();
    }
    // The ping-pong ORDER: v4's migration appended the mode first, then the
    // stale P4.D179 ensure re-appended the old column after it.
    if let Some(mode) = keep_mode {
        if !column_present(conn, MODE_COL) {
            conn.execute_batch(&format!(
                "ALTER TABLE \"chat_settings\" ADD COLUMN \"{MODE_COL}\" TEXT DEFAULT 'off'"
            ))
            .unwrap();
        }
        conn.execute(
            &format!("UPDATE chat_settings SET \"{MODE_COL}\" = ?1"),
            [mode],
        )
        .unwrap();
    }
    if !column_present(conn, OLD_COL) {
        conn.execute_batch(&format!(
            "ALTER TABLE \"chat_settings\" ADD COLUMN \"{OLD_COL}\" INTEGER DEFAULT 0"
        ))
        .unwrap();
    }
    conn.execute(
        &format!("UPDATE chat_settings SET \"{OLD_COL}\" = ?1"),
        [old_value],
    )
    .unwrap();
    assert!(
        column_present(conn, OLD_COL),
        "{OLD_COL} must be present before boot"
    );
    assert_eq!(
        column_present(conn, MODE_COL),
        keep_mode.is_some(),
        "{MODE_COL} presence before boot"
    );
}

/// P4.D251 (v4 `07b8f0209`): the `impersonationVoiceMode` column, end to end
/// over a live server. The boot ensure is v4's `impersonation-voice-mode-v1`
/// re-homed — ADD + translate + DROP — so the instance is planted in the
/// §R.13 Friday shape (the retired boolean holding `1`, no mode column) and
/// the wire must read `'ask'` back with the old column GONE.
#[tokio::test(flavor = "multi_thread")]
async fn impersonation_voice_mode_web_edges() {
    let base = common::materialize_fixture_instance();
    let data = base.path().join("data");
    plant_old_column(&data, 1, None);

    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // --- the ensure ran: the mode column exists, the retired one is GONE ---
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        assert!(
            column_present(w.connection(), MODE_COL),
            "{MODE_COL} must exist after boot"
        );
        assert!(
            !column_present(w.connection(), OLD_COL),
            "{OLD_COL} must be DROPPED by the boot ensure"
        );
    }

    // --- GET: the planted `1` was translated to v4's 'ask' ---
    let (status, body) = dispatch(&client, &addr, json!({ "type": "chatSettings" })).await;
    assert_eq!(status, 200, "settings GET");
    assert_eq!(
        body["data"][MODE_COL],
        json!("ask"),
        "1 → 'ask' (v4's translation)"
    );
    assert!(
        body["data"].get(OLD_COL).is_none(),
        "the retired key is never emitted"
    );

    // --- PUT: the single-key payload the SPA saves, round-tripping 'always' ---
    let (status, body) = dispatch(
        &client,
        &addr,
        json!({ "type": "chatSettingsUpdate", "settings": { MODE_COL: "always" } }),
    )
    .await;
    assert_eq!(status, 200, "impersonationVoiceMode PUT");
    assert_eq!(body["data"][MODE_COL], json!("always"), "echo");
    let (_, body) = dispatch(&client, &addr, json!({ "type": "chatSettings" })).await;
    assert_eq!(
        body["data"][MODE_COL],
        json!("always"),
        "the value must persist"
    );

    // --- the refusals: an EXPLICIT null, a number, a stranger, and the OLD
    //     boolean TYPE under the new key — all v4's fixed sentence ---
    for bad in [json!(null), json!(1), json!("yes"), json!(true)] {
        let (status, body) = dispatch(
            &client,
            &addr,
            json!({ "type": "chatSettingsUpdate", "settings": { MODE_COL: bad } }),
        )
        .await;
        assert_eq!(status, 400, "a present-and-invalid value ({bad}) must 400");
        assert_eq!(body["data"]["message"], json!(MODE_SENTENCE), "{bad}");
    }

    // --- the RETIRED key is ignored: 200, the row unchanged ---
    let (status, body) = dispatch(
        &client,
        &addr,
        json!({ "type": "chatSettingsUpdate", "settings": { OLD_COL: true } }),
    )
    .await;
    assert_eq!(
        status, 200,
        "a body carrying the retired key is ignored, not refused"
    );
    assert_eq!(body["data"][MODE_COL], json!("always"), "nothing written");
    assert!(body["data"].get(OLD_COL).is_none());

    // --- and the rejected PUTs left the stored value untouched ---
    let (_, body) = dispatch(&client, &addr, json!({ "type": "chatSettings" })).await;
    assert_eq!(body["data"][MODE_COL], json!("always"));
}

/// P4.D251 Tier 2 item 13 (§R.13): the ping-pong heal. A file where v4's
/// migration already wrote the mode and a v5 still carrying the P4.D179
/// ensure re-added the retired column after it (old `1`, mode `'ask'`) boots
/// into: the old column gone, the mode UNTOUCHED (v4's `IS NULL OR = 'off'`
/// predicate leaves a non-default row alone), the table's `sqlite_master.sql`
/// no longer naming the retired column, and a second boot an exact no-op.
#[tokio::test(flavor = "multi_thread")]
async fn impersonation_voice_mode_ping_pong_shape_is_healed_at_boot() {
    let base = common::materialize_fixture_instance();
    let data = base.path().join("data");
    plant_old_column(&data, 1, Some("ask"));
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        let cols: Vec<String> = {
            let mut stmt = w
                .connection()
                .prepare("PRAGMA table_info('chat_settings')")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(
            cols.last().map(String::as_str),
            Some(OLD_COL),
            "the old column LAST"
        );
    }

    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    let healed_sql = {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        assert!(
            !column_present(w.connection(), OLD_COL),
            "the stale re-add is DROPPED"
        );
        assert!(column_present(w.connection(), MODE_COL));
        let sql = table_sql(w.connection());
        assert!(!sql.contains(OLD_COL), "{sql}");
        sql
    };
    let (status, body) = dispatch(&client, &addr, json!({ "type": "chatSettings" })).await;
    assert_eq!(status, 200);
    assert_eq!(
        body["data"][MODE_COL],
        json!("ask"),
        "a non-default mode is left alone"
    );

    // A second pass over the healed table changes nothing.
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), common::TEST_PEPPER).unwrap();
        let outcome = quilltap_core::db::chat_settings_impersonation_voice_mode_repair::
            ensure_chat_settings_impersonation_voice_mode(w.connection())
        .unwrap();
        assert_eq!(outcome, Default::default(), "the second run is a no-op");
        assert_eq!(table_sql(w.connection()), healed_sql);
    }
}
