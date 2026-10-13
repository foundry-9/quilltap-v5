//! Tier-3 differential for the shared vision request (P4.D283, v4 `9ecebf986` —
//! bug 197's half where v5 has the surface): `quilltap_core::llm::vision_request::
//! {resolve_vision_profile, send_vision_request}` vs v4's REAL
//! `resolveVisionProfile` / `sendVisionRequest` (`lib/llm/vision-request.ts`),
//! the LLM provider mocked BELOW both.
//!
//! Per case both sides take a fresh copy of the file-attachment fixture pair
//! (the describer profiles — OPENAI cheap, Z_AI, OLLAMA llava whose plugin
//! cannot transport images; every profile keyless; `chat_settings` naming the
//! OPENAI describer; an INCAPABLE profile is llava with `supportsImageUpload = 0`
//! planted by the case's `sql`). The `resolve` cases run the pick after the
//! case's `sql` and compare the picked profile id and the `[Vision]` lines. The
//! `send` cases run one request with the provider scripted (`ok` / `throw` /
//! `hang`) and compare:
//!   - the result (`ok` with the content, usage and the SENT attachment's
//!     filename + mime; or the refusal `{refusal, detail}`; or the rethrown
//!     message — incl. v4's `Vision request timed out after <ms>ms`);
//!   - the params the provider was handed (model, temperature, maxTokens,
//!     topP, profileParameters, the messages' roles / contents / attachments'
//!     filename + mime) — the `request` comparand;
//!   - the `[Vision]` lines (`durationMs` placeholdered);
//!   - the `llm_logs` rows — ONE per send under the asked type, answered or
//!     not, the request carrying the attachment and the sampling on BOTH arms
//!     (id / timestamps / `durationMs` placeholdered by the shared helpers).
//!
//! Most cases carry a 1x1 PNG under the transport shrink's bounds (`shrunk:
//! false` on both sides, through the real codecs); `ok_shrunk_scripted` runs
//! the bug-151 SCRIPTED encoder on both sides (D19: the shrink's decision and
//! its `(maxEdge, quality)` ladder are comparands, encoded bytes are not), so
//! the Tier-2 "shrink skipped" mutation turns it RED. The shrink policy itself
//! is `llm_image_budget`'s family.
//!
//! `hasData` on the provider's attachment is normalized as "a non-empty
//! string": v4 leaves `data` undefined for a plugin-loaded attachment where
//! v5's `CompletionAttachment.data` is the empty string.
//!
//! NEGATIVE pin: v4's `[Vision] Failed to record the vision call in llm_logs`
//! WARN guards a `logLLMCall` throw that `logLLMCall` swallows — unreachable on
//! both sides, so no `[Vision]` WARN but `Model answered without the image…`
//! appears in either capture.
//!
//! Regenerate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-vr-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
//!   cp "$V5W/harness/oracle/cases/vision-request.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/lib/shrink-script.ts" "$TMPO/lib/"
//!   cp "$V5W/harness/oracle/fixtures/vision-request.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-vr-main.db QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-vr-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-file-attachment-fixture.ts
//!   QT_FIXTURE_VR_MAIN=/tmp/qt-vr-main.db QT_FIXTURE_VR_MOUNT=/tmp/qt-vr-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-vision-request.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/vision-request\.test\.ts$"
//!
//! Run:
//!   QT_ORACLE_VISION_REQUEST=/tmp/oracle-vision-request.ndjson \
//!   QT_FIXTURE_VR_MAIN=/tmp/qt-vr-main.db QT_FIXTURE_VR_MOUNT=/tmp/qt-vr-mount.db \
//!     cargo test -p quilltap-harness --test vision_request_tier3_equivalence

mod common;
mod wardrobe_images_support;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::llm::vision_request::{
    resolve_vision_profile, send_vision_request, ConfiguredVisionProfile,
    ResolveVisionProfileOptions, VisionPreference, VisionRequestInput, VisionRequestResult,
    VisionSampling,
};
use quilltap_core::model::completion::{
    CompletionAttachment, CompletionError, CompletionParams, CompletionProvider,
    CompletionResponse, CompletionRole, CompletionUsage,
};
use quilltap_core::model::stream::{
    StreamAttachmentFailure, StreamAttachmentResults, StreamCacheUsage,
};
use quilltap_core::test_support::captured_with;
use quilltap_harness::scripted_transcoder::{original_bytes, CorpusScript, ScriptedTranscoder};
use serde_json::{json, Value};
use wardrobe_images_support::{oracle_lines, rust_lines};

const LOG_PREFIXES: [&str; 1] = ["[Vision]"];

/// The scripted provider: records the params it was handed, answers per mode.
struct ScriptedCompletion {
    script: Value,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl CompletionProvider for ScriptedCompletion {
    async fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        self.send_message_keyed(provider, base_url, "", params)
            .await
    }

