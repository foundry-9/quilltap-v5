//! P4.D138 unit 5 — the bug-111 capturing pin (v4 `648d5c8aa`).
//!
//! A log-only fix is invisible to a differential: the wire bytes, the parsed
//! response and every DB row are byte-identical whether or not the line fires.
//! So the sanctioned proof is a capturing `tracing::Layer` over the REAL
//! [`RealImageProvider`] — asserting BOTH that the ERROR line fires on the
//! failure branch with v4's field set, and that the failure line stays SILENT
//! on the success branch, because a line attached to the wrong branch is
//! exactly the defect a presence-only assertion cannot catch.
//!
//! `tracing::subscriber::set_default` is thread-scoped, which is what keeps
//! this from colouring the rest of the harness (the standing note).
//!
//! Run: `cargo test -p quilltap-harness --test nanogpt_lora_wire_log`

use quilltap_core::image_gen::lora_support::ImageLoraSpec;
use quilltap_core::model::image::{ImageGenParams, ImageProvider};
use quilltap_core::model::image_dialects::{build_image_request, RealImageProvider};
use quilltap_core::model::nanogpt_catalog::{
    remember_detailed_catalog, reset_catalog_for_tests, CATALOG_TEST_LOCK,
};
use quilltap_core::model::wire::{wire_key, CannedWireTransport, WireResponse};
use quilltap_core::test_support::CaptureLayer;
use serde_json::json;

/// v4's two sentences, anchored to the first field that follows them.
///
/// `tracing` renders a static message through `record_debug` on a
/// `format_args!`, so it lands UNQUOTED and a bare `contains` on the sentence
/// would accept a corrupted one — `"… failed (muted)"` contains `"… failed"`,
/// and a mutation that renamed the message slipped through exactly that way.
/// Pinning the trailing ` context=` makes the match exact at the end.
const FAILED_MSG: &str = "NanoGPT image request failed context=";
const POSTING_MSG: &str = "Posting NanoGPT image request context=";

/// A `url`-family NanoGPT request carrying an adapter, a scale, a preset and a
/// passthrough key — so every field of the two log lines has something to say.
fn params() -> ImageGenParams {
    ImageGenParams {
        prompt: "a cat".into(),
        model: "flux-lora".into(),
        n: Some(1.0),
        size: Some("1024x1024".into()),
        loras: vec![ImageLoraSpec {
            source: "https://fal.test/w.safetensors".into(),
            scale: Some(1.2),
            ..Default::default()
        }],
        profile_parameters: Some(json!({
            "lora_preset": "anime",
            "num_inference_steps": 20
        })),
        ..Default::default()
    }
}

fn run(status: u16, body: &str) -> Vec<String> {
    run_with(params(), status, body)
}

fn run_with(p: ImageGenParams, status: u16, body: &str) -> Vec<String> {
    let request = build_image_request("NANOGPT", &p).expect("build");
    let transport = CannedWireTransport::new().with_raw_response(
        wire_key(&request.method, &request.url, &request.body_string()),
        WireResponse::new(status, body),
    );
    // No catalog answer is canned: the cold-cache GET (bug 269) misses, the
    // refresh WARN fires, and the request is posted as asked — this fixture's
    // square, catalog-less shape (R-14).
    run_on(p, transport, None)
}

/// Run one generate under a capture, holding the catalog lock (R-9) with the
/// process-global cache reset cold — or warmed with `warm` (a listing body).
fn run_on(p: ImageGenParams, transport: CannedWireTransport, warm: Option<&str>) -> Vec<String> {
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    let _lock = CATALOG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_catalog_for_tests();
    if let Some(body) = warm {
        remember_detailed_catalog(body).expect("warm listing");
    }
    let provider = RealImageProvider::new(transport);
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
    {
        let _guard = tracing::subscriber::set_default(subscriber);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = rt.block_on(provider.generate_image("NANOGPT", "sk-test", &p));
    }
    reset_catalog_for_tests();
    let out = logs.lock().unwrap().clone();
    out
}

/// The composed-body fields both lines carry, by KEY NAME only.
fn assert_composed_body_fields(line: &str) {
    assert!(
        line.contains("context=NanoGPTImageProvider.generateImage"),
        "{line}"
    );
    assert!(line.contains("model=flux-lora"), "{line}");
    assert!(line.contains("size=Some(\"1024x1024\")"), "{line}");
    // `4b0d1cb04`: the POSTED shape — no catalog, so no fitted ratio.
    assert!(line.contains("aspect_ratio=None"), "{line}");
    assert!(line.contains("n=1"), "{line}");
    assert!(line.contains("lora_dialect=Some(\"url\")"), "{line}");
    assert!(line.contains("lora_url"), "{line}");
    assert!(line.contains("lora_strength"), "{line}");
    assert!(line.contains("lora_preset"), "{line}");
    assert!(line.contains("lora_dropped=[]"), "{line}");
    assert!(
        line.contains("passthrough_keys=[\"num_inference_steps\"]"),
        "{line}"
    );
    // NEVER the values — this is what keeps a credential out of the log.
    assert!(
        !line.contains("fal.test") && !line.contains("anime"),
        "the line must carry key NAMES, not values: {line}"
    );
}

