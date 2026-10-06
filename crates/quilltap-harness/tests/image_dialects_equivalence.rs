//! Differential (W4.7f): the five image-generation wire dialects
//! (`quilltap_core::model::image_dialects`) vs v4's REAL image-provider plugins.
//!
//! For every committed row, this reconstructs the [`ImageGenParams`] from the
//! recorded `input`, runs the Rust `build_image_request` (diffing method / url /
//! body bytes against what the plugin/SDK actually sent), and — for `mode:'wire'`
//! rows — runs `parse_image_response` over the recorded wire `{status, body}`,
//! diffing the parsed `ImageGenResponse` OR the thrown error. For
//! `mode:'sdkThrow'` rows (an SDK converting a non-2xx to a throw) the whole
//! composed [`RealImageProvider`] runs over the recorded wire, so v5's own
//! reconstruction of the SDK's `APIError` — and, since v4 `8bd080267` (#73,
//! P4.D225), each plugin's `to*ImageModerationError` mapping — is diffed too.
//!
//! **The thrown comparand is the refusal classifier's view of the error**
//! (widened from a bare string at P4.D225): `{ message, code?, errorCode?,
//! providerReason?, status? }` — the typed `MODERATION_REJECTED`, the SDK's own
//! `code` / `error.code` (read through `code_string`, exactly as the
//! classifier's `collectCodes` reads them), the provider's reason, the HTTP
//! status. The keyword-verdict arms (`is_image_moderation_error` over every
//! rejection row) are RETIRED with the helper, by name: v4 deleted
//! `isImageModerationError` at `8bd080267`, and the classifier's own
//! equivalence lives in `refusal_classify_equivalence`.
//!
//! **P4.154: the Gemini safety WARN.** The recorder now bridges every
//! plugin's logger (`pluginWarnLog` on a `dialect` row); for the pure-parse
//! rows the family compares v5's `Gemini withheld the image on safety grounds`
//! line against v4's BOTH ways — present on exactly v4's three rows, under
//! v4's camelCase `finishReason` / `blockReason`, with the key the plugin
//! passed as `undefined` omitted. The other WARNs the bridge now records are
//! NOT compared (out of P4.154's scope; a named follow-up).
//!
//! The fixture is committed (no env var); regenerate with
//! `harness/oracle/providers/regenerate-image-fixtures.sh`.
//!
//! Run:
//!   cargo test -p quilltap-harness --test image_dialects_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::image_gen::{OrientationMapping, OrientationStrategy, OrientationSupport};
use quilltap_core::image_gen_data::orientation_data_for;
use quilltap_core::model::image::ImageGenError;
use quilltap_core::model::image::{
    ImageGenParams, ImageGenResponse, ImageModelDiscovery, ImageProvider,
};
use quilltap_core::model::image_bytes::{CannedImageBytes, FetchedImageBytes};
use quilltap_core::model::image_dialects::{
    build_image_request, build_models_request, finalize_models, parse_image_response,
    parse_models_page, supported_image_models, RealImageProvider,
};
use quilltap_core::model::wire::{CannedWireTransport, WireResponse};
use quilltap_core::services::dangerous_content::refusal::code_string;
use quilltap_core::test_support::captured_with;
use serde_json::{Map, Value};

