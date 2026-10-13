//! Tier-3 differential for the shared image attempt + provider-image decode
//! (P4.D283, v4 `9ecebf986`): `quilltap_core::image_gen::image_attempt::
//! {make_logged_image_attempt, decode_provider_image}` vs v4's REAL
//! `makeLoggedImageAttempt` / `decodeProviderImage` (`lib/image-gen/
//! image-attempt.ts`), the image provider mocked BELOW both.
//!
//! Per attempt case both sides take a fresh copy of the wardrobe-item-image-
//! generation fixture pair (the owner, `chat_settings` with logging ON, the
//! image profiles + api keys), build ONE attempt from the corpus's options
//! (`harness/oracle/fixtures/image-attempt.json`) and run its scripted steps —
//! each against a REAL profile row, with the mocked provider answering the
//! fixture WebP (with / without a revised prompt) or throwing — then compare:
//!   - the ordered provider calls `{provider, apiKey, params}` (the shared
//!     builder's output per profile: the PREBUILT params reused for their own
//!     profile only, every other profile built with its own LoRAs and options);
//!   - each step's outcome (the answer's image count + revised prompt, or the
//!     rethrown message);
//!   - the `[ImageAttempt] Calling image provider` lines (`rerouted` keyed on
//!     `≠ primaryProfileId`, `reusedPrebuiltParams`);
//!   - the `llm_logs` rows — ONE per step under the asked log type, `chatId` /
//!     `characterId` / `imageProfileId`, the request's one user message, the
//!     success content (`revisedPrompt || Generated N image(s)[ (Concierge
//!     reroute)]`) or `content: ''` + `error` (id / timestamps / `durationMs`
//!     placeholdered by the shared helpers).
//!
//! Per decode case: `decode_provider_image` over the REAL codec
//! (`quilltap_host::HostImageCodec` — v4's `sharp` `convertToWebP`) — the
//! stored mime / bytes / sha256 / measured dimensions / revised prompt, the
//! minted filename with its `Date.now()` masked `<ms>`, or `None` with the
//! `Provider response carried no image data` DEBUG (`index`, `imageCount`,
//! `hasUrl`). v4's `b64Json` spelling is folded into `data` on this side, as
//! the decoders do.
//!
//! NEGATIVE pins: v4's `[ImageAttempt] Failed to record the image call in
//! llm_logs` WARN guards a `logLLMCall` throw that `logLLMCall` itself
//! swallows — unreachable on both sides, so no `[ImageAttempt]` WARN ever
//! appears in either capture.
//!
//! Regenerate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-ia-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/image-attempt.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/image-attempt.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WIIG_MAIN=/tmp/qt-ia-main.db QT_FIXTURE_WIIG_MOUNT=/tmp/qt-ia-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-image-generation-fixture.ts
//!   QT_FIXTURE_IA_MAIN=/tmp/qt-ia-main.db QT_FIXTURE_IA_MOUNT=/tmp/qt-ia-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-image-attempt.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/image-attempt\.test\.ts$"
//!
//! Run:
//!   QT_ORACLE_IMAGE_ATTEMPT=/tmp/oracle-image-attempt.ndjson \
//!   QT_FIXTURE_IA_MAIN=/tmp/qt-ia-main.db QT_FIXTURE_IA_MOUNT=/tmp/qt-ia-mount.db \
//!     cargo test -p quilltap-harness --test image_attempt_tier3_equivalence

mod common;
mod wardrobe_images_support;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::image_gen::image_attempt::{
    decode_provider_image, make_logged_image_attempt, ImageAttemptParams,
    LoggedImageAttemptOptions, PixelCodecTranscoder, PrebuiltImageParams, DEFAULT_IMAGE_MODEL,
};
use quilltap_core::image_gen::params_builder::{ImageGenOverrides, ImageParamsLogContext};
use quilltap_core::image_gen::Orientation;
use quilltap_core::model::image::{
    GeneratedImageData, ImageGenError, ImageGenParams, ImageGenResponse, ImageProvider,
};
use quilltap_core::services::dangerous_content::image_failover::FailoverProfile;
use quilltap_core::test_support::captured_with;
use serde_json::{json, Value};
use wardrobe_images_support::{oracle_lines, rust_lines};

