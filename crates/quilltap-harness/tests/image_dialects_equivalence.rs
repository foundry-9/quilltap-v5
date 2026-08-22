//! Differential (W4.7f): the five image-generation wire dialects
//! (`quilltap_core::model::image_dialects`) vs v4's REAL image-provider plugins.
//!
//! For every committed row, this reconstructs the [`ImageGenParams`] from the
//! recorded `input`, runs the Rust `build_image_request` (diffing method / url /
//! body bytes against what the plugin/SDK actually sent), and — for `mode:'wire'`
//! rows — runs `parse_image_response` over the recorded wire `{status, body}`,
//! diffing the parsed `ImageGenResponse` OR the exact thrown string. For
//! `mode:'sdkThrow'` rows (an SDK converting a non-2xx to a throw) the message is
//! the recorded SDK error; the Rust side only replays it. Every rejection row's
//! `is_image_moderation_error` verdict is checked against the recorded one
//! (proving the keyword matrix incl. the three documented GAPs — gemini refusal,
//! openrouter "declined", z-ai generic).
//!
//! The fixture is committed (no env var); regenerate with
//! `harness/oracle/providers/regenerate-image-fixtures.sh`.
//!
//! Run:
//!   cargo test -p quilltap-harness --test image_dialects_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::image_gen::{OrientationMapping, OrientationStrategy, OrientationSupport};
use quilltap_core::image_gen_data::orientation_data_for;
use quilltap_core::model::image::{ImageGenParams, ImageGenResponse};
use quilltap_core::model::image_dialects::{build_image_request, parse_image_response};
use quilltap_core::model::wire::WireResponse;
use quilltap_core::services::dangerous_content::provider_routing::is_image_moderation_error;
use serde_json::{Map, Value};

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/image-dialects/image-dialects.recorded.ndjson")
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn params_from_json(input: &Value) -> ImageGenParams {
    ImageGenParams {
        prompt: input
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        negative_prompt: opt_str(input, "negativePrompt"),
        model: input
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        n: input.get("n").and_then(Value::as_i64),
        size: opt_str(input, "size"),
        aspect_ratio: opt_str(input, "aspectRatio"),
        quality: opt_str(input, "quality"),
        style: opt_str(input, "style"),
        seed: input.get("seed").and_then(Value::as_i64),
        guidance_scale: input.get("guidanceScale").and_then(Value::as_f64),
        steps: input.get("steps").and_then(Value::as_i64),
    }
}

/// Project an [`OrientationMapping`] to v4's JSON shape (key order irrelevant —
/// `serde_json::Value` equality is order-insensitive). Empty mapping → `{}`.
fn mapping_json(m: &OrientationMapping) -> Value {
    let mut o = Map::new();
    if let Some(s) = &m.size {
        o.insert("size".into(), Value::String(s.clone()));
    }
    if let Some(a) = &m.aspect_ratio {
        o.insert("aspectRatio".into(), Value::String(a.clone()));
    }
    if let Some(h) = &m.prompt_hint {
        o.insert("promptHint".into(), Value::String(h.clone()));
    }
    // Nominal dims are all integer-valued in the real data; v4 dumps `1024`.
    if let Some(w) = m.nominal_width {
        o.insert("nominalWidth".into(), Value::from(w as i64));
    }
    if let Some(h) = m.nominal_height {
        o.insert("nominalHeight".into(), Value::from(h as i64));
    }
    Value::Object(o)
}

fn support_json(s: &OrientationSupport) -> Value {
    let strategy = match s.strategy {
        OrientationStrategy::Size => "size",
        OrientationStrategy::AspectRatio => "aspectRatio",
        OrientationStrategy::Prompt => "prompt",
    };
    let mut o = Map::new();
    o.insert("strategy".into(), Value::String(strategy.into()));
    o.insert("portrait".into(), mapping_json(&s.portrait));
    o.insert("landscape".into(), mapping_json(&s.landscape));
    if let Some(sq) = &s.square {
        o.insert("square".into(), mapping_json(sq));
    }
    Value::Object(o)
}

