//! P4.98 — the images-COLLECTION generate verb's five body keys AT THE
//! DISPATCH WIRE (`POST /api/v1/images?action=generate`'s sibling transport).
//!
//! The P4.96 idiom, applied to the one remaining live instance of the
//! `a-plain-option-value-eats-an-explicit-null` class. `Request::Images
//! Generate`'s `prompt` / `profileId` / `chatId` / `tags` / `options` were
//! plain `#[serde(default)] Option<Value>`: serde collapses an explicit JSON
//! `null` into `None`, so `{"chatId": null}` arrived at the handler
//! indistinguishable from an ABSENT `chatId` and SAILED THROUGH a gate v4
//! 400s — v4's `chatId: z.uuid().optional()` is `.optional()`, not
//! `.nullable()`, so a present `null` is an `invalid_type` refusal. The
//! measured live consequence: a `chatId: null` posted over dispatch generated
//! and SAVED an image v4 refuses.
//!
//! **What no other instrument sees.** `images_generate_route_equivalence`
//! calls `api::images::images_generate` DIRECTLY with `Some(&Value::Null)`, so
//! its `zod_profile_null` / `zod_options_null` / `zod_tags_null` /
//! `generate_chat_id_null` rows were green on both sides throughout — the trip
//! through serde is exactly the part a handler-direct family cannot reach
//! (`a-dispatch-wire-test-sees-what-the-differential-cannot`). The REST edge
//! (`images_routes.rs::images_generate`) hand-built the variant from
//! `map.get(..).cloned()`, so `null` always survived THERE — which is why
//! `images_edge_routes.rs`'s null arm is green in both directions and pins
//! only that the P4.98 edge rewrite did not regress it.
//!
//! **The Tauri IPC leg is proven by construction, not by a second venue.**
//! `quilltap-tauri`'s `dispatch` command is `dispatch_inner`
//! (`crates/quilltap-tauri/src/commands.rs:19-26`), which serializes the
//! request `Value` and calls `quilltap_web::dispatch::dispatch_body` — the
//! SAME function this file drives over HTTP, and therefore the same
//! `serde_json::from_slice::<Request>` at `dispatch.rs:69`. There is exactly
//! one decoder for both transports, so an assertion here holds there; v5 has
//! no Tauri test venue and inventing one was out of P4.98's scope.
//!
//! **No spine factory, and no spend.** Unlike the profile-generate verb,
//! `ready_images_generate()` is satisfied by the venue's ordinary host
//! assembly (`quilltap-host/src/host.rs:897` wires the seams
//! unconditionally), which is why `images_edge_routes.rs`'s empty-body arm
//! already reaches v4's Zod parse. And this route parses the body FIRST and
//! reads the connection profile SECOND (`route.ts:191` then `:194` —
//! the opposite order from the profile-id route, whose 404 gate comes first),
//! so a VALID-but-absent `profileId` is a complete discriminator: the body
//! that satisfies the parse stage answers `Connection profile not found`
//! rather than `Validation error`, and the provider is never constructed. The
//! order's prescribed "seed a profile at a dead socket" shape is unnecessary
//! here for that reason, and a dead socket would be the weaker pin: an exact
//! second sentence discriminates better than "not a 400".
//!
//! **This route carries no `details`.** v4's `validationError` renders
//! `{error, details: zodError.issues}`, but v5 answers the sentence alone on
//! this route — the standing project-wide deferral recorded in
//! `images_generate_route_equivalence`'s `drop_zod_details`. So the dispatch
//! envelope here is the typed one ALONE (`dispatch_body` merges v4's flat
//! `error` key only for a refusal carrying `CoreError::details`), and the
//! two REQUIRED keys' null arms are green in both directions: absent and
//! present-null both answer `Validation error` on v5, there being no
//! `details` array in which the two could differ.
//!
//! v4 DOES differ, and the difference was measured rather than assumed — the
//! `bcd7e4852`-pinned oracle for `images_generate_route_equivalence` records,
//! for this exact route:
//!
//! ```text
//!   zod_prompt_missing   Invalid input: expected string, received undefined
//!   zod_prompt_null      Invalid input: expected string, received null
//!   zod_profile_missing  Invalid input: expected string, received undefined
//!   zod_profile_null     Invalid input: expected string, received null
//! ```
//!
//! So the tri-state is load-bearing for all five on v4's side; v5's `details`
//! deferral is the only reason two of them cannot yet show it. Carrying the
//! tri-state on those two anyway is what makes the day that deferral is lifted
//! a rendering change and not another variant change — and the arms below pin
//! the class meanwhile.
//!
//! Run:
//!   cargo test -p quilltap-web --test images_generate_dispatch_wire

mod common;

use serde_json::{json, Value};

/// `images-collection.json`'s `PROFILE_MAIN` — the fixture's real connection
/// profile, NEVER posted here: a body that names it would get past the profile
/// read and construct a real image provider. Recorded so the next reader knows
/// the omission is deliberate.
#[allow(dead_code)]
const PROFILE_MAIN: &str = "aaaa0000-0000-4000-8000-000000000001";