const LOG_PREFIXES: [&str; 1] = ["[ImageAttempt]"];

/// The scripted provider: records every call, answers per the step's mode.
struct ScriptedProvider {
    webp: String,
    steps: Vec<String>,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl ImageProvider for ScriptedProvider {
    async fn generate_image(
        &self,
        provider: &str,
        api_key: &str,
        params: &ImageGenParams,
    ) -> Result<ImageGenResponse, ImageGenError> {
        let n = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(json!({
                "provider": provider,
                "apiKey": api_key,
                "params": params.to_key_value(),
            }));
            calls.len()
        };
        match self.steps[n - 1].as_str() {
            "throw" => Err(ImageGenError::new("canned provider failure")),
            mode => Ok(ImageGenResponse {
                images: vec![GeneratedImageData {
                    data: Some(self.webp.clone()),
                    url: None,
                    mime_type: Some("image/webp".into()),
                    revised_prompt: (mode == "ok").then(|| "a revised coat".to_string()),
                }],
            }),
        }
    }
}

fn orientation_of(s: Option<&str>) -> Option<Orientation> {
    match s {
        Some("portrait") => Some(Orientation::Portrait),
        Some("landscape") => Some(Orientation::Landscape),
        Some("square") => Some(Orientation::Square),
        _ => None,
    }
}

/// The corpus's `overrides` bag → the builder's overrides (the keys the corpus
/// uses; a new key is a corpus growth here too).
fn overrides_of(v: &Value) -> ImageGenOverrides {
    ImageGenOverrides {
        n: v.get("n").and_then(Value::as_f64),
        style: v.get("style").and_then(Value::as_str).map(str::to_string),
        quality: v.get("quality").and_then(Value::as_str).map(str::to_string),
        size: v.get("size").and_then(Value::as_str).map(str::to_string),
        ..Default::default()
    }
}

/// The corpus's prebuilt params object → `ImageGenParams` (the keys the corpus
/// uses).
fn params_of(v: &Value) -> ImageGenParams {
    ImageGenParams {
        prompt: v["prompt"].as_str().unwrap_or("").to_string(),
        model: v["model"].as_str().unwrap_or("").to_string(),
        n: v.get("n").and_then(Value::as_f64),
        size: v.get("size").and_then(Value::as_str).map(str::to_string),
        quality: v.get("quality").and_then(Value::as_str).map(str::to_string),
        ..Default::default()
    }
}