/// Project an image to the recorded `{data, url, mimeType, revisedPrompt}` shape.
fn project(resp: &ImageGenResponse) -> Value {
    Value::Array(
        resp.images
            .iter()
            .map(|img| {
                serde_json::json!({
                    "data": img.data,
                    "url": img.url,
                    "mimeType": img.mime_type,
                    "revisedPrompt": img.revised_prompt,
                })
            })
            .collect(),
    )
}

#[test]
fn image_dialects_match_v4() {
    let text = std::fs::read_to_string(corpus_path()).expect("committed image-dialects NDJSON");
    let mut rows = 0usize;
    let mut providers = std::collections::HashSet::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line).unwrap();
        let provider = row["provider"]
            .as_str()
            .unwrap()
            .to_uppercase()
            .replace('-', "_");
        providers.insert(provider.clone());

        // Orientation rows: verify the `orientation_data_for` transcription against
        // v4's real `getImageGenerationModels` / `getImageProviderConstraints`.
        if row["kind"].as_str() == Some("orientation") {
            let (models, constraint) = orientation_data_for(&provider);
            let want_models = &row["models"];
            let got_models: Value = Value::Array(
                models
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "id": m.id,
                            "orientationSupport": m.orientation_support.as_ref().map(support_json),
                        })
                    })
                    .collect(),
            );
            assert_eq!(&got_models, want_models, "{provider} orientation models");
            let got_constraint = constraint.as_ref().map(support_json);
            let want_constraint = &row["providerConstraint"];
            assert_eq!(
                got_constraint.unwrap_or(Value::Null),
                *want_constraint,
                "{provider} provider constraint"
            );
            continue;
        }

        let case = row["case"].as_str().unwrap();
        let model = row["model"].as_str().unwrap();
        let label = format!("{provider}/{case}");
        rows += 1;

        // 1. Request bytes (method / url / body).
        let params = params_from_json(&row["input"]);
        let built = build_image_request(&provider, &params)
            .unwrap_or_else(|e| panic!("{label}: build failed: {e}"));
        let req = &row["request"];
        assert_eq!(
            built.method,
            req["method"].as_str().unwrap(),
            "{label} method"
        );
        assert_eq!(built.url, req["url"].as_str().unwrap(), "{label} url");
        assert_eq!(
            built.body_string(),
            req["body"].as_str().unwrap(),
            "{label} body"
        );

        let mode = row["mode"].as_str().unwrap();
        let is_moderation = row["isModeration"].as_bool().unwrap();

        if mode == "sdk_throw" || mode == "sdkThrow" {
            // The SDK message is recorded; the Rust side only replays it. Verify
            // the moderation-keyword verdict over the REAL thrown string.
            let thrown = row["thrown"].as_str().unwrap();
            assert_eq!(
                is_image_moderation_error(thrown),
                is_moderation,
                "{label} moderation verdict for {thrown:?}"
            );
            continue;
        }

        // 2. Parse the recorded wire response.
        let wire = &row["wire"];
        let resp = WireResponse::new(
            wire["status"].as_u64().unwrap() as u16,
            wire["body"].as_str().unwrap().to_string(),
        );
        let parsed = parse_image_response(&provider, model, &resp);
        match row["outcome"].as_str().unwrap() {
            "ok" => {
                let got = parsed.unwrap_or_else(|e| panic!("{label}: expected ok, got err {e}"));
                assert_eq!(project(&got), row["images"], "{label} images");
            }
            "thrown" => {
                let err = parsed.expect_err(&format!("{label}: expected thrown"));
                assert_eq!(
                    err.message,
                    row["thrown"].as_str().unwrap(),
                    "{label} thrown"
                );
                assert_eq!(
                    is_image_moderation_error(&err.message),
                    is_moderation,
                    "{label} moderation verdict"
                );
            }
            other => panic!("{label}: unknown outcome {other}"),
        }
    }

    assert!(rows >= 25, "expected a substantial corpus, got {rows}");
    for p in ["OPENAI", "GOOGLE", "GROK", "OPENROUTER", "Z_AI"] {
        assert!(providers.contains(p), "corpus missing provider {p}");
    }
}