/// A well-formed uuid that names no row — the discriminator (see the header).
const ABSENT_PROFILE: &str = "aaaa0000-0000-4000-8000-0000000000ff";

/// A well-formed uuid for `chatId`'s value arm.
const SOME_CHAT: &str = "cccc0000-0000-4000-8000-000000000001";

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

/// The dispatch transport answers the tagged envelope
/// `{"type":"error","data":{kind,message}}`, and merges v4's flat `{error,
/// details}` alongside it ONLY for a refusal carrying `CoreError::details`
/// (`dispatch.rs`'s `validation_wire_body` merge). This route carries none,
/// so every sentence here is read from the typed envelope — the flat reader
/// is kept first so this helper stays correct if the `details` deferral is
/// ever lifted.
fn sentence(v: &Value) -> &str {
    v.get("error")
        .and_then(Value::as_str)
        .or_else(|| v.pointer("/data/message").and_then(Value::as_str))
        .unwrap_or_default()
}

/// The base body: a prompt v4's `z.string().min(1).max(4000)` accepts and a
/// `profileId` its `z.uuid()` accepts. Every arm overlays ONE key onto it, so
/// each refusal names exactly the key under test.
fn body(extra: Value) -> Value {
    let mut v = json!({
        "type": "imagesGenerate",
        "prompt": "A brass observatory at dusk",
        "profileId": ABSENT_PROFILE,
    });
    let o = v.as_object_mut().unwrap();
    for (k, val) in extra.as_object().unwrap() {
        o.insert(k.clone(), val.clone());
    }
    v
}

#[tokio::test(flavor = "multi_thread")]
async fn the_five_body_keys_are_refused_by_the_handler_not_the_decode() {
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

    // ---- 0. The DISCRIMINATOR, asserted first so every arm below is read
    //         against a known-reachable parse stage. A body carrying all five
    //         keys well-formed satisfies `generateImageSchema` and dies at the
    //         profile read with v4's OTHER 400 sentence. If this arm ever
    //         answers `Validation error`, or the not-assembled refusal, the
    //         rest of this file is measuring the gate rather than the keys.
    let (status, v) = post(body(json!({
        "chatId": SOME_CHAT,
        "tags": [{ "tagType": "THEME", "tagId": "brass" }],
        "options": { "n": 1, "size": "1024x1024", "quality": "high", "style": "vivid" },
    })))
    .await;
    assert_eq!(
        (status, sentence(&v)),
        (400, "Connection profile not found"),
        "the five well-formed keys must cross serde intact and satisfy v4's parse \
         stage, leaving the profile read to refuse: {v}"
    );

    // ---- 1. The five NULL arms. `.optional()` is not `.nullable()`, so each
    //         is v4's `Validation error` — and a plain `Option<Value>` on the
    //         variant cannot tell any of them from an absent key.
    //
    //         RED-FIRST (measured on unported `main`, this file's first run):
    //         `chatId` / `tags` / `options` answered `Connection profile not
    //         found` — the null had collapsed to ABSENT, the parse stage
    //         PASSED, and on a body naming a REAL profile v5 would have
    //         generated and saved an image v4 refuses. `prompt` / `profileId`
    //         were green pre-fix: they are REQUIRED, so absent and
    //         present-null both refuse, and this route carries no `details`
    //         array in which `received undefined` and `received null` could
    //         differ (see the header). They pin the class regardless.
    for key in ["prompt", "profileId", "chatId", "tags", "options"] {
        let (status, v) = post(body(json!({ key: Value::Null }))).await;
        assert_eq!(
            (status, sentence(&v)),
            (400, "Validation error"),
            "`{key}: null` must reach the handler as `Value::Null` and be refused \
             the way v4's `.optional()` refuses it — not collapse to ABSENT: {v}"
        );
    }

    // ---- 2. The five WRONG-TYPE arms. These are already raw and green; they
    //         pin the class against a future re-typing. A serde-TYPED field
    //         would refuse at the DECODE and the wire would carry serde's
    //         `invalid type: …` where v4 carries its own sentence.
    for (key, wrong) in [
        ("prompt", json!(42)),
        ("profileId", json!(7)),
        ("chatId", json!([])),
        ("tags", json!("nope")),
        ("options", json!(7)),
    ] {
        let (status, v) = post(body(json!({ key: wrong }))).await;
        assert_eq!(
            (status, sentence(&v)),
            (400, "Validation error"),
            "`{key}` must answer v4's sentence, not serde's: {v}"
        );
        let rendered = serde_json::to_string(&v).unwrap();
        assert!(
            !rendered.contains("invalid type"),
            "serde's decode sentence leaked to the wire — `{key}` is typed again: {rendered}"
        );
    }

    // ---- 3. The ABSENT control for the three optional keys. Absent must stay
    //         absent: this is the half of the tri-state a naive fix (mapping
    //         `Some(None)` and `None` to the same thing) would break, and it
    //         is what M2's mutation reddens from the other side.
    let (status, v) = post(body(json!({}))).await;
    assert_eq!(
        (status, sentence(&v)),
        (400, "Connection profile not found"),
        "an ABSENT chatId/tags/options must still satisfy v4's `.optional()` and \
         reach the profile read: {v}"
    );
}