fn corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/image-dialects/image-dialects.recorded.ndjson")
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Rebuild one `ImageLoraSpec` from the recorded `loras` entry (P4.D138).
fn lora_spec_from_json(v: &Value) -> quilltap_core::image_gen::lora_support::ImageLoraSpec {
    quilltap_core::image_gen::lora_support::ImageLoraSpec {
        source: v
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        scale: v.get("scale").and_then(Value::as_f64),
        trigger_phrase: v
            .get("triggerPhrase")
            .and_then(Value::as_str)
            .map(str::to_string),
        label: v.get("label").and_then(Value::as_str).map(str::to_string),
    }
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
        n: input.get("n").and_then(Value::as_f64),
        size: opt_str(input, "size"),
        aspect_ratio: opt_str(input, "aspectRatio"),
        quality: opt_str(input, "quality"),
        style: opt_str(input, "style"),
        seed: input.get("seed").and_then(Value::as_f64),
        guidance_scale: input.get("guidanceScale").and_then(Value::as_f64),
        response_format: opt_str(input, "responseFormat"),
        loras: input
            .get("loras")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(lora_spec_from_json).collect())
            .unwrap_or_default(),
        profile_parameters: input.get("profileParameters").cloned(),
        // The insertion-order flags only steer `to_key_value`; a params object
        // rebuilt from a canned key never re-emits one.
        size_inserted_by_orientation: false,
        aspect_ratio_inserted_by_orientation: false,
        steps: input.get("steps").and_then(Value::as_f64),
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
    let mut openai_cases: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut sdk_throw_rows = 0usize;
    let mut typed_rows = 0usize;
    let mut gemini_safety_cases: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();

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
        if provider == "OPENAI" {
            openai_cases.insert(case.to_string());
        }
        // Absent when the case deliberately supplies NO model (P4.D101's
        // nanogpt `default_model`, which proves the plugin's own `?? 'hidream'`;
        // `d8d2890ee`'s openai `no_model_at_all`, which proves the raw
        // `requestParams.model` never reaches the wire).
        let model = row["model"].as_str().unwrap_or_default();
        let label = format!("{provider}/{case}");
        rows += 1;

        // 1. Request bytes (method / url / body).
        let params = params_from_json(&row["input"]);
        // `d8d2890ee`: the parse now reads the whole `params` (OPENAI's
        // `mimeType` follows the requested output format), so the row's own
        // `model` field is no longer a second source. Pin that they agree
        // rather than drop it — a recorder that stopped writing `model` would
        // otherwise be invisible here.
        assert_eq!(params.model, model, "{label}: row.model vs input.model");
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

        if mode == "sdk_throw" || mode == "sdkThrow" {
            // P4.D225: drive the composed provider over the recorded non-2xx —
            // v5's `APIError` reconstruction AND the plugin's moderation
            // mapping, against v4's real SDK + plugin.
            check_sdk_throw_row(&row, &provider, &params, &built, &label);
            sdk_throw_rows += 1;
            continue;
        }

        // 2. Parse the recorded wire response.
        let wire = &row["wire"];
        let resp = WireResponse::new(
            wire["status"].as_u64().unwrap() as u16,
            wire["body"].as_str().unwrap().to_string(),
        );
        // `ca22ec45` (Z.AI) and P4.D101 (NanoGPT): the URL→base64 download
        // happens INSIDE the provider, above the pure parse — so both
        // providers' rows are driven through the whole composed
        // `generate_image`, with the recorded download answer canned per URL.
        if provider == "Z_AI" || provider == "NANOGPT" {
            check_download_row(&row, &provider, &params, &resp, &built);
            continue;
        }
        let (parsed, v5_lines) = captured_with(|| parse_image_response(&provider, &params, &resp));
        // P4.154 (§R.5): the Gemini safety WARN against the line v4's REAL
        // plugin logged (the recorder's `pluginWarnLog` bridge), both ways — v5
        // logs it on exactly the rows v4 does, under v4's camelCase keys, with
        // the key v4 passed as `undefined` OMITTED (winston drops it).
        let v5_safety: Vec<String> = v5_lines
            .into_iter()
            .filter(|l| l.contains(GEMINI_SAFETY_WARN))
            .collect();
        let v4_safety = v4_warn_lines(&row, GEMINI_SAFETY_WARN);
        assert_eq!(v5_safety, v4_safety, "{label}: the Gemini safety WARN");
        if !v4_safety.is_empty() {
            gemini_safety_cases.insert(case.to_string());
        }
        match row["outcome"].as_str().unwrap() {
            "ok" => {
                let got = parsed.unwrap_or_else(|e| panic!("{label}: expected ok, got err {e}"));
                assert_eq!(project(&got), row["images"], "{label} images");
            }
            "thrown" => {
                let err = parsed.expect_err(&format!("{label}: expected thrown"));
                assert_eq!(thrown_of(&err), expected_thrown(&row), "{label} thrown");
                if err.refusal.as_deref().is_some_and(|r| r.is_typed_refusal()) {
                    typed_rows += 1;
                }
            }
            other => panic!("{label}: unknown outcome {other}"),
        }
    }

    assert!(rows >= 25, "expected a substantial corpus, got {rows}");
    // P4.D225 floors: the composed SDK-throw path and the typed wire rows must
    // actually be exercised (a corpus that lost them would pass vacuously).
    assert!(
        sdk_throw_rows >= 15,
        "expected the sdkThrow rows, got {sdk_throw_rows}"
    );
    assert!(
        typed_rows >= 10,
        "expected the typed wire refusals, got {typed_rows}"
    );
    for p in ["OPENAI", "GOOGLE", "GROK", "OPENROUTER", "Z_AI"] {
        assert!(providers.contains(p), "corpus missing provider {p}");
    }
    // P4.154: the three arms of the Gemini safety WARN — `finishReason` alone
    // (`blockReason` undefined), `blockReason` alone (`finishReason`
    // undefined), and both — each recorded through the real plugin's logger.
    assert_eq!(
        gemini_safety_cases.into_iter().collect::<Vec<_>>(),
        [
            "gemini_block_and_finish",
            "gemini_block_reason",
            "gemini_image_safety_finish"
        ],
        "the Gemini safety WARN rows"
    );

    // `d8d2890ee` coverage floor. A green run over a corpus that lost these
    // rows would measure nothing: each names one arm of the OpenAI rewrite
    // that has no other pin — the premium tiers, the arbitrary-size accept and
    // every rejection reason, the four GPT Image extras, the transparent-JPEG
    // force, the per-format mimeType, the `n` cap, and the unknown-model
    // passthrough. Names, not a count, so a rename is as loud as a deletion.
    for case in [
        "q25_sunburst_xhigh",
        "q25_flare_max",
        "q_dated_snapshot_max",
        "q_drop_xhigh_on_gpt_image_2",
        "q_omit_for_gpt_image_unset",
        "q_dalle3_bogus_falls_back",
        "size_arbitrary_accepted",
        "size_experimental_arbitrary",
        "size_bad_edge_multiple",
        "size_bad_aspect",
        "size_bad_edge",
        "size_bad_pixels",
        "size_unparseable",
        "size_arbitrary_refused_on_1_5",
        "extras_all_four",
        "extras_force_png_for_transparent_jpeg",
        "extras_drop_compression_for_png",
        "extras_drop_bad_values",
        "extras_never_on_dalle",
        "extras_numeric_string_compression",
        "extras_zero_compression",
        "mime_webp",
        "mime_jpeg",
        "mime_unset_png",
        "style_not_on_dalle2",
        "style_dropped_on_gpt_image",
        "n_capped_on_dalle3",
        "n_kept_on_sunburst",
        "unknown_model_forwarded",
        "no_model_at_all",
    ] {
        assert!(
            openai_cases.contains(case),
            "image-dialects corpus lost the d8d2890ee OPENAI case {case}"
        );
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

/// v4's Gemini no-image safety WARN (`image-provider.ts:185`).
const GEMINI_SAFETY_WARN: &str = "Gemini withheld the image on safety grounds";

/// The WARN lines v4's plugin logged on `row` with `message`, rendered as v5's
/// capture layer renders the port's line (target `image_dialects`; v4's context
/// keys in v4's order, a string unquoted). A key the plugin passed as
/// `undefined` is absent from the recorded JSON — and named in `undefinedKeys`,
/// which must never reach the line.
fn v4_warn_lines(row: &Value, message: &str) -> Vec<String> {
    let Some(log) = row.get("pluginWarnLog").and_then(Value::as_array) else {
        return Vec::new();
    };
    log.iter()
        .filter(|w| w["message"].as_str() == Some(message))
        .map(|w| {
            let ctx = w["context"].as_object().expect("a context bag");
            let undefined: Vec<&str> = w["undefinedKeys"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let fields: Vec<String> = ctx
                .iter()
                .filter(|(k, _)| !undefined.contains(&k.as_str()))
                .map(|(k, v)| match v {
                    Value::String(s) => format!("{k}={s}"),
                    other => format!("{k}={other}"),
                })
                .collect();
            format!(
                "WARN quilltap_core::model::image_dialects {message} {}",
                fields.join(" ")
            )
        })
        .collect()
}

/// Drive a recorded z-ai `generateImage` row through the WHOLE composed
/// provider — build, wire, parse, and (when the entry carries only a `url`) the
/// `ca22ec45` image download — so the URL→base64 conversion, the content-type
/// sniff and both new error sentences are diffed against v4's real plugin.
///
/// The download seam is canned per URL from the recorded `download` block; a
/// row with NO download block registers nothing, so an unexpected download
/// attempt fails loudly rather than yielding empty bytes.
fn check_download_row(
    row: &Value,
    provider: &str,
    params: &ImageGenParams,
    resp: &WireResponse,
    built: &quilltap_core::model::request_builder::BuiltRequest,
) {
    let case = row["case"].as_str().unwrap();
    let label = format!("{provider}/{case}");

    // The pure parse still has to agree (the pre-download shape).
    let pure = parse_image_response(provider, params, resp);

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
            assert_eq!(thrown_of(&err), expected_thrown(row), "{label} thrown");
        }
        other => panic!("{label}: unknown outcome {other}"),
    }
}

