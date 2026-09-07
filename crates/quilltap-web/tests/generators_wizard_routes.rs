//! P4.9K2 unit 6 — the creation-pair generator actions on the wire: `ai-wizard`
//! / `ai-wizard-stream` on `POST /api/v1/characters` (v4 `handlers/post.ts`)
//! and `ai-import-stream` on `POST /api/v1/system/tools` (v4 `route.ts`), over
//! the host's LIVE assembly (the spine's driver, the production providers) on
//! the committed `character-generators` pair.
//!
//! The handlers are differential-proven (`character_wizard_tier3_equivalence`,
//! `ai_import_tier3_equivalence`); this proves the PLUMBING — the URLs resolve,
//! the body reaches v4's Zod / hand-rolled arms, the envelopes come back, the
//! host driver is WIRED (a model-calling arm reaches the provider rather than
//! the not-assembled refusal), and the streams come back as v4's SSE bytes.
//!
//! No spend: the fixture's connection profile points at `http://127.0.0.1:1`
//! (nothing listens), so every model call fails at the socket — the wizard
//! contains it as a `field_error`, the import as the fatal basics failure.
//!
//! Run:
//!   cargo test -p quilltap-web --test generators_wizard_routes

mod common;

use serde_json::{json, Value};

/// The OPENAI_COMPATIBLE profile at `http://127.0.0.1:1/v1`.
const PROFILE: &str = "c0000002-0000-4000-8000-000000000001";
const MISSING: &str = "00000000-0000-4000-8000-00000000dead";

