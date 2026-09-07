//! P4.9K1 unit 5 — the four generator actions on `POST /api/v1/characters/
//! {id}?action=…` (v4 `[id]/handlers/post.ts`): `rename` / `refresh-archive`
//! / `generate-external-prompt` as JSON and `optimize-stream` as v4's
//! `text/event-stream`, over the host's LIVE assembly (the spine's driver, the
//! production providers) on the committed `character-generators` pair.
//!
//! This is a web-edge test on the `characters_action_route` pattern: the
//! handlers are differential-proven (`character_rename_equivalence`,
//! `external_prompt_tier3_equivalence`, `character_optimizer_tier3_
//! equivalence`); what needs proving here is the PLUMBING — the URL resolves,
//! the body's tri-state reaches v4's Zod arms through the `Request` enum, the
//! 404 wins over a malformed body, v4's envelopes come back, the host driver
//! is actually WIRED (a model-calling arm reaches the provider rather than
//! the not-assembled refusal), and the optimizer's frames come back as v4's
//! SSE bytes through the K0 re-framer.
//!
//! No spend: the fixture's connection profile points at `http://127.0.0.1:1`
//! (nothing listens), so every model call fails at the socket — the
//! external prompt answers v4's 500 with the transport's message, and the
//! optimizer's stream ends in an `error` frame after its `loading` step.
//!
//! Run:
//!   cargo test -p quilltap-web --test characters_generators_routes

mod common;

use serde_json::{json, Value};

/// Mira — the rich vaulted character of the `character-generators` pair.
const MIRA: &str = "a2000002-0000-4000-8000-000000000001";
/// The OPENAI_COMPATIBLE profile at `http://127.0.0.1:1/v1`.
const PROFILE: &str = "c0000002-0000-4000-8000-000000000001";
/// Mira's default system prompt.
const DEFAULT_PROMPT: &str = "41c13f30-6ddb-8763-9fc8-eec33620c6ac";
const MISSING: &str = "00000000-0000-4000-8000-00000000dead";

