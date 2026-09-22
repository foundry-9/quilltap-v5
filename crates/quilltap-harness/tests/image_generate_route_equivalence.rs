//! P4.6ai IMAGE-GENERATE ROUTE-ENVELOPE differential: the ported
//! `imageProfileGenerate` un-refusal (`api::image_profiles::image_profile_generate`
//! over the injected W4.9a runner) vs v4's REAL
//! `POST /api/v1/image-profiles/[id]?action=generate` route handler. Both sides run
//! the same case list on a FRESH copy of the shared image-generation two-DB fixture
//! (the deep save/store writes are separately pinned by `image_generation_tier3`;
//! here we diff the ROUTE wrapper's `{success, data, expandedPrompt, metadata}`
//! envelope). The minted `files.id` (+ its `/api/v1/images|files/<id>` url/filepath)
//! is uuid-normalized on both sides.
//!
//! MODEL SEAMS (the `image_generation_tier3` mold; the corpus is PLACEHOLDER-FREE +
//! danger OFF, so NO completion/cheap-LLM call fires — only the image provider is
//! canned): the REAL image dialect ([`RealImageProvider`]) over a canned
//! [`CannedWireTransport`] (each oracle image reverse-mapped to the OpenAI-SDK wire
//! shape, keyed by `build_image_request`), an empty completion provider,
//! [`NoModerationProvider`], an injected [`ApiKeyResolver`] map, the
//! [`PassthroughTranscoder`], [`NoLanternNotification`] (the notification is a
//! side effect off the envelope), the OPENAI size-strategy `orientation_data_for`
//! closure (the route passes no orientation → the square default), and the frozen
//! `now_ms`.
//!
//! P4.96 widened the family from the four-field Shared contract to v4's WHOLE
//! `generateImageSchema` body (eight keys), and added two comparands the
//! envelope cannot supply:
//!   - the `details` issue array is NO LONGER subtracted (it used to be, so
//!     every refusal row compared a fixed sentence against a fixed sentence);
//!   - a `kind:"toolInput"` SIDE CHANNEL — the oracle WRAPS v4's
//!     `executeImageGenerationTool` (the real one still runs, so no envelope
//!     row moves) and records the object the route assembled, and the Rust
//!     runner records its own. The five shaping fields appear NOWHERE in
//!     `{success, data, expandedPrompt, metadata}`, so this is the only place
//!     they can be diffed at all.
//!
//! 24 of the 30 rows are red against the pre-P4.96 handler.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .test.ts header):
//!   … build-image-generation-fixture.ts (QT_FIXTURE_IMGGEN_MAIN/MOUNT) …
//!   QT_ORACLE_OUT=/tmp/oracle-image-generate-route.ndjson \
//!     npx jest … --roots "$PWD" --roots "$STAGE/cases" -- image-generate-route.test
//! Run:
//!   QT_ORACLE_IMGGEN_ROUTE=/tmp/oracle-image-generate-route.ndjson \
//!   QT_FIXTURE_IMGGEN_MAIN=/tmp/qt-imggen-main.db QT_FIXTURE_IMGGEN_MOUNT=/tmp/qt-imggen-mount.db \
//!     cargo test -p quilltap-harness --test image_generate_route_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use quilltap_core::api::image_profiles::{image_profile_generate, ImageProfileGenerateBody};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::image_gen::params_builder::ImageDeclarations;
use quilltap_core::image_gen::{
    ModelInfo, OrientationMapping, OrientationStrategy, OrientationSupport,
};
use quilltap_core::model::completion::CannedCompletionProvider;
use quilltap_core::model::image::{ImageGenParams, PassthroughTranscoder};
use quilltap_core::model::image_dialects::{build_image_request, RealImageProvider};
use quilltap_core::model::wire::{wire_key, CannedWireTransport, WireResponse};
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::dangerous_content::gatekeeper::NoModerationProvider;
use quilltap_core::services::dangerous_content::provider_routing::ApiKeyResolver;
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::tools::generate_image::{
    execute_image_generation_tool, ErasedImageGeneration, ImageGenDeps, ImageGenerationRunner,
    ImageGenerationToolInput, ImageGenerationToolOutput, ImageToolExecutionContext,
    NoLanternNotification,
};
use serde::Deserialize;
use serde_json::{json, Value};