/// v4's recorded thrown fields, with both code slots read through
/// `code_string` — the classifier's `collectCodes` view (a numeric `1210`
/// and the string `"1210"` are the same code to it).
fn expected_thrown(row: &Value) -> Value {
    let t = &row["thrown"];
    let mut o = Map::new();
    o.insert("message".into(), t["message"].clone());
    for (key, out) in [("code", "code"), ("errorCode", "errorCode")] {
        if let Some(c) = t.get(key).and_then(code_string) {
            o.insert(out.into(), Value::String(c));
        }
    }
    if let Some(r) = t.get("providerReason") {
        o.insert("providerReason".into(), r.clone());
    }
    if let Some(st) = t.get("status") {
        o.insert("status".into(), st.clone());
    }
    Value::Object(o)
}

/// v5's error in the same shape: the message, and the structured refusal side
/// when the dialect filled one.
fn thrown_of(err: &ImageGenError) -> Value {
    let mut o = Map::new();
    o.insert("message".into(), Value::String(err.message.clone()));
    if let Some(r) = err.refusal.as_deref() {
        if let Some(c) = &r.code {
            o.insert("code".into(), Value::String(c.clone()));
        }
        if let Some(c) = &r.nested_code {
            o.insert("errorCode".into(), Value::String(c.clone()));
        }
        if let Some(p) = &r.provider_reason {
            o.insert("providerReason".into(), Value::String(p.clone()));
        }
        if let Some(st) = r.status {
            o.insert("status".into(), Value::from(st));
        }
    }
    Value::Object(o)
}

/// An SDK provider's non-2xx, through the whole composed [`RealImageProvider`]
/// (P4.D225): v5 fetches the wire itself, so the SDK's `APIError` and the
/// plugin's `to*ImageModerationError` both live in v5 code that only a
/// composed run reaches.
fn check_sdk_throw_row(
    row: &Value,
    provider: &str,
    params: &ImageGenParams,
    built: &quilltap_core::model::request_builder::BuiltRequest,
    label: &str,
) {
    let wire = &row["wire"];
    let resp = WireResponse::new(
        wire["status"].as_u64().unwrap() as u16,
        wire["body"].as_str().unwrap().to_string(),
    );
    let transport = CannedWireTransport::new().with_response(
        &built.method,
        &built.url,
        &built.body_string(),
        resp,
    );
    let p = RealImageProvider::with_bytes_fetch(transport, CannedImageBytes::new());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let got = rt.block_on(p.generate_image(provider, "test-api-key", params));
    assert_eq!(
        row["outcome"].as_str(),
        Some("thrown"),
        "{label}: an sdkThrow row throws"
    );
    let err = got.expect_err(&format!("{label}: expected thrown"));
    assert_eq!(thrown_of(&err), expected_thrown(row), "{label} thrown");
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
