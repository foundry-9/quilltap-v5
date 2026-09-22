//! P4.D210 — the sync verb's six body keys AT THE DISPATCH WIRE, and its REST
//! arm beside them.
//!
//! `mount_sync_action_equivalence` drives `decode_sync_options` DIRECTLY with a
//! tri-state it constructs itself, so the trip through serde — the one place an
//! explicit JSON `null` can be silently eaten — is exactly the part it cannot
//! reach (`a-dispatch-wire-test-sees-what-the-differential-cannot`). This file
//! is that part.
//!
//! What a collapse would cost here is not hypothetical. v4's `syncSchema` spells
//! all five defaulted keys `.optional().default(x)`, which accepts an ABSENT key
//! and refuses a present `null`. A plain `#[serde(default)] Option<Value>` maps
//! both onto `None`, so `{"propagateDeletes": null}` would take the default
//! `true` and **run a sync that deletes**, where v4 answers 400 and does
//! nothing. `dryRun: null` is the same shape with the same teeth: v4 refuses,
//! a collapsed decode runs for real.
//!
//! **The discriminator is the store lookup.** v4 parses the body FIRST and looks
//! the store up SECOND, so a body that satisfies the schema against an absent
//! `mountPointId` answers `Mount point not found` — a 404 that no schema failure
//! can produce. Every arm below is read against that, so a green arm means "the
//! schema accepted this and the next gate refused", never "something earlier
//! refused for another reason".
//!
//! **The Tauri IPC leg is proven by construction.** `quilltap-tauri`'s `dispatch`
//! command calls `quilltap_web::dispatch::dispatch_body` — the same
//! `serde_json::from_slice::<Request>` this file drives over HTTP. One decoder,
//! both transports.
//!
//! Run:
//!   cargo test -p quilltap-web --test mount_sync_dispatch_wire

mod common;

use serde_json::{json, Value};

/// A well-formed uuid that names no store — the discriminator (see the header).
const ABSENT_STORE: &str = "aaaa0000-0000-4000-8000-0000000000ff";

/// The typed envelope's sentence, with v4's flat `error` key read first for the
/// day the `details` deferral is lifted.
fn sentence(v: &Value) -> &str {
    v.get("error")
        .and_then(Value::as_str)
        .or_else(|| v.pointer("/data/message").and_then(Value::as_str))
        .unwrap_or_default()
}

fn body(extra: Value) -> Value {
    let mut v = json!({
        "type": "mountSync",
        "mountPointId": ABSENT_STORE,
        "targetPath": "/tmp/qt-sync-wire-target",
    });
    let o = v.as_object_mut().unwrap();
    for (k, val) in extra.as_object().unwrap() {
        o.insert(k.clone(), val.clone());
    }
    v
}