#[test]
fn nanogpt_failure_logs_the_composed_body_by_key_name() {
    let lines = run(
        400,
        r#"{"error":{"message":"try a different prompt or image"}}"#,
    );
    let line = lines
        .iter()
        .find(|l| l.contains(FAILED_MSG))
        .unwrap_or_else(|| panic!("bug-111 line missing; captured: {lines:#?}"));
    assert!(line.starts_with("ERROR "), "must be at ERROR level: {line}");
    assert_composed_body_fields(line);
    assert!(
        line.contains("try a different prompt or image"),
        "the provider's own sentence must ride the line: {line}"
    );
}

#[test]
fn nanogpt_posts_the_debug_line_on_every_request() {
    // v4 `84f33ce94` logs this BEFORE the call, on success and failure alike.
    for (status, body) in [
        (200u16, r#"{"data":[{"b64_json":"QUJD"}]}"#),
        (400, r#"{"error":{"message":"nope"}}"#),
    ] {
        let lines = run(status, body);
        let line = lines
            .iter()
            .find(|l| l.contains(POSTING_MSG))
            .unwrap_or_else(|| panic!("debug line missing for {status}; got: {lines:#?}"));
        assert!(line.starts_with("DEBUG "), "must be at DEBUG level: {line}");
        assert_composed_body_fields(line);
    }
}

#[test]
fn nanogpt_success_does_not_log_the_failure_line() {
    let lines = run(200, r#"{"data":[{"b64_json":"QUJD"}]}"#);
    assert!(
        !lines.iter().any(|l| l.contains(FAILED_MSG)),
        "the failure line must not fire on a 2xx: {lines:#?}"
    );
}

/// v4 wraps only `client.images.generate` in its try/catch and raises `Invalid
/// response from NanoGPT Images API` AFTER it, so a malformed 2xx body never
/// logs the failure line (the P4.D138 follow-up review's catch: v5 logged it).
#[test]
fn nanogpt_malformed_2xx_does_not_log_the_failure_line() {
    let lines = run(200, r#"{"nope":1}"#);
    assert!(
        !lines.iter().any(|l| l.contains(FAILED_MSG)),
        "a malformed 2xx is the invalid-response arm, outside v4's try/catch: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Posting NanoGPT image request")),
        "the debug line still posts: {lines:?}"
    );
}

/// v4 logs `params.model ?? 'hidream'` — the model it POSTS — so an absent
/// model reads `hidream` on both lines, never an empty string.
#[test]
fn nanogpt_logs_the_posted_model_default_when_absent() {
    let mut p = params();
    p.model = String::new();
    let lines = run_with(p, 400, r#"{"error":"nope"}"#);
    let failed = lines
        .iter()
        .find(|l| l.contains(FAILED_MSG))
        .expect("the failure line");
    assert!(failed.contains("model=hidream"), "{failed}");
    let debug = lines
        .iter()
        .find(|l| l.contains("Posting NanoGPT image request"))
        .expect("the debug line");
    assert!(debug.contains("model=hidream"), "{debug}");
}

// ---------------------------------------------------------------------------
// Bug 269 (`4b0d1cb04`, P4.D356): the lines carry the POSTED shape. The
// `nanogpt_image_provider_equivalence` family compares the key NAMES against
// v4's; these pin the VALUES the names carry.
// ---------------------------------------------------------------------------

const QWEN3_LISTING: &str = r#"{"data":[{"id":"qwen-image-3-uncensored",
  "capabilities":{"image_generation":true},
  "supported_parameters":{"resolutions":["1k","2k"],
    "aspect_ratio":["1:1","3:2","16:9","21:9","9:16","2:3","3:4","4:5"]}}]}"#;

fn portrait() -> ImageGenParams {
    ImageGenParams {
        prompt: "a lighthouse keeper".into(),
        model: "qwen-image-3-uncensored".into(),
        size: Some("832x1248".into()),
        ..Default::default()
    }
}

const POSTED_TIER: &str = r#"{"model":"qwen-image-3-uncensored","prompt":"a lighthouse keeper","n":1,"response_format":"b64_json","size":"1k","aspect_ratio":"2:3"}"#;
const POSTED_SQUARE: &str = r#"{"model":"qwen-image-3-uncensored","prompt":"a lighthouse keeper","n":1,"response_format":"b64_json","size":"1k","aspect_ratio":"1:1"}"#;
const GENERATIONS: &str = "https://nano-gpt.com/api/v1/images/generations";