mod common;

// The shared fixture's chat ids + a bogus profile (mirror the oracle's `cases`).
const CHAT_PLAIN: &str = "aaaa0001-0000-4000-8000-000000000001";
const CHAT_ORIENT: &str = "aaaa0002-0000-4000-8000-000000000002";
const BOGUS_PROFILE: &str = "e0000000-0000-4000-8000-0000000000ff";

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "profileId")]
    profile_id: String,
    #[serde(rename = "apiKeys")]
    api_keys: HashMap<String, String>,
    #[serde(rename = "frozenNowMs")]
    frozen_now_ms: i64,
}

#[derive(Deserialize)]
struct CannedImageRow {
    key: String,
    images: Vec<CannedImageImg>,
}
#[derive(Deserialize)]
struct CannedImageImg {
    data: String,
    #[serde(rename = "revisedPrompt")]
    revised_prompt: Option<String>,
}
#[derive(Deserialize)]
struct ResultRow {
    name: String,
    status: i64,
    body: Value,
}

/// The P4.96 side channel: the input object v4's route handed
/// `executeImageGenerationTool` (`null` when the route refused before the tool).
#[derive(Deserialize)]
struct ToolInputRow {
    name: String,
    input: Value,
}

/// One test case. `body` is the JSON object the oracle POSTs to v4's route,
/// spelled IDENTICALLY here — P4.96 moved the case from four decoded fields to
/// the raw body so the two lists stay diffable key for key, and so every shape
/// v4's `generateImageSchema` can refuse (`null`, a number, `{}`, a bad enum) is
/// expressible on this side at all.
struct Case {
    name: &'static str,
    id: String,
    body: Value,
}

/// The `Request::ImageProfileGenerate` body the handler takes, built from the
/// case's raw JSON exactly as the engine arm builds it from the variant — an
/// ABSENT key stays `None` (v4's `.optional()`), a present one rides RAW.
fn body_of(case: &Case) -> ImageProfileGenerateBody {
    let g = |k: &str| case.body.get(k).cloned();
    ImageProfileGenerateBody {
        prompt: g("prompt"),
        chat_id: g("chatId"),
        count: g("count"),
        size: g("size"),
        quality: g("quality"),
        style: g("style"),
        aspect_ratio: g("aspectRatio"),
        negative_prompt: g("negativePrompt"),
    }
}

/// The `kind:"toolInput"` comparand: the object v4's route hands
/// `executeImageGenerationTool`, rebuilt from the v5 tool input.
///
/// v4 passes exactly seven keys (`prompt`, `count`, `size`, `quality`, `style`,
/// `aspectRatio`, `negativePrompt`) and `JSON.stringify` drops the `undefined`
/// ones, so an absent field is an absent KEY on both sides. `orientation` is
/// rendered when present precisely BECAUSE v4 never passes it: a v5 handler that
/// started setting it would diverge here rather than silently.
fn tool_input_comparand(input: &ImageGenerationToolInput) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("prompt".into(), Value::String(input.prompt.clone()));
    if let Some(n) = input.count {
        o.insert("count".into(), serde_json::json!(n));
    }
    for (k, v) in [
        ("size", &input.size),
        ("quality", &input.quality),
        ("style", &input.style),
        ("aspectRatio", &input.aspect_ratio),
        ("negativePrompt", &input.negative_prompt),
        ("orientation", &input.orientation),
    ] {
        if let Some(v) = v {
            o.insert(k.into(), Value::String(v.clone()));
        }
    }
    Value::Object(o)
}

