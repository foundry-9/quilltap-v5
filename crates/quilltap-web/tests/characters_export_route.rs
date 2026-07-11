//! Route-level integration for `GET /api/v1/characters/{id}?action=export`
//! (P4.6m unit 3): the PNG binary leg (real-avatar embed + the placeholder leg),
//! the JSON leg, and the error arms, against the committed characters fixture.

mod common;

use quilltap_core::db::Writer;
use quilltap_core::services::sillytavern::parse_st_character_png;
use serde_json::Value;

const ARIA: &str = "a1000000-0000-4000-8000-000000000001";

#[tokio::test(flavor = "multi_thread")]
async fn characters_export_png_and_json() {
    // --- Instance A: Aria keeps her avatar (the real-avatar embed leg) ---
    let base = common::materialize_characters_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();

    // PNG export: the card JSON is embedded in a `chara` tEXt chunk; the avatar
    // bytes are the container (Aria's avatar is a WebP → v4 clamps the IHDR walk
    // and appends the tEXt, byte-for-byte identical to the port).
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/characters/{ARIA}?action=export&format=png"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["content-type"], "image/png");
    let cd = resp.headers()["content-disposition"].to_str().unwrap();
    assert!(cd.starts_with("attachment; filename=\""), "{cd}");
    assert!(cd.ends_with(".png\""), "{cd}");
    let png = resp.bytes().await.unwrap();
    // The embedded ST card is present (keyword + spec).
    assert!(contains(&png, b"chara\0"), "tEXt keyword missing");
    assert!(contains(&png, b"chara_card_v2"), "card spec missing");

    // JSON export: pretty-printed ST card as an attachment download.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/characters/{ARIA}?action=export"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["content-type"], "application/json");
    let card: Value = resp.json().await.unwrap();
    assert_eq!(card["spec"], "chara_card_v2");
    assert!(card["data"]["name"].as_str().is_some());

    // Unknown character → 404.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/characters/no-such?action=export&format=png"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    // A non-export action → 400 (JSON reads live on /api/dispatch).
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/characters/{ARIA}?action=chats"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // --- Instance B: null Aria's avatar → the placeholder leg (a real 256×256
    //     PNG that round-trips through parse_st_character_png). ---
    let base2 = common::materialize_characters_instance();
    {
        let w = Writer::open_writable(&base2.path().join("data/quilltap.db"), common::TEST_PEPPER)
            .unwrap();
        w.connection()
            .execute(
                "UPDATE characters SET defaultImageId = NULL WHERE id = ?1",
                rusqlite::params![ARIA],
            )
            .unwrap();
    }
    let (addr2, _state2) = common::serve_instance(base2.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let resp = client
        .get(format!(
            "http://{addr2}/api/v1/characters/{ARIA}?action=export&format=png"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let png = resp.bytes().await.unwrap();
    // A valid PNG signature + a card that parses back out.
    assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    let card = parse_st_character_png(&png).expect("placeholder PNG round-trips the card");
    assert!(card["name"].as_str().is_some(), "{card}");
    assert_eq!(card["spec"], Value::Null); // parse returns `.data`, not the wrapper
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