    async fn send_message_keyed(
        &self,
        provider: &str,
        base_url: Option<&str>,
        api_key: &str,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        // v4's `sendMessage(messageParams, apiKeyValue)` — the recorded shape
        // is the oracle's `sanitize(params)` (keys in v4's assignment order).
        let mut recorded = serde_json::Map::new();
        recorded.insert("model".into(), json!(params.model));
        recorded.insert(
            "messages".into(),
            Value::Array(
                params
                    .messages
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        let mut mm = json!({
                            "role": match m.role {
                                CompletionRole::System => "system",
                                CompletionRole::User => "user",
                                CompletionRole::Assistant => "assistant",
                                CompletionRole::Tool => "tool",
                            },
                            "content": m.content,
                        });
                        // The attachments ride the LAST (user) message.
                        if i + 1 == params.messages.len() {
                            mm["attachments"] = Value::Array(
                                params
                                    .attachments
                                    .iter()
                                    .map(|a| json!({ "id": a.id, "filename": a.filename, "mimeType": a.mime_type, "data": a.data }))
                                    .collect(),
                            );
                        }
                        mm
                    })
                    .collect(),
            ),
        );
        if let Some(t) = params.temperature {
            recorded.insert("temperature".into(), json!(t));
        }
        if let Some(m) = params.max_tokens {
            recorded.insert("maxTokens".into(), json!(m));
        }
        if let Some(p) = params.top_p {
            recorded.insert("topP".into(), json!(p));
        }
        if let Some(pp) = &params.profile_parameters {
            recorded.insert("profileParameters".into(), pp.clone());
        }
        self.calls.lock().unwrap().push(json!({
            "provider": provider,
            "baseUrl": base_url,
            "apiKey": api_key,
            "params": Value::Object(recorded),
        }));
        match self.script["mode"].as_str().unwrap_or("ok") {
            "throw" => Err(CompletionError::new(
                self.script["message"]
                    .as_str()
                    .unwrap_or("canned vision failure"),
            )),
            "hang" => std::future::pending::<Result<CompletionResponse, CompletionError>>().await,
            _ => {
                Ok(CompletionResponse {
                    content: self.script["content"].as_str().unwrap_or("").to_string(),
                    usage: self.script.get("usage").filter(|u| !u.is_null()).map(|u| {
                        CompletionUsage {
                            prompt_tokens: u["promptTokens"].as_i64().unwrap_or(0),
                            completion_tokens: u["completionTokens"].as_i64().unwrap_or(0),
                            total_tokens: u["totalTokens"].as_i64().unwrap_or(0),
                        }
                    }),
                    finish_reason: self.script["finishReason"].as_str().map(str::to_string),
                    attachment_results: self
                        .script
                        .get("attachmentResults")
                        .filter(|a| !a.is_null())
                        .map(|a| StreamAttachmentResults {
                            sent: a["sent"]
                                .as_array()
                                .map(|s| {
                                    s.iter()
                                        .filter_map(Value::as_str)
                                        .map(str::to_string)
                                        .collect()
                                })
                                .unwrap_or_default(),
                            failed: a["failed"]
                                .as_array()
                                .map(|f| {
                                    f.iter()
                                        .map(|x| StreamAttachmentFailure {
                                            id: x["id"].as_str().unwrap_or("").to_string(),
                                            error: x["error"].as_str().unwrap_or("").to_string(),
                                        })
                                        .collect()
                                })
                                .unwrap_or_default(),
                        }),
                    cache_usage: self
                        .script
                        .get("cacheUsage")
                        .filter(|c| !c.is_null())
                        .map(|c| StreamCacheUsage {
                            cached_tokens: c.get("cachedTokens").and_then(Value::as_i64),
                            cache_read_input_tokens: c
                                .get("cacheReadInputTokens")
                                .and_then(Value::as_i64),
                            cache_creation_input_tokens: c
                                .get("cacheCreationInputTokens")
                                .and_then(Value::as_i64),
                            ..Default::default()
                        }),
                })
            }
        }
    }
}

/// `durationMs=<n>` → `durationMs=<ms>` (a wall clock).
fn blank_duration(lines: Vec<String>) -> Vec<String> {
    let re = regex::Regex::new(r"durationMs=\d+").unwrap();
    lines
        .into_iter()
        .map(|l| re.replace_all(&l, "durationMs=<ms>").into_owned())
        .collect()
}

