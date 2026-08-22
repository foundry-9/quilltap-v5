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
use quilltap_core::model::image::{
    ImageGenParams, ImageGenResponse, ImageModelDiscovery, ImageProvider,
};
use quilltap_core::model::image_bytes::{CannedImageBytes, FetchedImageBytes};
use quilltap_core::model::image_dialects::{
    build_image_request, build_models_request, finalize_models, parse_image_response,
    parse_models_page, supported_image_models, RealImageProvider,
};
use quilltap_core::model::wire::{CannedWireTransport, WireResponse};
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
    let mut models_rows = 0usize;
    let mut models_cases: std::collections::HashSet<String> = std::collections::HashSet::new();
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

        // `ca22ec45` keyed model discovery: replay the recorded page sequence
        // through `build_models_request` / `parse_models_page` /
        // `finalize_models`, diffing the request bytes AND the built header set
        // against what v4's plugin (or its SDK) actually sent.
        if row["kind"].as_str() == Some("models") {
            check_models_row(&provider, &row, &mut models_rows, &mut models_cases);
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
        // `ca22ec45`: Z.AI's URL→base64 download happens INSIDE the provider,
        // above the pure parse — so z-ai rows are driven through the whole
        // composed `generate_image`, with the recorded download answer canned
        // per URL.
        if provider == "Z_AI" {
            check_zai_download_row(&row, &provider, model, &params, &resp, &built);
            continue;
        }
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

    // Shape, not a hand count: every provider must carry a no-key row AND at
    // least one keyed row, so a silently-shrunk regen cannot pass. The
    // named cases are the contract arms the port would otherwise lose
    // unnoticed — the asymmetric empty-result dispositions, grok's two
    // top-level key spellings, google's paging, and the openrouter SDK-strip
    // tripwire.
    assert!(
        models_rows >= 18,
        "expected the keyed model-discovery rows, got {models_rows}"
    );
    for p in ["OPENAI", "GOOGLE", "GROK", "OPENROUTER", "Z_AI"] {
        assert!(
            models_cases.contains(&format!("{p}/models_static")),
            "corpus missing the no-key discovery row for {p}"
        );
        assert!(
            models_cases.iter().any(|c| {
                c.starts_with(&format!("{p}/models_")) && !c.ends_with("/models_static")
            }),
            "corpus missing a keyed discovery row for {p}"
        );
    }
    for required in [
        "OPENAI/models_live",
        "OPENAI/models_live_empty",
        "GOOGLE/models_paged",
        "GOOGLE/models_empty",
        "GROK/models_live_models_key",
        "GROK/models_live_data_key",
        "GROK/models_empty",
        "Z_AI/models_live_union",
        "Z_AI/models_live_none_matching",
        "OPENROUTER/models_live_every_signal",
        "OPENROUTER/models_empty_page",
    ] {
        assert!(
            models_cases.contains(required),
            "corpus missing the {required} contract arm"
        );
    }
}