fn parse_frames(sse: &str) -> Vec<Value> {
    sse.split("\n\n")
        .filter_map(|chunk| chunk.strip_prefix("data: "))
        .map(|payload| serde_json::from_str(payload).expect("a JSON frame"))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_generator_actions_resolve_over_the_live_assembly() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("quilltap=warn")
        .try_init();
    let base = common::materialize_generators_instance();
    // The PRODUCTION spine, as every deployment shell boots it — the driver
    // under test is assembled from its bundle (a spine-less host answers the
    // named refusal, which is what this test must be able to tell apart).
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
    let url =
        |id: &str, action: &str| format!("http://{addr}/api/v1/characters/{id}?action={action}");

    // 1. An action this edge does not serve names every served one and the
    //    dispatch channel.
    let resp = client.post(url(MIRA, "favorite")).send().await.unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    let err = body["error"].as_str().unwrap();
    for served in [
        "?action=archive",
        "?action=rehydrate",
        "?action=rename",
        "?action=refresh-archive",
        "?action=generate-external-prompt",
        "?action=optimize-stream",
    ] {
        assert!(err.contains(served), "{err}");
    }

    // 2. rename — a dry run answers v4's RenamePreviewResponse raw.
    let resp = client
        .post(url(MIRA, "rename"))
        .json(&json!({"primaryRename": {"oldValue": "Mira", "newValue": "Mora"}}))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "rename failed: {body}");
    assert_eq!(body["characterId"], json!(MIRA));
    assert_eq!(body["dryRun"], json!(true));
    assert!(
        body["summary"]["total"].as_i64().unwrap() > 0,
        "Mira's fields carry her name: {body}"
    );

    // 3. rename — the tri-state reaches v4's Zod arm THROUGH the edge: a
    //    `null` primaryRename is `invalid_type` (an absent one is fine).
    let resp = client
        .post(url(MIRA, "rename"))
        .json(&json!({"primaryRename": null}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Validation error"));
    assert_eq!(body["details"][0]["expected"], json!("object"), "{body}");
    assert_eq!(
        body["details"][0]["path"],
        json!(["primaryRename"]),
        "{body}"
    );

    // 4. rename — no replacement at all is v4's route-level 400.
    let resp = client
        .post(url(MIRA, "rename"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(
        body["error"],
        json!("At least one replacement must be specified")
    );

    // 5. A body that is not JSON: v4's `req.json()` throws into the generic
    //    500 — but only for a character that EXISTS; a missing one is 404
    //    first (v4 resolves the character before any handler reads a body).
    let resp = client
        .post(url(MIRA, "rename"))
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 500);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Internal server error"));
    let resp = client
        .post(url(MISSING, "rename"))
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Character not found"));
    // A JSON non-object reaches the schema as the root-level issue.
    let resp = client
        .post(url(MIRA, "rename"))
        .body("[1]")
        .header("content-type", "application/json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["details"][0]["path"], json!([]), "{body}");
    assert_eq!(
        body["details"][0]["message"],
        json!("Invalid input: expected object, received array"),
        "{body}"
    );

    // 6. refresh-archive — Mira sits in no chat: `{queued: 0}`.
    let resp = client
        .post(url(MIRA, "refresh-archive"))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 200, "refresh-archive failed: {body}");
    assert_eq!(body, json!({"queued": 0}));

    // 7. generate-external-prompt — the Zod arm through the edge…
    let resp = client
        .post(url(MIRA, "generate-external-prompt"))
        .json(&json!({"maxTokens": "no"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Validation error"));
    assert!(body["details"].as_array().unwrap().len() >= 3, "{body}");
    //    …and the DRIVER IS WIRED: a valid request reaches the provider (the
    //    socket refuses) and answers v4's 500 with the transport's message —
    //    NOT the engine's not-assembled 503.
    let resp = client
        .post(url(MIRA, "generate-external-prompt"))
        .json(&json!({
            "connectionProfileId": PROFILE,
            "systemPromptId": DEFAULT_PROMPT,
            "maxTokens": 1000,
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(status, 500, "expected the transport failure: {body}");
    let err = body["error"].as_str().unwrap();
    assert!(
        !err.contains("GeneratorsDetailDriver"),
        "the host driver is not wired: {err}"
    );

    // 8. optimize-stream — a refusal before the first frame answers JSON…
    let resp = client
        .post(url(MIRA, "optimize-stream"))
        .json(&json!({"connectionProfileId": "not-a-uuid"}))
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
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], json!("Validation error"));
    assert_eq!(body["details"][0]["path"], json!(["connectionProfileId"]));
    let resp = client
        .post(url(MISSING, "optimize-stream"))
        .json(&json!({"connectionProfileId": PROFILE}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    //    …and a run streams v4's bytes: the frames the runner emitted, in
    //    order, until the provider refusal ends the run with an `error`.
    let resp = client
        .post(url(MIRA, "optimize-stream"))
        .json(&json!({"connectionProfileId": PROFILE}))
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
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap(),
        "no-cache"
    );
    let text = resp.text().await.unwrap();
    let frames = parse_frames(&text);
    assert_eq!(frames.first(), Some(&json!({"type": "start"})), "{text}");
    assert!(
        frames.contains(&json!({"type": "step_start", "step": "loading"})),
        "{text}"
    );
    let loading = frames
        .iter()
        .find(|f| f["type"] == json!("step_complete") && f["step"] == json!("loading"))
        .unwrap_or_else(|| panic!("no loading step_complete: {text}"));
    // Mira's eight reinforced about-self memories, through the real pipeline.
    assert_eq!(loading["memoryCount"], json!(8), "{loading}");
    assert!(
        frames.contains(&json!({"type": "step_start", "step": "analyzing"})),
        "{text}"
    );
    let last = frames.last().unwrap();
    assert_eq!(last["type"], json!("error"), "{text}");
    assert!(
        !last["error"]
            .as_str()
            .unwrap()
            .contains("GeneratorsDetailDriver"),
        "the host driver is not wired: {last}"
    );
}
