//! Route-level integration for `POST /api/v1/characters?action=import` (P4.6m
//! unit 4): the multipart PNG leg (create + the vault avatar write + the
//! defaultImageId set) and the JSON-file leg + error arms, against the
//! characters fixture. The written avatar is verified end-to-end by exporting
//! the created character (its WebP avatar becomes the export container).
//!
//! Run:
//!   cargo test -p quilltap-web --test characters_import_route

mod common;

use quilltap_core::services::sillytavern::create_st_character_png;
use serde_json::{json, Value};

/// A ST-card PNG for import: the internal character exported into a `tEXt` chunk
/// over a generated placeholder (a valid 256×256 PNG).
fn st_card_png(name: &str) -> Vec<u8> {
    let character = json!({
        "name": name,
        "description": "An imported adventurer.",
        "personality": "Curious and brave.",
        "firstMessage": "Greetings, traveller.",
        "exampleDialogues": "",
        "scenarios": [{ "title": "Default", "content": "A crossroads at dawn." }],
        "systemPrompts": [{ "name": "Default", "isDefault": true, "content": "You are the hero." }],
    });
    create_st_character_png(&character, None)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[tokio::test(flavor = "multi_thread")]
async fn characters_import_png_and_json_and_arms() {
    let base = common::materialize_characters_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let import_url = format!("http://{addr}/api/v1/characters?action=import");

    // --- PNG import: create + avatar write + defaultImageId ---
    let png = st_card_png("Imported Hero");
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(png.clone())
            .file_name("hero.png")
            .mime_str("image/png")
            .unwrap(),
    );
    let resp = client
        .post(&import_url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "png import");
    let body: Value = resp.json().await.unwrap();
    let ch = &body["character"];
    assert_eq!(ch["name"], "Imported Hero");
    assert_eq!(ch["description"], "An imported adventurer.");
    let created_id = ch["id"].as_str().expect("created id").to_string();
    let default_image_id = ch["defaultImageId"].as_str().expect("defaultImageId set");
    assert!(!default_image_id.is_empty());
    assert_eq!(ch["_count"]["chats"], 0);

    // The avatar landed in the vault: export the created character and confirm
    // the container is the transcoded WebP (RIFF/WEBP) carrying the card.
    let resp = client
        .get(format!(
            "http://{addr}/api/v1/characters/{created_id}?action=export&format=png"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let exported = resp.bytes().await.unwrap();
    assert!(contains(&exported, b"WEBP"), "avatar not stored as WebP");
    assert!(contains(&exported, b"chara_card_v2"), "card missing");

    // --- JSON-file import (no avatar) ---
    let card = json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": {
            "name": "JSON Import",
            "description": "From a .json card.",
            "personality": "Terse.",
            "first_mes": "Hi.",
        }
    });
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(serde_json::to_vec(&card).unwrap())
            .file_name("card.json")
            .mime_str("application/json")
            .unwrap(),
    );
    let resp = client
        .post(&import_url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "json import");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["character"]["name"], "JSON Import");
    assert_eq!(body["character"]["defaultImageId"], Value::Null);

    // --- Error arms ---
    // No file field.
    let form = reqwest::multipart::Form::new().text("x", "y");
    let resp = client
        .post(&import_url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // A PNG that isn't a SillyTavern card → 400 "Invalid SillyTavern PNG file".
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"\x89PNG\r\n\x1a\nnot-a-card".to_vec())
            .file_name("plain.png")
            .mime_str("image/png")
            .unwrap(),
    );
    let resp = client
        .post(&import_url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let b: Value = resp.json().await.unwrap();
    assert_eq!(b["error"], "Invalid SillyTavern PNG file");

    // An unsupported file type → 400.
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("notes.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let resp = client
        .post(&import_url)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let b: Value = resp.json().await.unwrap();
    assert!(b["error"]
        .as_str()
        .unwrap()
        .contains("Unsupported file type"));

    // Wrong action → 400 pointer.
    let resp = client
        .post(format!(
            "http://{addr}/api/v1/characters?action=quick-create"
        ))
        .multipart(reqwest::multipart::Form::new().text("name", "x"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}
