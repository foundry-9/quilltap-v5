//! P4.96 tier-1 item 5 — the profile-id generate verb's five shaping keys AT
//! THE DISPATCH WIRE.
//!
//! v5 has **no** `/api/v1/image-profiles` REST edge (`quilltap-web::lib`
//! registers `embedding-profiles`, `images`, `images/{id}` and `user/profile`;
//! every hit for `image-profiles` in the crate is census prose). The only HTTP
//! path to this verb is `POST /api/dispatch` → `serde_json::from_slice::<Request>`
//! (`dispatch.rs:69`), and that is what this file drives. Adding a REST edge is
//! NOT this order's — it is recorded as P4.96's Tier-3 deferral so the next
//! reader does not re-derive the measurement.
//!
//! What the differential (`image_generate_route_equivalence`) CANNOT see: it
//! calls `image_profile_generate` directly, so it proves the handler is
//! v4-faithful but says nothing about the trip through serde. That trip is the
//! whole reason the five ride RAW: a serde-TYPED field would refuse a wrong
//! value at the DECODE, and the web edge would answer serde's sentence where v4
//! answers `{"error":"Validation error","details":[…]}` — while Tauri IPC
//! answered something else again. That is the divergence P4.62/P4.73 ruled out,
//! and it is invisible from either side alone.
//!
//! The venue boots `ProductionSpineFactory` (`web-test-venue-has-no-spine-factory`):
//! without it `ready_generate_image()` answers the not-assembled refusal and the
//! parse stage is never reached at all. The seeded profile points at a dead
//! socket, so the one body that PASSES validation fails downstream at the
//! provider with no spend — which is itself the discriminator between "the
//! gate let it through" and "the gate refused it".
//!
//! Run:
//!   cargo test -p quilltap-web --test image_profile_generate_dispatch_wire

mod common;

use quilltap_core::db::Writer;
use serde_json::{json, Value};

/// A profile the 404 gate lets through, pointed at a dead socket.
const PROFILE: &str = "11110000-0000-4000-8000-0000000000aa";
const BOGUS_PROFILE: &str = "e0000000-0000-4000-8000-0000000000ff";

/// Seed one image profile into the materialized instance's main partition.
/// The fixture carries none, and the parse stage sits BEHIND the 404 gate —
/// without a real row every body in this file would answer 404 and the test
/// would prove nothing (a vacuous-green shape this tree has been bitten by).
fn seed_profile(base: &std::path::Path) {
    let w = Writer::open_writable(&base.join("data/quilltap.db"), common::TEST_PEPPER).unwrap();
    // `image_profiles` is a LAZY collection: the committed chat-send fixture
    // never touched it, so the table does not exist
    // (`a-lazy-repo-fixture-lacks-the-tables-a-boot-touches`). Create it from
    // the PROVISIONER's own statement rather than a transcription, so a D23
    // re-dump moves this test with the schema instead of leaving it stale.
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../quilltap-core/src/services/provisioning/fresh_schema.json"),
        )
        .expect("read fresh_schema.json"),
    )
    .expect("parse fresh_schema.json");
    let ddl = schema["main"]
        .as_array()
        .expect("main statements")
        .iter()
        .filter_map(Value::as_str)
        .find(|s| s.contains("CREATE TABLE \"image_profiles\""))
        .expect("the image_profiles CREATE TABLE statement");
    w.connection().execute_batch(ddl).unwrap();
    w.connection()
        .execute(
            "INSERT INTO image_profiles \
               (id, userId, name, provider, apiKeyId, baseUrl, modelName, parameters, \
                isDefault, isDangerousCompatible, tags, createdAt, updatedAt) \
             VALUES (?1, ?2, 'Wire probe', 'OPENAI', NULL, 'http://127.0.0.1:1', \
                     'dall-e-3', '{}', 0, 0, '[]', \
                     '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
            rusqlite::params![PROFILE, common::FIXTURE_USER],
        )
        .unwrap();
}

/// The dispatch transport answers the tagged envelope
/// `{"type":"error","data":{kind,message}}`, and MERGES v4's flat
/// `{error, details}` alongside it for the one refusal that carries
/// `CoreError::details` (`validation_wire_body`). So a Zod refusal is readable
/// flat and every other refusal is not — which is itself a discriminator, and
/// why the flat `error` key is asserted directly wherever the Zod envelope is
/// the point.
fn sentence(v: &Value) -> &str {
    v.get("error")
        .and_then(Value::as_str)
        .or_else(|| v.pointer("/data/message").and_then(Value::as_str))
        .unwrap_or_default()
}

fn body(extra: Value) -> Value {
    let mut v = json!({
        "type": "imageProfileGenerate",
        "imageProfileId": PROFILE,
        "prompt": "A kite over the fens",
        "count": 1,
    });
    let o = v.as_object_mut().unwrap();
    for (k, val) in extra.as_object().unwrap() {
        o.insert(k.clone(), val.clone());
    }
    v
}