/// The oracle's `ASTRAL_AT_MAX` / `ASTRAL_OVER_MAX`, character for character:
/// `n` BMP chars plus one astral, so the code-point count is `n + 17` while the
/// UTF-16 count is `n + 18`. At `n = 3983` that is 4000 code points in 4001
/// units — the one string on which the two measures disagree at Zod's bound.
fn astral_prompt(fill: usize) -> String {
    format!("A moonlit tower {}\u{1F702}", "a".repeat(fill))
}

// ===========================================================================
// Injected seams (mirror the oracle mocks)
// ===========================================================================

struct CannedApiKeys(HashMap<String, String>);
impl ApiKeyResolver for CannedApiKeys {
    fn resolve(&self, api_key_id: &str, _user_id: &str) -> Option<String> {
        self.0.get(api_key_id).cloned()
    }
}

fn size_support(portrait: &str, landscape: &str, square: &str) -> OrientationSupport {
    OrientationSupport {
        strategy: OrientationStrategy::Size,
        portrait: OrientationMapping {
            size: Some(portrait.to_string()),
            ..Default::default()
        },
        landscape: OrientationMapping {
            size: Some(landscape.to_string()),
            ..Default::default()
        },
        square: Some(OrientationMapping {
            size: Some(square.to_string()),
            ..Default::default()
        }),
    }
}

/// The OPENAI size-strategy support (identical to the oracle's OPENAI_SUPPORT).
/// P4.D138: the canned per-model `loraSupport` the oracle's registry mock also
/// declares, so the shared params builder's cap + trigger-phrase append are
/// MEASURABLE on this path (two adapters, no scale block).
fn canned_lora_support() -> quilltap_core::image_gen::lora_support::ImageLoraSupport {
    quilltap_core::image_gen::lora_support::ImageLoraSupport {
        max_loras: 2.0,
        scale: None,
        source_kinds: vec!["url".to_string(), "hf-repo".to_string()],
        supports_private_weights_token: None,
    }
}

/// The canned plugin-registry declarations (v4's `getImageGenerationModels` +
/// `getImageProviderConstraints`). P4.D138 widened the seam from the
/// orientation half to v4's whole declaration set; these fixtures declare no
/// LoRA support on the ids the fixtures point at, mirroring the oracle's mock.
fn declarations_for(provider: &str) -> ImageDeclarations {
    match provider {
        "OPENAI" => {
            let s = size_support("1024x1792", "1792x1024", "1024x1024");
            ImageDeclarations {
                models: vec![ModelInfo {
                    lora_support: Some(canned_lora_support()),
                    id: "dall-e-3".to_string(),
                    orientation_support: Some(s.clone()),
                }],
                orientation_provider: Some(s),
                lora_provider: None,
            }
        }
        _ => ImageDeclarations::default(),
    }
}

/// The test's `ImageGenerationRunner` — the `image_generation_tier3` seams behind the
/// engine's injection boundary, rebuilding `ImageGenDeps` per run (the host-runner
/// idiom). The executor is inert here (placeholder-free corpus → no cheap-LLM call).
struct TestImageRunner {
    image_provider: RealImageProvider<CannedWireTransport>,
    completion: CannedCompletionProvider,
    api_keys: CannedApiKeys,
    now_ms: i64,
    /// P4.96's side channel, mirroring the oracle's wrapped
    /// `executeImageGenerationTool`: the input the handler assembled for the
    /// case in flight. The route envelope carries none of the five shaping
    /// keys, so without this the differential cannot see them at all. Shared
    /// with the test loop: the runner is moved into the erased box, so the
    /// handle — not the struct — is what the per-case read goes through.
    recorded_input: Arc<Mutex<Option<Value>>>,
}
impl ImageGenerationRunner for TestImageRunner {
    fn run<'a>(
        &'a self,
        db: &'a Db,
        input: &'a ImageGenerationToolInput,
        ctx: &'a ImageToolExecutionContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ImageGenerationToolOutput> + Send + 'a>>
    {
        Box::pin(async move {
            *self.recorded_input.lock().expect("recorded_input") =
                Some(tool_input_comparand(input));
            let moderation = NoModerationProvider;
            let transcoder = PassthroughTranscoder;
            let lantern = NoLanternNotification;
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: db.clone(),
                user_id: ctx.user_id.clone(),
                chat_id: ctx.chat_id.clone(),
                message_id: None,
                ctx: LogContext::none(),
            });
            let declarations_fn = declarations_for;
            let deps = ImageGenDeps {
                image_provider: &self.image_provider,
                completion: &self.completion,
                moderation: &moderation,
                api_keys: &self.api_keys,
                transcoder: &transcoder,
                lantern: &lantern,
                executor: &executor,
                now_ms: self.now_ms,
                declarations_for: &declarations_fn,
                blob_webp: None,
            };
            execute_image_generation_tool(db, &deps, input, ctx).await
        })
    }
}