#[tokio::test(flavor = "multi_thread")]
async fn the_six_body_keys_are_refused_by_the_handler_not_the_decode() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
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

    // ---- 0. The DISCRIMINATOR. A body carrying every key well-formed
    //         satisfies the schema and dies at the store lookup. If this arm
    //         ever answers a 400, the rest of this file is measuring the gate
    //         rather than the keys.
    let (status, v) = post(body(json!({
        "dryRun": true,
        "direction": "to-disk",
        "prefer": "store",
        "propagateDeletes": false,
        "useManifest": false,
    })))
    .await;
    assert_eq!(
        status, 404,
        "the discriminator body must reach the store lookup, not the schema: {v}"
    );
    assert_eq!(sentence(&v), "Mount point not found");

    // ---- 1. The five defaulted keys: an explicit `null` is a 400, and the
    //         SAME key absent is the 404. A serde collapse makes the first
    //         answer the second.
    for key in [
        "dryRun",
        "direction",
        "prefer",
        "propagateDeletes",
        "useManifest",
    ] {
        let (status, v) = post(body(json!({ key: Value::Null }))).await;
        assert_eq!(
            status, 400,
            "`{key}: null` reached the store lookup — serde collapsed the explicit \
             null into an absent key and the schema took its default: {v}"
        );
        assert!(
            sentence(&v).starts_with("Invalid sync request: "),
            "`{key}: null` answered the wrong sentence: {}",
            sentence(&v)
        );
        assert!(
            sentence(&v).contains(key),
            "`{key}: null` refused without naming the key: {}",
            sentence(&v)
        );

        // …and the control: the same body WITHOUT the key is the 404.
        let (status, _) = post(body(json!({}))).await;
        assert_eq!(status, 404, "the control body stopped reaching the lookup");
    }

    // ---- 2. `targetPath` — REQUIRED, so absent and `null` are both 400s, but
    //         with DIFFERENT sentences. That difference is the tri-state's whole
    //         visible effect on this key, and an earlier draft of the handler
    //         lost it by flattening `Some(None)` onto `None` (the route family
    //         caught it; this arm is the wire-side pin).
    let mut without = body(json!({}));
    without.as_object_mut().unwrap().remove("targetPath");
    let (status, absent_v) = post(without).await;
    assert_eq!(status, 400);
    assert_eq!(
        sentence(&absent_v),
        "Invalid sync request: targetPath Invalid input: expected string, received undefined"
    );

    let (status, null_v) = post(body(json!({ "targetPath": Value::Null }))).await;
    assert_eq!(status, 400);
    assert_eq!(
        sentence(&null_v),
        "Invalid sync request: targetPath Invalid input: expected string, received null"
    );
    assert_ne!(
        sentence(&absent_v),
        sentence(&null_v),
        "absent and explicit-null answer the same sentence — the tri-state is gone"
    );

    // ---- 3. Wrong types reach the HANDLER's sentence, not serde's. A typed
    //         field on the variant would answer `Invalid request: <serde text>`
    //         here and the Tauri transport would say the same, which is why the
    //         refusal lives in the handler.
    for (key, wrong, expected) in [
        (
            "dryRun",
            json!("yes"),
            "Invalid sync request: dryRun Invalid input: expected boolean, received string",
        ),
        (
            "direction",
            json!("sideways"),
            "Invalid sync request: direction Invalid option: expected one of \"both\"|\"to-disk\"|\"to-store\"",
        ),
        (
            "prefer",
            json!(7),
            "Invalid sync request: prefer Invalid option: expected one of \"newer\"|\"store\"|\"disk\"",
        ),
        (
            "useManifest",
            json!(1),
            "Invalid sync request: useManifest Invalid input: expected boolean, received number",
        ),
        (
            "targetPath",
            json!(42),
            "Invalid sync request: targetPath Invalid input: expected string, received number",
        ),
        (
            "targetPath",
            json!(""),
            "Invalid sync request: targetPath Too small: expected string to have >=1 characters",
        ),
    ] {
        let (status, v) = post(body(json!({ key: wrong }))).await;
        assert_eq!(status, 400, "`{key}` wrong type: {v}");
        assert_eq!(sentence(&v), expected, "`{key}` wrong type");
    }

    // ---- 4. Several issues at once list ALL of them, in the SCHEMA's key
    //         order, joined by '; '.
    let (status, v) = post(body(json!({
        "targetPath": "",
        "direction": "sideways",
        "prefer": "whichever",
    })))
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        sentence(&v),
        "Invalid sync request: targetPath Too small: expected string to have >=1 characters; \
         direction Invalid option: expected one of \"both\"|\"to-disk\"|\"to-store\"; \
         prefer Invalid option: expected one of \"newer\"|\"store\"|\"disk\""
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_rest_arm_serves_the_same_verb_over_v4s_own_url() {
    let base = common::materialize_fixture_instance();
    let (addr, _state) = common::serve_instance(base.path(), |mut c| {
        c.terminal = false;
        c
    })
    .await;
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/v1/mount-points/{ABSENT_STORE}?action=sync");

    // The CLI posts THIS url, so the REST edge must reach the same handler.
    let r = client
        .post(&url)
        .json(&json!({ "targetPath": "/tmp/qt-sync-wire-target" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 404);
    let v: Value = r.json().await.unwrap();
    assert_eq!(sentence(&v), "Mount point not found");

    // The schema refusal, over the same edge.
    let r = client
        .post(&url)
        .json(&json!({ "dryRun": null, "targetPath": "/tmp/x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 400);
    let v: Value = r.json().await.unwrap();
    assert!(sentence(&v).contains("dryRun"), "{v}");

    // A body that is valid JSON but not an OBJECT: v4's Zod issue has an empty
    // path, so the sentence carries a double space where the field name would
    // be. This arm is the REST edge's alone — over dispatch a non-object body
    // cannot carry the `type` tag, so the verb is never selected.
    for (payload, word) in [("[]", "array"), ("null", "null"), ("42", "number")] {
        let r = client
            .post(&url)
            .header("content-type", "application/json")
            .body(payload)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), 400, "{payload}");
        let v: Value = r.json().await.unwrap();
        assert_eq!(
            sentence(&v),
            format!("Invalid sync request:  Invalid input: expected object, received {word}"),
            "{payload}"
        );
    }

    // UNPARSEABLE JSON is a DIFFERENT arm: v4's `req.json().catch(() => ({}))`
    // makes it an empty object, which fails on the required `targetPath`.
    let r = client
        .post(&url)
        .header("content-type", "application/json")
        .body("{ not json")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 400);
    let v: Value = r.json().await.unwrap();
    assert_eq!(
        sentence(&v),
        "Invalid sync request: targetPath Invalid input: expected string, received undefined"
    );

    // And `sync` is now one of the actions the edge's own envelope advertises,
    // in v4's literal key order (`Object.keys(actions)`), between `deconvert`
    // and `move-file`.
    let r = client
        .post(format!(
            "http://{addr}/api/v1/mount-points/{ABSENT_STORE}?action=wibble"
        ))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    let v: Value = r.json().await.unwrap();
    let rendered = v.to_string();
    assert!(
        rendered.contains("sync"),
        "the unknown-action envelope omits `sync`: {v}"
    );
    assert!(
        rendered.find("deconvert").unwrap() < rendered.find("\"sync\"").unwrap(),
        "`sync` is not in v4's position in `availableActions`: {v}"
    );
}