#[tokio::test(flavor = "multi_thread")]
async fn the_five_shaping_keys_are_refused_by_the_handler_not_the_decode() {
    let base = common::materialize_fixture_instance();
    seed_profile(base.path());
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
    let url = format!("http://{addr}/api/dispatch");
    let post = |b: Value| {
        let client = client.clone();
        let url = url.clone();
        async move {
            let r = client.post(url).json(&b).send().await.unwrap();
            let status = r.status().as_u16();
            let v: Value = r.json().await.unwrap();
            (status, v)
        }
    };

    // ---- 1. A wrong-typed `quality` answers v4's ZOD ENVELOPE, at the wire.
    //
    // This is the assertion the whole raw crossing exists for. Retype `quality`
    // to `Option<String>` on the variant and this arm reddens: the decode would
    // refuse first and the body would carry serde's
    // `invalid type: integer \`7\`, expected a string` instead.
    let (status, v) = post(body(json!({ "quality": 7 }))).await;
    assert_eq!(status, 400, "a wrong-typed quality is a 400: {v}");
    assert_eq!(
        v.get("error").and_then(Value::as_str),
        Some("Validation error"),
        "the wire must carry v4's fixed sentence, not serde's: {v}"
    );
    let details = v
        .get("details")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("v4's `details` array is missing from the wire body: {v}"));
    assert_eq!(details.len(), 1, "one issue for one bad key: {v}");
    assert_eq!(details[0]["code"], json!("invalid_value"));
    assert_eq!(details[0]["path"], json!(["quality"]));
    assert_eq!(
        details[0]["values"],
        json!(["auto", "low", "medium", "high", "xhigh", "max", "standard", "hd"]),
        "the shared eight-tier list reaches the wire: {v}"
    );
    let rendered = serde_json::to_string(&v).unwrap();
    assert!(
        !rendered.contains("invalid type"),
        "serde's decode sentence leaked to the wire — the field is typed again: {rendered}"
    );

    // ---- 2. …and so do the other four, each naming its own key.
    for (key, wrong, code) in [
        ("size", json!(1024), "invalid_type"),
        ("size", json!(Value::Null), "invalid_type"),
        ("style", json!("sketch"), "invalid_value"),
        ("aspectRatio", json!(true), "invalid_type"),
        ("negativePrompt", json!({}), "invalid_type"),
    ] {
        let (status, v) = post(body(json!({ key: wrong }))).await;
        assert_eq!(status, 400, "{key}: {v}");
        assert_eq!(
            v.get("error").and_then(Value::as_str),
            Some("Validation error"),
            "{key}: {v}"
        );
        let d = v.get("details").and_then(Value::as_array).unwrap();
        assert_eq!(d[0]["code"], json!(code), "{key}: {v}");
        assert_eq!(d[0]["path"], json!([key]), "{key}: {v}");
    }

    // ---- 3. A well-formed body carrying ALL FIVE gets PAST the gate.
    //
    // The discriminator: it must NOT answer `Validation error`. The profile
    // points at a dead socket, so it fails downstream at the provider instead —
    // which is what proves the five crossed serde intact and satisfied the
    // parse stage rather than being dropped or refused.
    let (status, v) = post(body(json!({
        "size": "1024x1536",
        "quality": "high",
        "style": "vivid",
        "aspectRatio": "16:9",
        "negativePrompt": "no wires",
    })))
    .await;
    let err = sentence(&v);
    assert_ne!(
        err, "Validation error",
        "a well-formed five-key body was refused by the parse stage: {status} {v}"
    );
    assert!(
        v.get("details").is_none(),
        "a non-Zod refusal must not carry a `details` array: {v}"
    );
    assert!(
        !err.contains("not assembled") && !err.contains("unavailable"),
        "the image-generation seam is not wired in this venue — the test is \
         measuring the refusal, not the gate: {status} {v}"
    );

    // ---- 4. The guard order, at the wire: v4 reads the body only after
    //         `findById`, so a missing profile with a garbage body is a 404.
    let mut b = body(json!({ "quality": 7, "size": Value::Null }));
    b.as_object_mut()
        .unwrap()
        .insert("imageProfileId".into(), json!(BOGUS_PROFILE));
    let (status, v) = post(b).await;
    assert_eq!(status, 404, "404 must beat 400: {v}");
    assert_eq!(sentence(&v), "Image profile not found", "{v}");
    assert!(
        v.get("details").is_none() && v.pointer("/data/details").is_none(),
        "the 404 arm must carry no Zod issues — the body was never parsed: {v}"
    );

    // ---- 5. The control: `prompt` and `count` are still serde-TYPED, so the
    //         DECODE refuses them before the handler — the recorded
    //         pre-existing narrowing P4.96 does not reopen, pinned here so the
    //         day it changes this file says so.
    for (key, wrong) in [("prompt", json!(5)), ("count", json!("2"))] {
        let mut b = body(json!({}));
        b.as_object_mut().unwrap().insert(key.into(), wrong);
        let (status, v) = post(b).await;
        let rendered = serde_json::to_string(&v).unwrap();
        assert!(
            status == 400 && rendered.contains("invalid type"),
            "`{key}` stopped being serde-typed at the edge — it now reaches the \
             handler and its census classification has moved: {status} {rendered}"
        );
    }
}