#[test]
fn a_warm_catalog_fits_the_posted_shape_and_logs_it() {
    let transport = CannedWireTransport::new().with_response(
        "POST",
        GENERATIONS,
        POSTED_TIER,
        WireResponse::new(200, r#"{"data":[{"b64_json":"QUJD"}]}"#),
    );
    let lines = run_on(portrait(), transport, Some(QWEN3_LISTING));
    let fitted = lines
        .iter()
        .find(|l| l.contains("Fitted the requested image shape to the model"))
        .unwrap_or_else(|| panic!("the fit line is missing: {lines:#?}"));
    assert!(fitted.starts_with("DEBUG "), "{fitted}");
    assert!(
        fitted.contains("requested_size=Some(\"832x1248\")"),
        "{fitted}"
    );
    assert!(fitted.contains("requested_aspect_ratio=None"), "{fitted}");
    assert!(
        fitted.contains("size=Some(\"1k\") aspect_ratio=Some(\"2:3\")"),
        "{fitted}"
    );
    let posting = lines
        .iter()
        .find(|l| l.contains(POSTING_MSG))
        .expect("posting");
    assert!(
        posting.contains("size=Some(\"1k\") aspect_ratio=Some(\"2:3\")"),
        "{posting}"
    );
    // Warm: no refresh, no retry, no failure.
    for silent in ["Could not refresh", "retrying once as a square", FAILED_MSG] {
        assert!(
            !lines.iter().any(|l| l.contains(silent)),
            "{silent}: {lines:#?}"
        );
    }
}

#[test]
fn a_square_retry_that_fails_too_logs_the_last_posted_shape() {
    let transport = CannedWireTransport::new()
        .with_response(
            "POST",
            GENERATIONS,
            POSTED_TIER,
            WireResponse::new(503, "busy"),
        )
        .with_response(
            "POST",
            GENERATIONS,
            POSTED_SQUARE,
            WireResponse::new(503, "still busy"),
        );
    let lines = run_on(portrait(), transport, Some(QWEN3_LISTING));
    let warn = lines
        .iter()
        .find(|l| l.contains("NanoGPT turned down a non-square image; retrying once as a square"))
        .unwrap_or_else(|| panic!("the retry WARN is missing: {lines:#?}"));
    assert!(warn.starts_with("WARN "), "{warn}");
    assert!(
        warn.contains("size=Some(\"1k\") aspect_ratio=Some(\"2:3\") square_size=Some(\"1k\") square_aspect_ratio=Some(\"1:1\") error=503 busy"),
        "{warn}"
    );
    let failed = lines
        .iter()
        .find(|l| l.contains(FAILED_MSG))
        .expect("failed");
    assert!(failed.starts_with("ERROR "), "{failed}");
    assert!(
        failed.contains("size=Some(\"1k\") aspect_ratio=Some(\"1:1\")"),
        "{failed}"
    );
    assert!(failed.contains("error=503 still busy"), "{failed}");
}

#[test]
fn a_cold_catalog_that_cannot_be_fetched_posts_as_asked() {
    let transport = CannedWireTransport::new()
        .with_response(
            "GET",
            "https://nano-gpt.com/api/v1/image-models?detailed=true",
            "",
            WireResponse::new(503, "busy"),
        )
        .with_response(
            "POST",
            GENERATIONS,
            r#"{"model":"qwen-image-3-uncensored","prompt":"a lighthouse keeper","n":1,"response_format":"b64_json","size":"832x1248"}"#,
            WireResponse::new(200, r#"{"data":[{"b64_json":"QUJD"}]}"#),
        );
    let lines = run_on(portrait(), transport, None);
    let warn = lines
        .iter()
        .find(|l| {
            l.contains("Could not refresh the NanoGPT image catalog; posting the size as asked")
        })
        .unwrap_or_else(|| panic!("the refresh WARN is missing: {lines:#?}"));
    assert!(warn.starts_with("WARN "), "{warn}");
    assert!(
        warn.contains("context=NanoGPTImageProvider.catalogEntryFor model=qwen-image-3-uncensored error=NanoGPT image-model listing failed: HTTP 503"),
        "{warn}"
    );
    let posting = lines
        .iter()
        .find(|l| l.contains(POSTING_MSG))
        .expect("posting");
    assert!(
        posting.contains("size=Some(\"832x1248\") aspect_ratio=None"),
        "{posting}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("Fitted the requested")),
        "{lines:#?}"
    );
}