/// `images[]` from the corpus: `$webp` / `$png1x1` resolved, `b64Json` folded
/// into `data` (the decoders' fold).
fn response_of(spec: &Value, v: &Value) -> Option<ImageGenResponse> {
    let images = v.get("images")?.as_array()?;
    let resolve = |s: &str| -> String {
        match s {
            "$webp" => spec["webp"].as_str().unwrap().to_string(),
            "$png1x1" => spec["png1x1"].as_str().unwrap().to_string(),
            other => other.to_string(),
        }
    };
    Some(ImageGenResponse {
        images: images
            .iter()
            .map(|img| GeneratedImageData {
                data: img
                    .get("data")
                    .or_else(|| img.get("b64Json"))
                    .and_then(Value::as_str)
                    .map(resolve),
                url: img.get("url").and_then(Value::as_str).map(str::to_string),
                mime_type: img
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                revised_prompt: img
                    .get("revisedPrompt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
            .collect(),
    })
}

/// A JS number: an integral `f64` prints as an integer (`4`, never `4.0`).
fn js_num(v: Option<f64>) -> Value {
    match v {
        None => Value::Null,
        Some(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(f as i64),
        Some(f) => json!(f),
    }
}

fn mask_ms(s: &str) -> String {
    regex::Regex::new(r"_\d+\.")
        .unwrap()
        .replace(s, "_<ms>.")
        .into_owned()
}

#[test]
fn image_attempt_tier3_matches_oracle() {
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_IMAGE_ATTEMPT"),
        std::env::var("QT_FIXTURE_IA_MAIN"),
        std::env::var("QT_FIXTURE_IA_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_IMAGE_ATTEMPT + QT_FIXTURE_IA_MAIN + QT_FIXTURE_IA_MOUNT (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/image-attempt.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let user_id = spec["userId"].as_str().unwrap().to_string();
    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    let attempts = spec["attempts"].as_array().unwrap();
    let decodes = spec["decodes"].as_array().unwrap();
    assert_eq!(
        oracle.len(),
        attempts.len() + decodes.len(),
        "oracle row count != corpus"
    );

    let mut failures: Vec<String> = Vec::new();
    let declarations = quilltap_core::image_gen_data::image_declarations_for;

    for a in attempts {
        let name = a["name"].as_str().unwrap();
        let want = &oracle[name];
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-ia-rust-{name}-"))
            .tempdir()
            .unwrap();
        let (mw, nw, lw) = (
            scratch.path().join("main.db"),
            scratch.path().join("mount.db"),
            scratch.path().join("llm-logs.db"),
        );
        std::fs::copy(&main_fixture, &mw).unwrap();
        std::fs::copy(&mount_fixture, &nw).unwrap();
        common::materialize_llm_logs(&lw, &pepper);
        // The profile rows, read REAL off the fixture.
        let profile_rows: HashMap<String, Value> = {
            let main = Writer::open_writable(&mw, &pepper).unwrap();
            spec["profiles"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, id)| {
                    let row = quilltap_core::db::image_profiles::find_by_id(
                        main.connection(),
                        id.as_str().unwrap(),
                    )
                    .unwrap()
                    .unwrap_or_else(|| panic!("profile {k} missing from the fixture"));
                    (k.clone(), row)
                })
                .collect()
        };
        let db = Db::open(
            DbPaths {
                main: mw.clone(),
                mount_index: Some(nw.clone()),
                llm_logs: Some(lw.clone()),
            },
            &pepper,
        )
        .unwrap();
        let steps = a["steps"].as_array().unwrap();
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let provider = ScriptedProvider {
            webp: spec["webp"].as_str().unwrap().to_string(),
            steps: steps
                .iter()
                .map(|s| s["provider"].as_str().unwrap().to_string())
                .collect(),
            calls: Arc::clone(&calls),
        };
        let o = &a["options"];
        let lc = &o["params"]["logContext"];
        let context: &'static str =
            Box::leak(lc["context"].as_str().unwrap().to_string().into_boxed_str());
        let log_type: &'static str = match o["logType"].as_str().unwrap() {
            "WARDROBE_ITEM_IMAGE" => {
                quilltap_core::services::llm_logging::log_type::WARDROBE_ITEM_IMAGE
            }
            "IMAGE_GENERATION" => quilltap_core::services::llm_logging::log_type::IMAGE_GENERATION,
            other => panic!("unknown log type {other}"),
        };
        let primary_id = spec["profiles"][o["primaryProfile"].as_str().unwrap()]
            .as_str()
            .unwrap()
            .to_string();
        let prompt = o["prompt"].as_str().unwrap().to_string();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (outcomes, lines) = captured_with(|| {
            let mut attempt = make_logged_image_attempt(
                &db,
                &provider,
                LoggedImageAttemptOptions {
                    user_id: &user_id,
                    log_type,
                    prompt: &prompt,
                    primary_profile_id: &primary_id,
                    chat_id: o.get("chatId").and_then(Value::as_str),
                    character_id: o.get("characterId").and_then(Value::as_str),
                    params: ImageAttemptParams {
                        overrides: overrides_of(&o["params"]["overrides"]),
                        orientation: orientation_of(o["params"]["orientation"].as_str()),
                        log_context: ImageParamsLogContext {
                            context,
                            chat_id: lc.get("chatId").and_then(Value::as_str).map(str::to_string),
                            job_id: lc.get("jobId").and_then(Value::as_str).map(str::to_string),
                            profile_id: None,
                            rerouted: None,
                        },
                        fallback_model: DEFAULT_IMAGE_MODEL,
                        declarations_for: &declarations,
                    },
                    prebuilt: o.get("prebuilt").map(|p| PrebuiltImageParams {
                        profile_id: spec["profiles"][p["profile"].as_str().unwrap()]
                            .as_str()
                            .unwrap()
                            .to_string(),
                        params: params_of(&p["params"]),
                    }),
                },
            );
            let mut outcomes = Vec::new();
            for step in steps {
                let profile =
                    FailoverProfile::from_row(&profile_rows[step["profile"].as_str().unwrap()]);
                let key = step["apiKey"].as_str().unwrap().to_string();
                let r = rt.block_on(attempt.call(profile, key));
                outcomes.push(match r {
                    Ok(resp) => json!({
                        "ok": true,
                        "imageCount": resp.images.len(),
                        "revisedPrompt": resp.images.first().and_then(|i| i.revised_prompt.clone()),
                    }),
                    Err(e) => json!({ "ok": false, "threw": e.message }),
                });
            }
            outcomes
        });
        let got = json!({
            "providerCalls": calls.lock().unwrap().clone(),
            "outcomes": outcomes,
            "logs": rust_lines(&lines, &LOG_PREFIXES),
            "llmLogs": common::dump_llm_logs(&db),
        });
        let want_s = json!({
            "providerCalls": want["providerCalls"],
            "outcomes": want["outcomes"],
            "logs": oracle_lines(&want["logs"]),
            "llmLogs": common::oracle_llm_logs(&want["llmLogs"]),
        });
        for part in ["providerCalls", "outcomes", "logs", "llmLogs"] {
            if got[part] != want_s[part] {
                failures.push(format!(
                    "{name}: {part} diverged\n  rust:   {}\n  oracle: {}",
                    got[part], want_s[part]
                ));
            }
        }
        // The negative pin: no `[ImageAttempt]` WARN on either side.
        let warned = |ls: &[String]| ls.iter().any(|l| l.starts_with("WARN "));
        assert!(
            !warned(&rust_lines(&lines, &LOG_PREFIXES)),
            "{name}: a v5 [ImageAttempt] WARN"
        );
        assert!(
            !warned(&oracle_lines(&want["logs"])),
            "{name}: a v4 [ImageAttempt] WARN"
        );
    }

    let codec = quilltap_host::HostImageCodec;
    for d in decodes {
        let name = d["name"].as_str().unwrap();
        let want = &oracle[name];
        let response = response_of(&spec, &d["response"]);
        let stem = d["stem"].as_str().unwrap();
        let index = d["index"].as_u64().unwrap() as usize;
        let (decoded, lines) = captured_with(|| {
            decode_provider_image(
                &PixelCodecTranscoder(&codec),
                response.as_ref(),
                stem,
                index,
                1_700_000_000_000,
            )
        });
        let got = json!({
            "decoded": decoded.map(|x| json!({
                "mimeType": x.mime_type,
                "filename": mask_ms(&x.filename),
                "width": js_num(x.width),
                "height": js_num(x.height),
                "sha256": x.sha256,
                "bytesHex": x.buffer.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                "revisedPrompt": x.revised_prompt,
            })),
            "logs": rust_lines(&lines, &LOG_PREFIXES).iter().map(|l| mask_ms(l)).collect::<Vec<_>>(),
        });
        let want_s = json!({
            "decoded": want["decoded"],
            "logs": oracle_lines(&want["logs"]).iter().map(|l| mask_ms(l)).collect::<Vec<_>>(),
        });
        for part in ["decoded", "logs"] {
            if got[part] != want_s[part] {
                failures.push(format!(
                    "{name}: {part} diverged\n  rust:   {}\n  oracle: {}",
                    got[part], want_s[part]
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