/// The oracle recorded `sanitize(params)` — the Node-side object; the keys the
/// Rust recorder does not know are dropped from BOTH sides' comparand.
fn comparable_calls(calls: &Value) -> Value {
    Value::Array(
        calls
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let p = &c["params"];
                let messages: Vec<Value> = p["messages"]
                    .as_array()
                    .map(|ms| {
                        ms.iter()
                            .map(|m| {
                                let mut mm = json!({ "role": m["role"], "content": m["content"] });
                                if let Some(atts) = m.get("attachments").and_then(Value::as_array) {
                                    mm["attachments"] = Value::Array(
                                        atts.iter()
                                            .map(|a| json!({ "id": a["id"], "filename": a["filename"], "mimeType": a["mimeType"], "hasData": a.get("data").and_then(Value::as_str).is_some_and(|d| !d.is_empty()) }))
                                            .collect(),
                                    );
                                }
                                mm
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut params = json!({ "model": p["model"], "messages": messages });
                for k in ["temperature", "maxTokens", "topP", "profileParameters"] {
                    if let Some(v) = p.get(k) {
                        params[k] = v.clone();
                    }
                }
                json!({ "provider": c["provider"], "baseUrl": c["baseUrl"], "apiKey": c["apiKey"], "params": params })
            })
            .collect(),
    )
}

fn open(
    main_fixture: &str,
    mount_fixture: &str,
    pepper: &str,
    name: &str,
    sql: &[Value],
) -> (Db, tempfile::TempDir) {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-vr-rust-{name}-"))
        .tempdir()
        .unwrap();
    let (mw, nw, lw) = (
        scratch.path().join("main.db"),
        scratch.path().join("mount.db"),
        scratch.path().join("llm-logs.db"),
    );
    std::fs::copy(main_fixture, &mw).unwrap();
    std::fs::copy(mount_fixture, &nw).unwrap();
    common::materialize_llm_logs(&lw, pepper);
    {
        let main = Writer::open_writable(&mw, pepper).unwrap();
        for s in sql {
            main.connection()
                .execute_batch(s.as_str().unwrap())
                .unwrap();
        }
    }
    let db = Db::open(
        DbPaths {
            main: mw,
            mount_index: Some(nw),
            llm_logs: Some(lw),
        },
        pepper,
    )
    .unwrap();
    (db, scratch)
}

#[test]
fn vision_request_tier3_matches_oracle() {
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_VISION_REQUEST"),
        std::env::var("QT_FIXTURE_VR_MAIN"),
        std::env::var("QT_FIXTURE_VR_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_VISION_REQUEST + QT_FIXTURE_VR_MAIN + QT_FIXTURE_VR_MOUNT (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/vision-request.json"),
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
    let resolves = spec["resolve"].as_array().unwrap();
    let sends = spec["send"].as_array().unwrap();
    assert_eq!(
        oracle.len(),
        resolves.len() + sends.len(),
        "oracle row count != corpus"
    );
    let codec = quilltap_host::HostImageCodec;
    let mut failures: Vec<String> = Vec::new();

    for c in resolves {
        let name = c["name"].as_str().unwrap();
        let want = &oracle[name];
        let (db, _scratch) = open(
            &main_fixture,
            &mount_fixture,
            &pepper,
            name,
            c.get("sql")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        let options = ResolveVisionProfileOptions {
            prefer: match c["prefer"].as_str().unwrap() {
                "cheap" => VisionPreference::Cheap,
                _ => VisionPreference::Capable,
            },
            configured: match c["configured"].as_str().unwrap() {
                "honour" => ConfiguredVisionProfile::Honour,
                _ => ConfiguredVisionProfile::SkipIncapable,
            },
        };
        let (picked, lines) = captured_with(|| resolve_vision_profile(&db, &user_id, options));
        let got = json!({
            "profileId": picked.unwrap().and_then(|p| p.get("id").cloned()),
            "logs": rust_lines(&lines, &LOG_PREFIXES),
        });
        let want_s = json!({ "profileId": want["profileId"], "logs": oracle_lines(&want["logs"]) });
        for part in ["profileId", "logs"] {
            if got[part] != want_s[part] {
                failures.push(format!(
                    "{name}: {part} diverged\n  rust:   {}\n  oracle: {}",
                    got[part], want_s[part]
                ));
            }
        }
    }

    for c in sends {
        let name = c["name"].as_str().unwrap();
        let want = &oracle[name];
        let (db, _scratch) = open(
            &main_fixture,
            &mount_fixture,
            &pepper,
            name,
            c.get("sql")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        let profile_id = c["profile"].as_str().unwrap().to_string();
        let profile = db
            .read_main(move |conn| {
                quilltap_core::db::connection_profiles::find_by_id(conn, &profile_id)
            })
            .unwrap()
            .expect("profile in the fixture");
        let att = &c["attachment"];
        // The one `shrink` case runs the bug-151 SCRIPTED encoder on both sides
        // (D19: decisions are comparands, encoders are not); its data is the
        // shared `originalBuffer(originalLen)` pattern.
        let scripted: Option<(Vec<u8>, ScriptedTranscoder)> = c.get("shrink").map(|sh| {
            let original = original_bytes(sh["originalLen"].as_u64().unwrap() as usize);
            let script: CorpusScript = serde_json::from_value(sh["script"].clone()).unwrap();
            let t = ScriptedTranscoder::new().with_script(original.clone(), script.to_script());
            (original, t)
        });
        let data = match &scripted {
            Some((original, _)) => {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD.encode(original)
            }
            None => att["data"].as_str().unwrap_or("").to_string(),
        };
        let transcoder: &dyn quilltap_core::files::image_processing::ImageTranscoder =
            match &scripted {
                Some((_, t)) => t,
                None => &codec,
            };
        let attachment = CompletionAttachment {
            id: att["id"].as_str().unwrap().to_string(),
            filename: att["filename"].as_str().unwrap().to_string(),
            mime_type: att["mimeType"].as_str().unwrap().to_string(),
            data,
        };
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let provider = ScriptedCompletion {
            script: c["provider"].clone(),
            calls: Arc::clone(&calls),
        };
        let log_type: &'static str = match c["logType"].as_str().unwrap() {
            "IMAGE_DESCRIPTION" => {
                quilltap_core::services::llm_logging::log_type::IMAGE_DESCRIPTION
            }
            "WARDROBE_IMAGE_ANALYSIS" => "WARDROBE_IMAGE_ANALYSIS",
            other => panic!("unknown log type {other}"),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (result, lines) = captured_with(|| {
            rt.block_on(send_vision_request(
                &db,
                &provider,
                VisionRequestInput {
                    profile: &profile,
                    user_id: &user_id,
                    attachment: &attachment,
                    system_prompt: c.get("systemPrompt").and_then(Value::as_str),
                    instruction: c["instruction"].as_str().unwrap(),
                    sampling: VisionSampling {
                        temperature: c["sampling"].get("temperature").and_then(Value::as_f64),
                        max_tokens: c["sampling"].get("maxTokens").and_then(Value::as_i64),
                        top_p: c["sampling"].get("topP").and_then(Value::as_f64),
                    },
                    timeout_ms: c["timeoutMs"].as_u64().unwrap(),
                    log_type,
                    require_api_key: c["requireApiKey"].as_bool().unwrap_or(false),
                    transcoder,
                },
            ))
        });
        let result_json = match result {
            Ok(VisionRequestResult::Ok {
                response,
                sent_attachment,
            }) => json!({
                "ok": true,
                "content": response.content,
                "usage": response.usage.map(|u| json!({ "promptTokens": u.prompt_tokens, "completionTokens": u.completion_tokens, "totalTokens": u.total_tokens })),
                "sentAttachment": { "filename": sent_attachment.filename, "mimeType": sent_attachment.mime_type, "hasData": !sent_attachment.data.is_empty() },
            }),
            Ok(VisionRequestResult::Refused {
                refusal,
                detail,
                response,
            }) => json!({
                "ok": false,
                "refusal": refusal.as_str(),
                "detail": detail,
                "responseContent": response.map(|r| r.content),
            }),
            Err(e) => json!({ "threw": e.message }),
        };
        let got = json!({
            "result": result_json,
            "providerCalls": comparable_calls(&Value::Array(calls.lock().unwrap().clone())),
            "shrinkCalls": scripted.as_ref().map(|(o, t)| t.calls_for(o).iter().map(|r| json!({ "maxEdge": r.max_edge, "quality": r.quality })).collect::<Vec<_>>()).unwrap_or_default(),
            "logs": blank_duration(rust_lines(&lines, &LOG_PREFIXES)),
            "llmLogs": common::dump_llm_logs(&db),
        });
        let want_s = json!({
            "result": want["result"],
            "providerCalls": comparable_calls(&want["providerCalls"]),
            "shrinkCalls": want["shrinkCalls"],
            "logs": blank_duration(oracle_lines(&want["logs"])),
            "llmLogs": common::oracle_llm_logs(&want["llmLogs"]),
        });
        for part in ["result", "providerCalls", "shrinkCalls", "logs", "llmLogs"] {
            if got[part] != want_s[part] {
                failures.push(format!(
                    "{name}: {part} diverged\n  rust:   {}\n  oracle: {}",
                    got[part], want_s[part]
                ));
            }
        }
        // The negative pin: no `[Vision]` WARN but the arrival one.
        for l in rust_lines(&lines, &LOG_PREFIXES)
            .iter()
            .chain(oracle_lines(&want["logs"]).iter())
        {
            if l.starts_with("WARN ") {
                assert!(
                    l.contains("Model answered without the image"),
                    "{name}: an unexpected [Vision] WARN: {l}"
                );
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