// ===========================================================================
// Canned image wire (the tier3 mold)
// ===========================================================================

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

/// Reconstruct [`ImageGenParams`] from an `image_gen_key` param JSON.
fn params_from_key_json(v: &Value) -> ImageGenParams {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    ImageGenParams {
        prompt: s("prompt").unwrap_or_default(),
        negative_prompt: s("negativePrompt"),
        model: s("model").unwrap_or_default(),
        n: v.get("n").and_then(Value::as_f64),
        size: s("size"),
        aspect_ratio: s("aspectRatio"),
        quality: s("quality"),
        style: s("style"),
        seed: v.get("seed").and_then(Value::as_f64),
        guidance_scale: v.get("guidanceScale").and_then(Value::as_f64),
        response_format: s("responseFormat"),
        loras: v
            .get("loras")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(lora_spec_from_json).collect())
            .unwrap_or_default(),
        profile_parameters: v.get("profileParameters").cloned(),
        // The insertion-order flags only steer `to_key_value`; a params object
        // rebuilt from a canned key never re-emits one.
        size_inserted_by_orientation: false,
        aspect_ratio_inserted_by_orientation: false,
        steps: v.get("steps").and_then(Value::as_f64),
    }
}

/// Register an oracle image response as the canned WIRE the dialect parses.
fn register_image_wire(
    transport: CannedWireTransport,
    row: &CannedImageRow,
) -> CannedWireTransport {
    let mut parts = row.key.splitn(3, '|');
    let provider = parts.next().expect("provider in key");
    let _model = parts.next();
    let params_json: Value = serde_json::from_str(parts.next().expect("params json in key"))
        .expect("params json parses");
    let params = params_from_key_json(&params_json);
    let req = build_image_request(provider, &params).expect("build image request");
    let data: Vec<Value> = row
        .images
        .iter()
        .map(|i| {
            let mut o = serde_json::Map::new();
            o.insert("b64_json".into(), Value::String(i.data.clone()));
            if let Some(rp) = &i.revised_prompt {
                o.insert("revised_prompt".into(), Value::String(rp.clone()));
            }
            Value::Object(o)
        })
        .collect();
    let body = serde_json::json!({ "data": data }).to_string();
    let key = wire_key(&req.method, &req.url, &req.body_string());
    transport.with_raw_response(key, WireResponse::new(200, body))
}

// ===========================================================================
// Normalization
// ===========================================================================

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

