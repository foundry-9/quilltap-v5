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

use quilltap_core::api::image_profiles::image_profile_generate;
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
use serde_json::Value;

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

/// One test case (mirrors the oracle's `cases`).
struct Case {
    name: &'static str,
    id: String,
    prompt: &'static str,
    chat_id: Option<&'static str>,
    count: Option<i64>,
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
        if i + 36 <= bytes.len() {
            let cand = &s[i..i + 36];
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
    let mut v = v.clone();
    // v4's `validationError` carries Zod's `details` issue array beside the
    // fixed `error`; v5's `Validation error` envelope omits it everywhere (the
    // autonomous-rooms / characters precedent), so the array is dropped from
    // the comparand — the sentence and the status are what is pinned.
    if v.get("error").and_then(Value::as_str) == Some("Validation error") {
        if let Some(obj) = v.as_object_mut() {
            obj.remove("details");
        }
    }
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
            (status, serde_json::json!({ "error": e.message }))
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
            _ => {}
        }
    }

    // One runner over the canned seams (the executor is rebuilt per run inside it).
    let runner = ErasedImageGeneration::new(TestImageRunner {
        image_provider: RealImageProvider::new(image_transport),
        completion: CannedCompletionProvider::new(),
        api_keys: CannedApiKeys(spec.api_keys.clone()),
        now_ms: spec.frozen_now_ms,
    });

    let cases = [
        Case {
            name: "generate_happy_chat",
            id: spec.profile_id.clone(),
            prompt: "A serene mountain lake at dawn, mist over the water",
            chat_id: Some(CHAT_PLAIN),
            count: Some(1),
        },
        Case {
            name: "generate_no_chat",
            id: spec.profile_id.clone(),
            prompt: "A quiet forest path",
            chat_id: None,
            count: Some(1),
        },
        Case {
            name: "generate_count2",
            id: spec.profile_id.clone(),
            prompt: "A tall waterfall in a canyon",
            chat_id: Some(CHAT_ORIENT),
            count: Some(2),
        },
        Case {
            name: "generate_profile_404",
            id: BOGUS_PROFILE.to_string(),
            prompt: "anything",
            chat_id: None,
            count: Some(1),
        },
        // The route's own `generateImageSchema` gate (v4 `route.ts:242`), which
        // runs after the 404 and before the tool — the §3 unification review of
        // the follow-ups round: v5 handed `count: 20` to the TOOL's schema and
        // answered its fixed sentence where v4 answers `Validation error`.
        Case {
            name: "generate_count_over_max",
            id: spec.profile_id.clone(),
            prompt: "A lighthouse at night",
            chat_id: None,
            count: Some(20),
        },
        Case {
            name: "generate_prompt_empty",
            id: spec.profile_id.clone(),
            prompt: "",
            chat_id: None,
            count: Some(1),
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

        let resp = rt.block_on(image_profile_generate(
            &db,
            &runner,
            &spec.user_id,
            &case.id,
            case.prompt,
            case.chat_id,
            case.count,
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

    assert!(
        failed.is_empty(),
        "image-generate-route differential FAILED: {failed:?}"
    );
    eprintln!("OK: image-generate-route envelope matched the oracle across all cases.");
}
