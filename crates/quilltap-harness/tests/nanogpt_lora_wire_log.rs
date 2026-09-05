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
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::layer::SubscriberExt;

    let request = build_image_request("NANOGPT", &p).expect("build");
    let transport = CannedWireTransport::new().with_raw_response(
        wire_key(&request.method, &request.url, &request.body_string()),
        WireResponse::new(status, body),
    );
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