/// Positional UUID + `/api/v1/*/<uuid>` normalization over a serialized envelope: the
/// minted `files.id` (and its url/filepath) differ between v4 + Rust, so a positional
/// map collapses them to `<uuid-N>` first-appearance tokens (the sorted-key traversal
/// is deterministic → the same assignment on both sides).
fn norm_uuids_in_string(s: &str) -> String {
    let bytes = s.as_bytes();
    let is_hex = |b: u8| b.is_ascii_hexdigit();
    let mut out = String::with_capacity(s.len());
    let mut map: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < bytes.len() {
        // `str::get` rather than a byte slice: a uuid is all ASCII, but the
        // envelope need not be — the P4.85 astral prompt arm made this panic
        // ("not a char boundary"), which no corpus row had ever asked before.
        if let Some(cand) = s.get(i..i + 36) {
            let cb = cand.as_bytes();
            let shape_ok = cb.len() == 36
                && cb[8] == b'-'
                && cb[13] == b'-'
                && cb[18] == b'-'
                && cb[23] == b'-'
                && cand
                    .chars()
                    .enumerate()
                    .all(|(j, c)| matches!(j, 8 | 13 | 18 | 23) || is_hex(c as u8));
            if shape_ok {
                let next = format!("<uuid-{}>", map.len());
                let token = map.entry(cand.to_string()).or_insert(next).clone();
                out.push_str(&token);
                i += 36;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn norm(v: &Value) -> String {
    // P4.96: the `details` issue array is NO LONGER dropped. It used to be —
    // v5's `Validation error` envelope carried only the sentence, so the array
    // was subtracted and the refusal rows compared a fixed string against a
    // fixed string. Every Zod issue this route can raise is now reproduced
    // byte-for-byte (measured through this oracle at `5f0a57dc4`, never
    // hand-written), so the array is a real comparand: `path`, `code`, the
    // enum's `values`, the bound sentences, and the issue ORDER.
    let mut v = v.clone();
    canon_numbers(&mut v);
    let s = serde_json::to_string_pretty(&sorted(&v)).unwrap();
    norm_uuids_in_string(&s)
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}

/// The envelope carried by a `Response::ImageProfile` (the `data` field of the
/// serialized enum) or the `{error}` body of a `Response::Error`, plus the HTTP
/// status the web edge maps (v4's 201 collapses to 200 across the dispatch family —
/// the P4.6p precedent: success cases compare BODY only).
fn envelope_and_status(r: &Response) -> (i64, Value) {
    match r {
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 503,
                // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
                ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            // v4 `validationError(err)` answers `{error, details}`; the ONE home
            // of that body is `CoreError::validation_wire_body`, which both
            // transports render through — so what this family diffs is what the
            // wire sends. A refusal with no `details` keeps the plain `{error}`.
            let body = e
                .validation_wire_body()
                .unwrap_or_else(|| serde_json::json!({ "error": e.message }));
            (status, body)
        }
        other => {
            let data = serde_json::to_value(other)
                .unwrap()
                .get("data")
                .cloned()
                .unwrap_or(Value::Null);
            (200, data)
        }
    }
}

// ===========================================================================
// Fixture helpers
// ===========================================================================

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/image-generation.json")
}
fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}
fn cleanup(main: &Path, mount: &Path) {
    for p in [main, mount] {
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
        }
    }
}
fn fresh_copy(main_fixture: &str, mount_fixture: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let main_work = dir.join(format!(
        "qt-imggenroute-main-{}-{tag}.db",
        std::process::id()
    ));
    let mount_work = dir.join(format!(
        "qt-imggenroute-mount-{}-{tag}.db",
        std::process::id()
    ));
    cleanup(&main_work, &mount_work);
    std::fs::copy(main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));
    (main_work, mount_work)
}

// ===========================================================================
// The test
// ===========================================================================