/// Drive a recorded z-ai `generateImage` row through the WHOLE composed
/// provider — build, wire, parse, and (when the entry carries only a `url`) the
/// `ca22ec45` image download — so the URL→base64 conversion, the content-type
/// sniff and both new error sentences are diffed against v4's real plugin.
///
/// The download seam is canned per URL from the recorded `download` block; a
/// row with NO download block registers nothing, so an unexpected download
/// attempt fails loudly rather than yielding empty bytes.
fn check_zai_download_row(
    row: &Value,
    provider: &str,
    model: &str,
    params: &ImageGenParams,
    resp: &WireResponse,
    built: &quilltap_core::model::request_builder::BuiltRequest,
) {
    let case = row["case"].as_str().unwrap();
    let label = format!("{provider}/{case}");

    // The pure parse still has to agree (the pre-download shape).
    let pure = parse_image_response(provider, model, resp);

    let mut bytes = CannedImageBytes::new();
    let recorded_downloads = row["downloadRequests"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if let Some(dl) = row.get("download").filter(|d| !d.is_null()) {
        // v4's download is a BARE `fetch(url)`: a GET with no headers at all.
        for req in &recorded_downloads {
            assert_eq!(
                req["method"].as_str().unwrap(),
                "GET",
                "{label} download method"
            );
            assert!(
                req["headers"].as_object().unwrap().is_empty(),
                "{label}: v4's image download sent headers: {}",
                req["headers"]
            );
            assert!(req["body"].is_null(), "{label} download body");
            let decoded = base64_decode(dl["bytes"].as_str().unwrap_or(""));
            bytes = bytes.with_response(
                req["url"].as_str().unwrap(),
                FetchedImageBytes {
                    status: dl["status"].as_u64().unwrap() as u16,
                    content_type: dl["contentType"].as_str().map(str::to_string),
                    bytes: decoded,
                },
            );
        }
    } else {
        assert!(
            recorded_downloads.is_empty(),
            "{label}: v4 downloaded without a scripted answer"
        );
    }

    let transport = CannedWireTransport::new().with_response(
        &built.method,
        &built.url,
        &built.body_string(),
        resp.clone(),
    );
    let p = RealImageProvider::with_bytes_fetch(transport, bytes);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let got = rt.block_on(p.generate_image(provider, "test-api-key", params));

    match row["outcome"].as_str().unwrap() {
        "ok" => {
            let out = got.unwrap_or_else(|e| panic!("{label}: expected ok, got err {e}"));
            assert_eq!(project(&out), row["images"], "{label} images");
            // A row with no download must be pure-parse-identical.
            if row.get("download").filter(|d| !d.is_null()).is_none() {
                assert_eq!(
                    project(&pure.unwrap()),
                    row["images"],
                    "{label}: no-download row diverged from the pure parse"
                );
            }
        }
        "thrown" => {
            let err = got.expect_err(&format!("{label}: expected thrown"));
            assert_eq!(
                err.message,
                row["thrown"].as_str().unwrap(),
                "{label} thrown"
            );
        }
        other => panic!("{label}: unknown outcome {other}"),
    }
    assert_eq!(
        is_image_moderation_error(row["thrown"].as_str().unwrap_or("")),
        row["isModeration"].as_bool().unwrap(),
        "{label} moderation verdict"
    );
}

/// Standard base64 → bytes (the recorder writes the download payload as base64
/// so the committed fixture stays a diffable text file).
fn base64_decode(s: &str) -> Vec<u8> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut out = Vec::new();
    for ch in s.bytes().filter(|b| *b != b'=' && !b.is_ascii_whitespace()) {
        let v = T
            .iter()
            .position(|c| *c == ch)
            .unwrap_or_else(|| panic!("non-base64 byte {ch:?} in a recorded download payload"))
            as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// Replay one recorded `kind:'models'` row through the Rust discovery path.
///
/// The no-key rows must make ZERO requests (v4 returns the static list without
/// touching the network), so those are checked against
/// `supported_image_models` directly. Keyed rows drive the whole composed
/// path through a canned transport, which ALSO proves the request bytes: an
/// unregistered `METHOD\nURL\nBODY` signature is a hard miss.
fn check_models_row(
    provider: &str,
    row: &Value,
    rows: &mut usize,
    cases: &mut std::collections::HashSet<String>,
) {
    let case = row["case"].as_str().unwrap();
    let label = format!("{provider}/{case}");
    *rows += 1;
    cases.insert(label.clone());

    // The plugin's `supportedModels`, recorded off the live instance.
    let want_supported: Vec<String> = row["supportedModels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let got_supported: Vec<String> = supported_image_models(provider)
        .unwrap_or_else(|e| panic!("{label}: {e}"))
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(got_supported, want_supported, "{label} supportedModels");

    let requests = row["requests"].as_array().unwrap();
    let wire = row["wire"].as_array().unwrap();
    let with_key = row["withKey"].as_bool().unwrap();
    let api_key = if with_key { Some("test-api-key") } else { None };

    if !with_key {
        assert!(
            requests.is_empty(),
            "{label}: v4 made {} request(s) without a key",
            requests.len()
        );
    }

    // 1. Request bytes + headers, page by page.
    let mut transport = CannedWireTransport::new();
    let mut page_token: Option<String> = None;
    for (i, want_req) in requests.iter().enumerate() {
        let built = build_models_request(provider, api_key.unwrap(), page_token.as_deref())
            .unwrap_or_else(|e| panic!("{label}: build page {i} failed: {e}"));
        assert_eq!(
            built.method,
            want_req["method"].as_str().unwrap(),
            "{label} page {i} method"
        );
        assert_eq!(
            built.url,
            want_req["url"].as_str().unwrap(),
            "{label} page {i} url"
        );
        assert!(
            want_req["body"].is_null(),
            "{label} page {i}: v4 sent a body on a model-list GET"
        );
        assert!(
            built.body.is_null(),
            "{label} page {i}: v5 built a body on a model-list GET"
        );
        // Header SUBSET: every header v5 builds must appear in what v4 sent,
        // with the same value (the P4.44 post-`apply_auth` precedent). The
        // transport's own `User-Agent` — a version string — is not compared.
        let want_headers = want_req["headers"].as_object().unwrap();
        for (k, v) in &built.headers {
            let got = want_headers
                .get(&k.to_lowercase())
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{label} page {i}: v4 sent no `{k}` header"));
            assert_eq!(got, v, "{label} page {i} header {k}");
        }
        // Register the recorded wire answer under the exact request signature.
        let w = &wire[i];
        transport = transport.with_response(
            &built.method,
            &built.url,
            "",
            WireResponse::new(
                w["status"].as_u64().unwrap() as u16,
                w["body"].as_str().unwrap(),
            ),
        );
        // Advance the page token the way the composer will.
        let resp = WireResponse::new(
            w["status"].as_u64().unwrap() as u16,
            w["body"].as_str().unwrap(),
        );
        page_token = parse_models_page(provider, &resp)
            .ok()
            .and_then(|p| p.next_page_token);
    }

    // 2. The composed answer (or the exact thrown sentence).
    let provider_impl = RealImageProvider::new(transport);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let got = rt.block_on(provider_impl.available_models(provider, api_key));
    match row["outcome"].as_str().unwrap() {
        "ok" => {
            let ids = got.unwrap_or_else(|e| panic!("{label}: expected ok, got err {e}"));
            let want: Vec<String> = row["models"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            assert_eq!(ids, want, "{label} models");
        }
        "thrown" => {
            let err = got.expect_err(&format!("{label}: expected thrown"));
            assert_eq!(
                err.message,
                row["thrown"].as_str().unwrap(),
                "{label} thrown"
            );
        }
        other => panic!("{label}: unknown outcome {other}"),
    }

    // 3. `finalize_models` in isolation over the collected page ids, so the
    //    dedup / union / sort / empty semantics are pinned even where the
    //    composed answer would hide them.
    if with_key {
        let mut collected = Vec::new();
        let mut ok_pages = true;
        for w in wire {
            let resp = WireResponse::new(
                w["status"].as_u64().unwrap() as u16,
                w["body"].as_str().unwrap(),
            );
            match parse_models_page(provider, &resp) {
                Ok(page) => collected.extend(page.ids),
                Err(_) => ok_pages = false,
            }
        }
        if ok_pages {
            let finalized = finalize_models(provider, collected);
            match row["outcome"].as_str().unwrap() {
                "ok" => assert_eq!(
                    finalized.unwrap(),
                    row["models"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_string())
                        .collect::<Vec<_>>(),
                    "{label} finalize_models"
                ),
                _ => assert_eq!(
                    finalized.unwrap_err().message,
                    row["thrown"].as_str().unwrap(),
                    "{label} finalize_models thrown"
                ),
            }
        }
    }
}
