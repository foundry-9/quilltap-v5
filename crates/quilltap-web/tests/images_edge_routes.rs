//! The `/api/v1/images` COLLECTION edge's OWN arms — the ones the
//! `images_routes_equivalence` family cannot see because it drives the verbs
//! directly (its header says so, and until the follow-ups-round-2 unification
//! it named a driver for them that did not exist).
//!
//! v4's answers, read from `app/api/v1/images/route.ts` and the context
//! middleware (`lib/api/middleware/context.ts:158-208`):
//!
//! * `await request.json()` / `await request.formData()` THROWING (an
//!   unreadable or unparseable body) is not a ZodError, so it falls to
//!   `handleRouteError`'s final arm — **500 `Internal server error`**. v5's edge
//!   used to answer a 400 (`Validation error` / a v5-invented `Invalid multipart
//!   body`) on both.
//! * a JSON body that parses to a NON-object still reaches
//!   `importFromUrlSchema.parse`, which refuses it — **400 `Validation error`**.
//! * neither JSON nor multipart → **400 `Invalid content type`** (`:505`).
//! * multipart without a `file` part → **400 `No file provided`** (`:459`).
//! * a `tags` form field that is not JSON → **400 `Invalid tags JSON`** (`:470`).
//! * `?action=generate` → the P4.76 generate leg (never a
//!   fall-through to upload).
//!
//! Runs without an oracle (`cargo test -p quilltap-web --test images_edge_routes`).

mod common;

use serde_json::{json, Value};

fn materialize_instance() -> tempfile::TempDir {
    let base = tempfile::tempdir().expect("tempdir");
    let data = base.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    for (fixture, name) in [
        ("images-main.db", "quilltap.db"),
        ("images-mount.db", "quilltap-mount-index.db"),
    ] {
        std::fs::copy(common::fixtures_dir().join(fixture), data.join(name))
            .unwrap_or_else(|e| panic!("copy {fixture}: {e}"));
    }
    base
}

fn multipart(fields: &[(&str, &str)]) -> (String, String) {
    let boundary = "qtimgedge";
    let mut body = String::new();
    for (name, value) in fields {
        body.push_str(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        ));
    }
    body.push_str(&format!("--{boundary}--\r\n"));
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_collection_post_edge_answers_v4s_own_arms() {
    let base = materialize_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        // The images fixture is keyed with the images corpus pepper
        // (`images-collection.json` `testPepperBase64`), not the venue's default.
        c.env_pepper = Some("dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHktMzJieXRl".to_string());
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/v1/images");

    async fn post(
        client: &reqwest::Client,
        url: &str,
        content_type: &str,
        body: String,
    ) -> (u16, Value) {
        let resp = client
            .post(url)
            .header("content-type", content_type)
            .body(body)
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    // Unparseable JSON — v4's `request.json()` throws → the unhandled-error 500.
    let (status, body) = post(&client, &url, "application/json", "{not json".into()).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (500, Some("Internal server error")),
        "{body}"
    );

    // A JSON body that PARSES to a non-object reaches the Zod refusal.
    let (status, body) = post(&client, &url, "application/json", "42".into()).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("Validation error")),
        "{body}"
    );

    // A multipart content-type over a body that is not multipart — v4's
    // `request.formData()` throws → the same unhandled-error 500.
    let (status, body) = post(
        &client,
        &url,
        "multipart/form-data; boundary=zz",
        "this is not a multipart body".into(),
    )
    .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (500, Some("Internal server error")),
        "{body}"
    );

    // Neither JSON nor multipart.
    let (status, body) = post(&client, &url, "text/plain", "hello".into()).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("Invalid content type")),
        "{body}"
    );

    // Multipart without a `file` part.
    let (ct, mp) = multipart(&[("tags", "[]")]);
    let (status, body) = post(&client, &url, &ct, mp).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("No file provided")),
        "{body}"
    );

    // A `tags` field that is not JSON, with a file present — v4 parses `tags`
    // BEFORE it reads the file bytes, so the sentence wins even here.
    let (ct, mp) = multipart(&[("tags", "{bad")]);
    let mp_with_file = mp.replace(
        "--qtimgedge--\r\n",
        "--qtimgedge\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.svg\"\r\nContent-Type: image/svg+xml\r\n\r\n<svg/>\r\n--qtimgedge--\r\n",
    );
    let (status, body) = post(&client, &url, &ct, mp_with_file).await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("Invalid tags JSON")),
        "{body}"
    );

    // P4.76: the generate leg is SERVED. An empty body reaches v4's
    // `generateImageSchema.parse`, which refuses the missing prompt — so the
    // edge's own proof is that the action routes there at all (a fall-through
    // to upload would answer `Invalid content type`, and the pre-P4.76 refusal
    // answered a 500 naming itself).
    let (status, body) = post(
        &client,
        &format!("{url}?action=generate"),
        "application/json",
        "{}".into(),
    )
    .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("Validation error")),
        "{body}"
    );
    // …and a body that does not PARSE takes v4's `await request.json()` throw:
    // the middleware's flat 500, never a 400 and never the upload leg.
    let (status, body) = post(
        &client,
        &format!("{url}?action=generate"),
        "application/json",
        "{not json".into(),
    )
    .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (500, Some("Internal server error")),
        "{body}"
    );

    // v4 bug 130 (`86d59660c`): the edge must FORWARD `chatId`. Nothing else
    // pins that — `images_generate_route_equivalence` drives the core handler
    // directly, so an edge that silently drops the key leaves the whole bug-130
    // corpus vacuously green. The discriminator is the handler's own
    // `z.uuid()` refusal: a malformed `chatId` can only produce a 400 if the
    // value ARRIVED.
    let (status, body) = post(
        &client,
        &format!("{url}?action=generate"),
        "application/json",
        json!({
            "prompt": "a brass observatory at dusk",
            "profileId": "aaaa0000-0000-4000-8000-000000000001",
            "chatId": "not-a-uuid",
        })
        .to_string(),
    )
    .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("Validation error")),
        "a malformed chatId must reach the handler's uuid gate: {body}"
    );
}