#[test]
fn image_generate_route_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_IMGGEN_ROUTE"),
        env_or_skip("QT_FIXTURE_IMGGEN_MAIN"),
        env_or_skip("QT_FIXTURE_IMGGEN_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    // Parse the oracle: canned image WIRE + per-case result envelopes.
    let mut image_transport = CannedWireTransport::new();
    let mut results: HashMap<String, ResultRow> = HashMap::new();
    let mut tool_inputs: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle {oracle_path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("cannedImage") => {
                let row: CannedImageRow = serde_json::from_value(v).expect("parse cannedImage row");
                image_transport = register_image_wire(image_transport, &row);
            }
            Some("result") => {
                let row: ResultRow = serde_json::from_value(v).expect("parse result row");
                results.insert(row.name.clone(), row);
            }
            Some("toolInput") => {
                let row: ToolInputRow = serde_json::from_value(v).expect("parse toolInput row");
                tool_inputs.insert(row.name.clone(), row.input);
            }
            _ => {}
        }
    }

    // One runner over the canned seams (the executor is rebuilt per run inside it).
    let recorded_input: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
    let runner = ErasedImageGeneration::new(TestImageRunner {
        image_provider: RealImageProvider::new(image_transport),
        completion: CannedCompletionProvider::new(),
        api_keys: CannedApiKeys(spec.api_keys.clone()),
        now_ms: spec.frozen_now_ms,
        recorded_input: Arc::clone(&recorded_input),
    });

    // The case list mirrors `harness/oracle/cases/image-generate-route.test.ts`
    // name for name and body for body — the ONE list rule
    // (`a-case-added-only-to-the-oracle-is-never-run`, which this very family
    // suffered). `cases_cover_the_oracle` below asserts the two sets are equal,
    // so a row added on either side alone is a RED, not a silent skip.
    let cases = [
        Case {
            name: "generate_happy_chat",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A serene mountain lake at dawn, mist over the water", "chatId": CHAT_PLAIN, "count": 1 }),
        },
        Case {
            name: "generate_no_chat",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A quiet forest path", "count": 1 }),
        },
        Case {
            name: "generate_count2",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A tall waterfall in a canyon", "chatId": CHAT_ORIENT, "count": 2 }),
        },
        Case {
            name: "generate_profile_404",
            id: BOGUS_PROFILE.to_string(),
            body: json!({ "prompt": "anything", "count": 1 }),
        },
        // The route's own `generateImageSchema` gate (v4 `route.ts:236`), which
        // runs after the 404 and before the tool — the §3 unification review of
        // the follow-ups round: v5 handed `count: 20` to the TOOL's schema and
        // answered its fixed sentence where v4 answers `Validation error`.
        Case {
            name: "generate_count_over_max",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A lighthouse at night", "count": 20 }),
        },
        Case {
            name: "generate_prompt_empty",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "", "count": 1 }),
        },
        // P4.85 item 5 — the ASTRAL boundary of the SAME gate. 4000 code
        // points in 4001 UTF-16 units: v4 accepts it and runs the tool, and
        // v5 refused it with `Validation error` until that lane, because this
        // route counted UTF-16 units where its sibling `images_generate`
        // route already counted code points.
        Case {
            name: "generate_prompt_astral_at_max",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": astral_prompt(3983), "count": 1 }),
        },
        // One code point over: still refused — in code points, so the fix
        // cannot have been "stop counting".
        Case {
            name: "generate_prompt_astral_over_max",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": astral_prompt(3984), "count": 1 }),
        },
        // ------------------------------------------------------------------
        // P4.96 — v4's five optional shaping keys. Each happy row sets ONE, so
        // the `toolInput` comparand names exactly which key moved; the
        // `all_five` row proves they compose. Before this lane the handler
        // built its tool input with five hard `None`s, so EVERY one of these
        // rows was red: the happy rows on the missing key, the refusal rows on
        // a 201 where v4 answers 400.
        // ------------------------------------------------------------------
        Case {
            name: "generate_size",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A cliffside monastery", "count": 1, "size": "1024x1536" }),
        },
        Case {
            name: "generate_quality_hd",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A brass orrery", "count": 1, "quality": "hd" }),
        },
        // `d8d2890ee` widened this key from `z.enum(['standard','hd'])` to the
        // shared eight-tier `imageQualitySchema`; `max` is only reachable
        // through that widening.
        Case {
            name: "generate_quality_max",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A glass conservatory", "count": 1, "quality": "max" }),
        },
        Case {
            name: "generate_style_natural",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A harbour at slack tide", "count": 1, "style": "natural" }),
        },
        Case {
            name: "generate_aspect_ratio_ok",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A long viaduct", "count": 1, "aspectRatio": "16:9" }),
        },
        // MEASURED, not assumed: the ROUTE's `aspectRatio` is `z.string()` —
        // any string — while the TOOL's is
        // `z.enum(['1:1','3:4','4:3','9:16','16:9'])`. So `3:2` passes the
        // route, REACHES the tool (the toolInput row proves it) and is refused
        // there, reported through v4's one blanket sentence, which names
        // `prompt` and means "the whole object". A route that pre-gated the
        // key on the tool's enum would answer the Zod envelope instead — this
        // row is what forbids that shortcut.
        Case {
            name: "generate_aspect_ratio_off_tool_enum",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A long viaduct", "count": 1, "aspectRatio": "3:2" }),
        },
        Case {
            name: "generate_negative_prompt",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A milliner at work", "count": 1, "negativePrompt": "no hats" }),
        },
        Case {
            name: "generate_all_five",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A zeppelin over the fens", "chatId": CHAT_PLAIN, "count": 1, "size": "1024x1536", "quality": "high", "style": "vivid", "aspectRatio": "16:9", "negativePrompt": "no wires" }),
        },
        // ---- the refusal arms: `.optional()` is NOT `.nullable()`,
        // `z.string()` rejects every non-string, and `z.enum` /
        // `imageQualitySchema` reject an unknown member. v5 answered 201
        // having silently ignored the key.
        Case {
            name: "generate_quality_unknown",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "quality": "ultra" }),
        },
        Case {
            name: "generate_style_unknown",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "style": "sketch" }),
        },
        Case {
            name: "generate_size_number",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "size": 1024 }),
        },
        Case {
            name: "generate_size_null",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "size": Value::Null }),
        },
        Case {
            name: "generate_negative_prompt_object",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "negativePrompt": {} }),
        },
        Case {
            name: "generate_aspect_ratio_bool",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "aspectRatio": true }),
        },
        // The PRE-EXISTING serde-typed class, recorded as it stands: `count` is
        // `Option<i64>` on the dispatch variant, so the WEB EDGE refuses a
        // string before this handler is reached. The handler itself is
        // v4-faithful, which is what this row pins.
        Case {
            name: "generate_count_string",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": "2" }),
        },
        Case {
            name: "generate_count_zero",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 0 }),
        },
        Case {
            name: "generate_count_fractional",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1.5 }),
        },
        // `prompt: z.string()` against a number — likewise the handler's arm;
        // the variant types `prompt` as a required `String`.
        Case {
            name: "generate_prompt_number",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": 5, "count": 1 }),
        },
        // `chatId: z.uuid().optional()` — v5 never gated this key AT ALL until
        // P4.96: a non-uuid chat id reached the tool.
        Case {
            name: "generate_chat_id_null",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "chatId": Value::Null }),
        },
        Case {
            name: "generate_chat_id_number",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "chatId": 7 }),
        },
        Case {
            name: "generate_chat_id_not_uuid",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "chatId": "not-a-uuid" }),
        },
        // Zod collects EVERY failing key into one issue array, in schema order
        // (`quality` is declared before `style`).
        Case {
            name: "generate_two_bad_fields",
            id: spec.profile_id.clone(),
            body: json!({ "prompt": "A kite", "count": 1, "quality": "ultra", "style": "sketch" }),
        },
        // …and across three DIFFERENT issue kinds, in the schema's DECLARATION
        // order — not the body's key order, which is reversed here.
        Case {
            name: "generate_three_bad_ordered",
            id: spec.profile_id.clone(),
            body: json!({ "size": 1, "count": 99, "prompt": "" }),
        },
        // Guard order: v4 reads the body only AFTER `findById`, so a missing
        // profile with a garbage body is a 404, never a 400.
        Case {
            name: "generate_404_beats_400",
            id: BOGUS_PROFILE.to_string(),
            body: json!({ "prompt": 5, "quality": "ultra", "size": Value::Null }),
        },
    ];

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut failed: Vec<String> = Vec::new();
    for case in &cases {
        let (main_work, mount_work) = fresh_copy(&main_fixture, &mount_fixture, case.name);
        // A fresh per-case llm-logs partition for the handler's fire-and-forget
        // IMAGE_GENERATION row (never diffed here; kept so the write succeeds).
        let ll_work = main_work.with_file_name(format!("imggenroute-ll-{}.db", case.name));
        let _ = std::fs::remove_file(&ll_work);
        common::materialize_llm_logs(&ll_work, &spec.test_pepper_base64);

        let db = Db::open(
            DbPaths {
                main: main_work.clone(),
                mount_index: Some(mount_work.clone()),
                llm_logs: Some(ll_work.clone()),
            },
            &spec.test_pepper_base64,
        )
        .unwrap_or_else(|e| panic!("open fixture {}: {e}", case.name));

        *recorded_input.lock().expect("recorded_input") = None;
        let resp = rt.block_on(image_profile_generate(
            &db,
            &runner,
            &spec.user_id,
            &case.id,
            &body_of(case),
        ));

        let want = results
            .get(case.name)
            .unwrap_or_else(|| panic!("oracle missing case {}", case.name));
        let (got_status, got_body) = envelope_and_status(&resp);

        // Success cases compare BODY only (the dispatch surface answers 200 for v4's
        // 201 — the P4.6p precedent); error cases compare status + body.
        let is_error = matches!(resp, Response::Error(_));
        if is_error && got_status != want.status {
            eprintln!(
                "[{}] STATUS diverged: rust {} vs oracle {}",
                case.name, got_status, want.status
            );
            failed.push(case.name.to_string());
        }
        // P4.96: the tool-input side channel. `null` on BOTH sides means the
        // route refused before the tool — which is itself the comparand for
        // every refusal row (a v5 that silently dropped a bad `quality` would
        // record an input where v4 records none).
        let got_input = recorded_input
            .lock()
            .expect("recorded_input")
            .clone()
            .unwrap_or(Value::Null);
        let want_input = tool_inputs
            .get(case.name)
            .unwrap_or_else(|| panic!("oracle missing toolInput for case {}", case.name));
        if norm(&got_input) != norm(want_input) {
            eprintln!(
                "[{}] TOOL INPUT diverged:\n{}",
                case.name,
                first_diff(&norm(&got_input), &norm(want_input))
            );
            failed.push(case.name.to_string());
        }

        let got_n = norm(&got_body);
        let want_n = norm(&want.body);
        if got_n != want_n {
            eprintln!(
                "[{}] BODY diverged:\n{}",
                case.name,
                first_diff(&got_n, &want_n)
            );
            failed.push(case.name.to_string());
        } else {
            eprintln!(
                "[{}] OK (status rust={got_status} oracle={}).",
                case.name, want.status
            );
        }

        drop(db);
        cleanup(&main_work, &mount_work);
        let _ = std::fs::remove_file(&ll_work);
    }

    // The ONE-list rule, made executable: every oracle row must have a Rust
    // case and vice versa. A row added to the .test.ts alone would otherwise
    // simply never run (`a-case-added-only-to-the-oracle-is-never-run`).
    let rust_names: std::collections::BTreeSet<&str> = cases.iter().map(|c| c.name).collect();
    let oracle_names: std::collections::BTreeSet<&str> =
        results.keys().map(String::as_str).collect();
    assert_eq!(
        rust_names, oracle_names,
        "the Rust case list and the oracle case list must match name for name"
    );
    // The floor the order sets: 8 pre-P4.96 rows + at least 12 new ones.
    assert!(
        cases.len() >= 20,
        "expected >= 20 cases, found {}",
        cases.len()
    );

    assert!(
        failed.is_empty(),
        "image-generate-route differential FAILED: {failed:?}"
    );
    eprintln!("OK: image-generate-route envelope matched the oracle across all cases.");
}