fn parse_frames(sse: &str) -> Vec<Value> {
    sse.split("\n\n")
        .filter_map(|chunk| chunk.strip_prefix("data: "))
        .map(|payload| serde_json::from_str(payload).expect("a JSON frame"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_creation_pair_actions_resolve_over_the_live_assembly() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("quilltap=warn")
        .try_init();
    let base = common::materialize_generators_instance();
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
    let chars = |action: &str| format!("http://{addr}/api/v1/characters?action={action}");
    let tools = format!("http://{addr}/api/v1/system/tools?action=ai-import-stream");

    // 1. The collection POST's unknown-action sentence names the two arms.
    let resp = client.post(chars("quick-create")).send().await.unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    let err = body["error"].as_str().unwrap();
    assert!(
        err.contains("?action=ai-wizard") && err.contains("?action=ai-wizard-stream"),
        "{err}"
    );

    // 2. ai-wizard — v4's Zod arm through the edge, the root-level issue, the
    //    non-JSON 500.
    let resp = client
        .post(chars("ai-wizard"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Validation error"));
    assert_eq!(
        body["details"][0]["path"],
        json!(["primaryProfileId"]),
        "{body}"
    );
    let resp = client
        .post(chars("ai-wizard"))
        .header("content-type", "application/json")
        .body("[1]")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["details"][0]["message"],
        json!("Invalid input: expected object, received array"),
        "{body}"
    );
    let resp = client
        .post(chars("ai-wizard"))
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Internal server error"));

    // 3. ai-wizard — the DRIVER IS WIRED: the one field's call reaches the
    //    socket-refused provider and is contained as v4's `errors.title`
    //    (a spine-less host would have answered the not-assembled 503).
    let wizard_body = json!({
        "primaryProfileId": PROFILE,
        "sourceType": "skip",
        "characterName": "Mira",
        "background": "A harbor town.",
        "fieldsToGenerate": ["title"],
    });
    let resp = client
        .post(chars("ai-wizard"))
        .json(&wizard_body)
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "wizard failed: {body}");
    assert_eq!(body["success"], json!(true));
    assert_eq!(body["generated"], json!({}));
    let title_err = body["errors"]["title"].as_str().unwrap_or("");
    assert!(
        !title_err.is_empty() && !title_err.contains("GeneratorsWizardDriver"),
        "the host driver is not wired: {body}"
    );

    // 4. ai-wizard-stream — v4's bytes: start, the field's start + error, done.
    let resp = client
        .post(chars("ai-wizard-stream"))
        .json(&wizard_body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "text/event-stream"
    );
    let text = resp.text().await.unwrap();
    let frames = parse_frames(&text);
    assert_eq!(frames.first(), Some(&json!({"type": "start"})), "{text}");
    assert!(
        frames.contains(&json!({"type": "field_start", "field": "title"})),
        "{text}"
    );
    let err_frame = frames
        .iter()
        .find(|f| f["type"] == json!("field_error"))
        .unwrap_or_else(|| panic!("no field_error: {text}"));
    assert_eq!(err_frame["field"], json!("title"));
    let done = frames.last().unwrap();
    assert_eq!(done["type"], json!("done"), "{text}");
    assert_eq!(done["fullContent"], json!({}));
    assert!(done["errors"]["title"].is_string(), "{done}");
    //    …and a throw BEFORE the field loop is a done frame, not a status.
    let resp = client
        .post(chars("ai-wizard-stream"))
        .json(&json!({
            "primaryProfileId": MISSING,
            "sourceType": "skip",
            "characterName": "Mira",
            "background": "",
            "fieldsToGenerate": ["title"],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let frames = parse_frames(&resp.text().await.unwrap());
    assert_eq!(
        frames,
        vec![
            json!({"type": "start"}),
            json!({"type": "done", "error": "Primary profile not found", "fullContent": {}, "errors": {"_fatal": "Primary profile not found"}}),
        ]
    );
    //    A Zod refusal on the streaming arm is JSON with its status.
    let resp = client
        .post(chars("ai-wizard-stream"))
        .json(&json!({"sourceType": "skip"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    assert!(resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("application/json"));

    // 5. ai-import-stream — the two 400s, the non-JSON 500 with V8's wording,
    //    then a run streamed to its fatal basics failure.
    let resp = client.post(&tools).json(&json!({})).send().await.unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Missing required field: profileId"));
    let resp = client
        .post(&tools)
        .json(&json!({"profileId": PROFILE, "sourceText": "   "}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        json!("Must provide at least one source file or source text")
    );
    let resp = client.post(&tools).body("not json").send().await.unwrap();
    assert_eq!(resp.status(), 500);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        json!("Unexpected token 'o', \"not json\" is not valid JSON")
    );
    // v4's `body.profileId` over a JSON `null` throws inside the handler —
    // the middleware's 500 carries V8's property-read sentence; a non-object
    // body reads `undefined` for every key and falls into the first 400.
    let resp = client.post(&tools).body("null").send().await.unwrap();
    assert_eq!(resp.status(), 500);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        json!("Cannot read properties of null (reading 'profileId')")
    );
    let resp = client.post(&tools).body("[1, 2]").send().await.unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Missing required field: profileId"));
    let resp = client
        .post(&tools)
        .json(&json!({"profileId": PROFILE, "sourceText": "Mira keeps the lanterns."}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "text/event-stream"
    );
    let text = resp.text().await.unwrap();
    let frames = parse_frames(&text);
    assert_eq!(frames.first(), Some(&json!({"type": "start"})), "{text}");
    assert!(
        frames.contains(&json!({"type": "step_start", "step": "character_basics"})),
        "{text}"
    );
    let step_err = frames
        .iter()
        .find(|f| f["type"] == json!("step_error"))
        .unwrap_or_else(|| panic!("no step_error: {text}"));
    assert_eq!(step_err["step"], json!("character_basics"));
    assert!(
        !step_err["error"]
            .as_str()
            .unwrap()
            .contains("GeneratorsWizardDriver"),
        "the host driver is not wired: {step_err}"
    );
    let done = frames.last().unwrap();
    assert_eq!(done["type"], json!("done"), "{text}");
    assert_eq!(
        done["error"],
        json!("Failed to generate character basics — cannot proceed without a character name")
    );
}
