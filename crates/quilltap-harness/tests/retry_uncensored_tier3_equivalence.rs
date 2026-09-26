//! P4.D228 — "Try uncensored" (v4 `ce2f1dabf`, #77) against v4's REAL code on
//! a purpose-built encrypted fixture.
//!
//! Three tiers, one corpus (`harness/oracle/fixtures/retry-uncensored.json`),
//! run IN ORDER on ONE database copy on both sides (a case's writes are visible
//! to the next):
//!
//! - **service** — `services::dangerous_content::retry_uncensored` against v4's
//!   `lib/services/dangerous-content/retry-uncensored.ts`: the Locked refusal
//!   (no lookup), the text exclusion set (connection-kind trail ids + the
//!   responder's resolved profile + every profile on the MESSAGE's own
//!   provider+model — the exclusion `docs/developer/API.md` omits), the image
//!   exclusion set (the caller's ids with null/empty dropped + image-kind trail
//!   ids + the TOOL content's provider+model), the configured desk off duty and
//!   on an exempt chat (E.7), `composeRetryRouteTrail`'s bytes (the `answered`
//!   rows DROPPED, `profileKind` LAST), `mayRetryUncensored`, and
//!   `resolveConfiguredConciergeDesk`.
//! - **chat** — `api::chat_media::chat_retry_image_uncensored` against v4's REAL
//!   `POST /api/v1/chats/[id]?action=retry-image-uncensored`: the union decode
//!   (first branch wins, unknown keys stripped), the 404 / 400 / 409 order, the
//!   picture arm's trail, `+1 ms` filing, the saved TOOL row, the conditional
//!   `refusal-rerouted`, the 200 body (its trail rows in the chokepoint's
//!   in-memory key order) and the 502 `details` (`{}` with no code); the
//!   background arm's 409 BEFORE `regenerate-background`'s own 400s and the
//!   REAL enqueue (`forceUncensored` spread LAST; the dedupe drops it).
//! - **message** — `api::salon::message_retry_uncensored` against v4's REAL
//!   `POST /api/v1/chats/[id]/messages/[messageId]?action=retry-uncensored`:
//!   the gate order, the 409 kinds, and what the route hands the swipe
//!   service (the override profile + the trail it composed).
//!
//! The image generator and the swipe service are RECORDING boundaries on both
//! sides — each has its own differential (`image_generation_tier3`'s
//! `primary_via` arm; `regenerate_swipe_tier3`'s override arms). v4's own unit
//! tests for #77 mock the gate itself, so none of them is an oracle
//! (`a-v4-mock-is-not-v4-in-a-route-family`).
//!
//! **Recorded divergences (pinned below, both ways):**
//! - **the 502 is carried as `ErrorKind::Internal`** (HTTP 500 over dispatch):
//!   v5 has no 502 kind, and adding one ripples through ~60 exhaustive
//!   `ErrorKind` matches across lanes. The body is v4's — `{error, details:
//!   {code}}`, `details: {}` with no code. v4's own client reads only `res.ok`
//!   and the 409. STOP-and-recorded for the unifier (P4.D228 lane record).
//! - **v4's `404 Chat not found`** on the message route has no v5 analogue:
//!   the verb is RPC-only and carries no chat id (the chat is the message's
//!   own), so there is no URL chat to mismatch.
//! - **the stream leg** answers the SAME `Response` on v5 (the frames ride the
//!   engine's broadcast, P4.D207's shape); v4 answers a 200 SSE. The oracle
//!   records `stream: true`; the harness pins that the stream flag reached the
//!   emitter and the recorded call is v4's `streamSwipeRegeneration` one.
//!
//! Regenerate the oracle (Node 24, from a PINNED v4 worktree — the sweep
//! driver's `--v4`; stage OUTSIDE any `.claude/` path, v4's jest ignores it):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; TMPO=/tmp/qt-oracle-run-retry-uncensored
//!   cd ~/source/quilltap-server
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
//!   cp "$V5W/harness/oracle/cases/retry-uncensored-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/retry-uncensored.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/build-retry-uncensored-fixture.ts" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/lib/v4-migrations.ts" "$TMPO/lib/"
//!   rm -f /tmp/qt-retry-uncensored-main.db /tmp/qt-retry-uncensored-mount.db /tmp/oracle-retry-uncensored.ndjson
//!   TZ=UTC QT_FIXTURE_OUT=/tmp/qt-retry-uncensored-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-retry-uncensored-mount.db \
//!     $N/npx tsx "$TMPO/fixtures/build-retry-uncensored-fixture.ts"
//!   TZ=UTC QT_FIXTURE_RETRY_UNCENSORED_MAIN=/tmp/qt-retry-uncensored-main.db \
//!   QT_FIXTURE_RETRY_UNCENSORED_MOUNT=/tmp/qt-retry-uncensored-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-retry-uncensored.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/retry-uncensored-tier3\.test\.ts$"
//!   grep -c '"announced":true' /tmp/oracle-retry-uncensored.ndjson
//! Run:
//!   QT_ORACLE_RETRY_UNCENSORED=/tmp/oracle-retry-uncensored.ndjson \
//!   QT_FIXTURE_RETRY_UNCENSORED_MAIN=/tmp/qt-retry-uncensored-main.db \
//!   QT_FIXTURE_RETRY_UNCENSORED_MOUNT=/tmp/qt-retry-uncensored-mount.db \
//!     cargo test -p quilltap-harness --test retry_uncensored_tier3_equivalence -- --nocapture

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use quilltap_core::api::chat_send::{
    SwipeGenerateDriver, SwipeGenerateFuture, SwipeGenerateRequest,
};
use quilltap_core::api::types::{CoreError, ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::llm_fallback::FallbackTrigger;
use quilltap_core::services::dangerous_content::provider_routing::ApiKeyResolver;
use quilltap_core::services::dangerous_content::refusal::RefusalEvidence;
use quilltap_core::services::dangerous_content::resolver::resolve_configured_concierge_desk;
use quilltap_core::services::dangerous_content::retry_uncensored::{
    compose_retry_route_trail, may_retry_uncensored, resolve_image_retry_understudy,
    resolve_text_retry_understudy, AnsweredBy, AnsweringProfile, RetryProfileKind,
};
use quilltap_core::services::regenerate_swipe::SwipeProgressEmitter;
use quilltap_core::services::route_trail::{
    RouteAttempt, RouteAttemptOutcome, RouteAttemptVia, RouteProfileKind,
};
use quilltap_core::test_support::captured_with;
use quilltap_core::tools::generate_image::{
    ErasedImageGeneration, GeneratedImageResult, ImageGenerationRunner, ImageGenerationToolInput,
    ImageGenerationToolOutput, ImageToolExecutionContext,
};
use serde_json::{json, Map, Value};

struct CannedApiKeys(HashMap<String, String>);

impl ApiKeyResolver for CannedApiKeys {
    fn resolve(&self, api_key_id: &str, _user_id: &str) -> Option<String> {
        self.0.get(api_key_id).cloned()
    }
}

// ---------------------------------------------------------------------------
// The recording boundaries
// ---------------------------------------------------------------------------

/// Boundary 1 (v4 `executeImageGenerationTool`): records `(arguments,
/// context)` and answers the case's canned result.
struct CannedImageGeneration {
    calls: Arc<Mutex<Vec<Value>>>,
    result: Arc<Mutex<Option<Value>>>,
}

fn opt_str(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

/// A v4 trail row (JSON) as the port's [`RouteAttempt`] — harness-local, only
/// for the canned generator's `routeTrail`.
fn route_attempt_from_json(v: &Value) -> RouteAttempt {
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    RouteAttempt {
        profile_id: s("profileId"),
        profile_name: s("profileName"),
        provider: s("provider"),
        model_name: s("modelName"),
        via: match v["via"].as_str() {
            Some("concierge") => RouteAttemptVia::Concierge,
            Some("primary") => RouteAttemptVia::Primary,
            other => panic!("via {other:?}"),
        },
        outcome: match v["outcome"].as_str() {
            Some("answered") => RouteAttemptOutcome::Answered,
            Some("refused") => RouteAttemptOutcome::Refused,
            Some("failed") => RouteAttemptOutcome::Failed,
            other => panic!("outcome {other:?}"),
        },
        trigger: v.get("trigger").and_then(Value::as_str).map(|t| match t {
            "moderation-refusal" => FallbackTrigger::ModerationRefusal,
            "network" => FallbackTrigger::Network,
            other => panic!("trigger {other}"),
        }),
        evidence: v
            .get("evidence")
            .and_then(Value::as_str)
            .map(|e| RefusalEvidence::from_wire(e).unwrap_or_else(|| panic!("evidence {e}"))),
        profile_kind: v
            .get("profileKind")
            .and_then(Value::as_str)
            .map(|k| match k {
                "image" => RouteProfileKind::Image,
                _ => RouteProfileKind::Connection,
            }),
        detail: opt_str(v, "detail"),
    }
}

impl ImageGenerationRunner for CannedImageGeneration {
    fn run<'a>(
        &'a self,
        _db: &'a Db,
        input: &'a ImageGenerationToolInput,
        ctx: &'a ImageToolExecutionContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ImageGenerationToolOutput> + Send + 'a>>
    {
        Box::pin(async move {
            // v4 records the context object with `undefined` keys dropped.
            let mut context = Map::new();
            context.insert("userId".into(), json!(ctx.user_id));
            context.insert("profileId".into(), json!(ctx.profile_id));
            if let Some(c) = &ctx.chat_id {
                context.insert("chatId".into(), json!(c));
            }
            if let Some(p) = &ctx.calling_participant_id {
                context.insert("callingParticipantId".into(), json!(p));
            }
            if let Some(v) = ctx.primary_via {
                context.insert("primaryVia".into(), json!(v.as_str()));
            }
            self.calls.lock().unwrap().push(json!({
                "boundary": "executeImageGenerationTool",
                "input": input.raw_arguments.clone().unwrap_or(Value::Null),
                "context": Value::Object(context),
            }));
            let r = self
                .result
                .lock()
                .unwrap()
                .clone()
                .expect("no canned image result for this case");
            ImageGenerationToolOutput {
                success: r["success"].as_bool().unwrap_or(false),
                images: r["images"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .map(|i| GeneratedImageResult {
                        id: opt_str(i, "id").unwrap(),
                        url: opt_str(i, "url").unwrap(),
                        filename: opt_str(i, "filename").unwrap(),
                        revised_prompt: opt_str(i, "revisedPrompt"),
                        filepath: opt_str(i, "filepath"),
                        mime_type: opt_str(i, "mimeType"),
                        size: i["size"].as_f64(),
                        width: i["width"].as_f64(),
                        height: i["height"].as_f64(),
                        sha256: opt_str(i, "sha256"),
                    })
                    .collect(),
                error: opt_str(&r, "error"),
                message: opt_str(&r, "message"),
                provider: opt_str(&r, "provider"),
                model: opt_str(&r, "model"),
                expanded_prompt: opt_str(&r, "expandedPrompt"),
                route_trail: r["routeTrail"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .map(route_attempt_from_json)
                    .collect(),
            }
        })
    }
}

/// Boundary 2 (v4 `regenerateMessageAsSwipe` / `streamSwipeRegeneration`):
/// records what the route hands the service and answers a canned swipe (or
/// the case's error).
struct RecordingSwipeDriver {
    calls: Arc<Mutex<Vec<Value>>>,
    throws: Arc<Mutex<Option<String>>>,
}

impl SwipeGenerateDriver for RecordingSwipeDriver {
    fn generate_swipe(&self, req: SwipeGenerateRequest) -> SwipeGenerateFuture<'_> {
        Box::pin(async move {
            let override_row = req.profile_override.as_ref();
            let mut call = Map::new();
            call.insert(
                "boundary".into(),
                json!(if req.progress.is_active() {
                    "streamSwipeRegeneration"
                } else {
                    "regenerateMessageAsSwipe"
                }),
            );
            if req.progress.is_active() {
                call.insert(
                    "logContext".into(),
                    json!("[DangerousContent] Uncensored retry:"),
                );
            }
            call.insert("targetMessageId".into(), req.target_message["id"].clone());
            call.insert("chatId".into(), req.chat["id"].clone());
            call.insert("allMessageCount".into(), json!(req.all_messages.len()));
            call.insert(
                "activeUserParticipantId".into(),
                json!(req.active_user_participant_id),
            );
            call.insert(
                "overrideProfileId".into(),
                override_row.map(|r| r["id"].clone()).unwrap_or(Value::Null),
            );
            call.insert(
                "overrideProfileName".into(),
                override_row
                    .map(|r| r["name"].clone())
                    .unwrap_or(Value::Null),
            );
            // v4 hands the understudy's decrypted key through the override;
            // v5's key is per provider at the transport (the standing
            // provider-I/O ruling) — the harness reads it off the canned
            // resolver by the row's `apiKeyId` so the comparand stays whole.
            call.insert(
                "overrideApiKey".into(),
                override_row
                    .and_then(|r| r.get("__cannedApiKey").cloned())
                    .unwrap_or(Value::Null),
            );
            call.insert(
                "routeTrail".into(),
                req.route_trail
                    .clone()
                    .map(Value::Array)
                    .unwrap_or(Value::Null),
            );
            self.calls.lock().unwrap().push(Value::Object(call));
            if let Some(msg) = self.throws.lock().unwrap().clone() {
                return Err(CoreError {
                    kind: ErrorKind::Internal,
                    message: msg,
                    pepper_state: None,
                    code: None,
                    associations: None,
                    character_id: None,
                    entity: None,
                    details: None,
                    already_saved: None,
                });
            }
            Ok(
                json!({ "id": "canned-swipe", "type": "message", "role": "ASSISTANT", "content": "canned" }),
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Log rendering (v4 bag → the capture rig's line)
// ---------------------------------------------------------------------------

fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

const TARGETS: [&str; 4] = [
    "quilltap::concierge_retry_uncensored",
    "quilltap::concierge_retry",
    "quilltap::chats",
    "quilltap::concierge_notification",
];

fn render_v4(log: &Value) -> String {
    let message = log["message"].as_str().unwrap();
    let target = match (log["service"].as_str(), message) {
        (Some("ConciergeRetryUncensored"), _) => "quilltap::concierge_retry_uncensored",
        (None, m) if m.starts_with("[DangerousContent]") => "quilltap::concierge_retry",
        (None, m) if m.starts_with("[Chats v1]") => "quilltap::chats",
        (None, m) if m.starts_with("[ConciergeNotification]") => "quilltap::concierge_notification",
        other => panic!("unexpected log {other:?}"),
    };
    let level = log["level"].as_str().unwrap().to_uppercase();
    let mut line = format!("{level} {target} {message}");
    for (k, v) in log["bag"].as_object().unwrap() {
        match v {
            // v4 `?? null` / `undefined` — the port omits the field.
            Value::Null => {}
            Value::String(s) => line.push_str(&format!(" {}={s}", snake(k))),
            // Arrays / objects ride the file layer's `…Json` convention.
            Value::Array(_) | Value::Object(_) => line.push_str(&format!(" {k}Json={v}")),
            other => line.push_str(&format!(" {}={other}", snake(k))),
        }
    }
    line
}

/// Minted ids (tool rows, announcements, jobs) are placeholdered by the
/// ORDER they are first seen in a case, on both sides.
struct Minted(Vec<String>);

impl Minted {
    fn norm(&mut self, s: &str) -> String {
        let mut out = s.to_string();
        for (i, id) in self.0.iter().enumerate() {
            out = out.replace(id.as_str(), &format!("<minted{i}>"));
        }
        out
    }
    fn learn(&mut self, id: &str) {
        if !self.0.iter().any(|x| x == id) {
            self.0.push(id.to_string());
        }
    }
}

/// Rows a case wrote, normalized: minted ids placeholdered, a minted
/// `createdAt` / `updatedAt` / `scheduledAt` / `startedAt` stamped `<ts>` —
/// EXCEPT a TOOL row's `createdAt`, which IS the `+1 ms` comparand.
fn norm_rows(rows: &[Value], minted: &mut Minted) -> Vec<Value> {
    rows.iter()
        .map(|r| {
            let mut o = r.as_object().unwrap().clone();
            if let Some(id) = o.get("id").and_then(Value::as_str) {
                minted.learn(id);
            }
            let is_tool = o.get("role").and_then(Value::as_str) == Some("TOOL");
            for col in ["createdAt", "updatedAt", "scheduledAt", "startedAt"] {
                if o.contains_key(col) && !(is_tool && col == "createdAt") && !o[col].is_null() {
                    o.insert(col.into(), json!("<ts>"));
                }
            }
            let text = minted.norm(&Value::Object(o).to_string());
            serde_json::from_str(&text).unwrap()
        })
        .collect()
}

fn read_new_rows(db: &Db, table: &str, before: &[String]) -> Vec<Value> {
    let dump = db
        .read_main(|c| quilltap_core::db::dump_table_json_conn(c, table, "rowid"))
        .expect("dump");
    dump["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["id"]
                .as_str()
                .is_some_and(|id| !before.iter().any(|b| b == id))
        })
        .map(|r| {
            // Fractional-zero floats → integers (the dump's REAL columns).
            let mut o = r.as_object().unwrap().clone();
            for v in o.values_mut() {
                if v.is_f64() && v.as_f64().unwrap().fract() == 0.0 {
                    *v = json!(v.as_f64().unwrap() as i64);
                }
            }
            Value::Object(o)
        })
        .collect()
}

fn ids_of(db: &Db, table: &str) -> Vec<String> {
    let sql = format!("SELECT id FROM {table}");
    db.read_main(move |c| {
        let mut st = c.prepare(&sql)?;
        let ids = st
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    })
    .expect("ids")
}

fn response_json(r: &Response) -> Value {
    match r {
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                // RECORDED DIVERGENCE: v4 answers 502; v5 has no 502 kind.
                ErrorKind::Internal if e.details.is_some() => 502,
                ErrorKind::Internal => 500,
                ref other => panic!("unexpected kind {other:?}"),
            };
            let mut body = Map::new();
            body.insert("error".into(), json!(e.message));
            if let Some(d) = &e.details {
                body.insert("details".into(), (**d).clone());
            }
            json!({ "status": status, "body": Value::Object(body) })
        }
        Response::ChatMedia(v) => json!({ "status": 200, "body": v }),
        Response::Message(v) => json!({ "status": 201, "body": v }),
        other => panic!("unexpected response {other:?}"),
    }
}

#[test]
fn retry_uncensored_matches_v4() {
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_RETRY_UNCENSORED"),
        std::env::var("QT_FIXTURE_RETRY_UNCENSORED_MAIN"),
        std::env::var("QT_FIXTURE_RETRY_UNCENSORED_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_RETRY_UNCENSORED + QT_FIXTURE_RETRY_UNCENSORED_MAIN/_MOUNT (see header)."
        );
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/retry-uncensored.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();
    let cases = spec["cases"].as_array().unwrap();
    assert_eq!(oracle.len(), cases.len(), "one oracle row per case");

    let keys: HashMap<String, String> = spec["apiKeys"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
    let api_keys = CannedApiKeys(keys.clone());

    let scratch =
        std::env::temp_dir().join(format!("qt-retry-uncensored-rust-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch");
    let work_main = scratch.join("main.db");
    let work_mount = scratch.join("mount.db");
    std::fs::copy(&fixture_main, &work_main).expect("copy main");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    let db = rt.block_on(async {
        Db::open(
            DbPaths {
                main: work_main.clone(),
                mount_index: Some(work_mount.clone()),
                llm_logs: None,
            },
            &pepper,
        )
        .expect("open fixture copy")
    });

    let image_calls = Arc::new(Mutex::new(Vec::new()));
    let image_result = Arc::new(Mutex::new(None));
    let image_generation = ErasedImageGeneration::new(CannedImageGeneration {
        calls: image_calls.clone(),
        result: image_result.clone(),
    });
    let swipe_calls = Arc::new(Mutex::new(Vec::new()));
    let swipe_throws = Arc::new(Mutex::new(None));
    let driver = RecordingSwipeDriver {
        calls: swipe_calls.clone(),
        throws: swipe_throws.clone(),
    };
    let (events, _keep) = tokio::sync::broadcast::channel(64);

    let mut failed: Vec<String> = Vec::new();
    let mut announced = 0usize;
    for (case, want) in cases.iter().zip(&oracle) {
        let name = case["name"].as_str().unwrap();
        assert_eq!(want["name"], case["name"], "oracle rows in spec order");
        let user_id = case["userId"].as_str().unwrap_or("").to_string();

        // The case's settings patch — v4's recorded SQL, replayed verbatim.
        let patches = want["patches"].as_array().unwrap().clone();
        let mut reverts: Vec<(String, String)> = Vec::new();
        for p in &patches {
            let sql = p["sql"].as_str().unwrap().to_string();
            let params: Vec<Value> = p["params"].as_array().unwrap().clone();
            if let Some(rest) = sql.strip_prefix("UPDATE chat_settings SET ") {
                let col = rest.split(' ').next().unwrap().to_string();
                let uid = params[1].as_str().unwrap().to_string();
                let prior: String = db
                    .read_main(|c| {
                        Ok(c.query_row(
                            &format!("SELECT {col} FROM chat_settings WHERE userId = ?1"),
                            [&uid],
                            |r| r.get(0),
                        )?)
                    })
                    .unwrap();
                reverts.push((col, prior));
            }
            rt.block_on(db.write(move |w| {
                let rparams: Vec<rusqlite::types::Value> = params
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                        Value::Number(n) => rusqlite::types::Value::Integer(n.as_i64().unwrap()),
                        Value::Null => rusqlite::types::Value::Null,
                        other => rusqlite::types::Value::Text(other.to_string()),
                    })
                    .collect();
                w.main()
                    .connection()
                    .execute(&sql, rusqlite::params_from_iter(rparams))?;
                Ok(())
            }))
            .expect("apply patch");
        }

        image_calls.lock().unwrap().clear();
        swipe_calls.lock().unwrap().clear();
        *image_result.lock().unwrap() = case.get("imageResult").cloned();
        *swipe_throws.lock().unwrap() = case["serviceThrows"].as_str().map(str::to_string);
        let msgs_before = ids_of(&db, "chat_messages");
        let jobs_before = ids_of(&db, "background_jobs");

        let chat_of = |id: &str| -> Value {
            let id = id.to_string();
            db.read_main(|c| quilltap_core::db::chats_read::find_by_id(c, &id))
                .unwrap()
                .unwrap()
        };
        let settings_of = |uid: &str| -> Option<Value> {
            let uid = uid.to_string();
            db.read_main(|c| quilltap_core::db::chat_settings::find_by_user_id(c, &uid))
                .unwrap()
        };
        let message_of = |chat_id: &str, mid: &str| -> Value {
            let (cid, mid) = (chat_id.to_string(), mid.to_string());
            let all = db
                .read_main(|c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
                .unwrap();
            all.into_iter()
                .find(|m| m["id"].as_str() == Some(mid.as_str()))
                .unwrap_or_else(|| panic!("{name}: message {mid} not seeded"))
        };
        let summarize = |r: Result<
            quilltap_core::services::dangerous_content::understudy::Understudy,
            quilltap_core::services::dangerous_content::retry_uncensored::RetryUncensoredRefusal,
        >| match r {
            Ok(u) => {
                json!({ "ok": true, "understudyProfileId": u.profile.id, "apiKey": u.api_key })
            }
            Err(reason) => json!({ "ok": false, "reason": reason.as_str() }),
        };

        let (got_result, lines) = captured_with(|| -> Value {
            match case["kind"].as_str().unwrap() {
                "service" => match case["fn"].as_str().unwrap() {
                    "text" => {
                        let chat = chat_of(case["chatId"].as_str().unwrap());
                        let msg = message_of(
                            case["chatId"].as_str().unwrap(),
                            case["messageId"].as_str().unwrap(),
                        );
                        let settings = settings_of(&user_id);
                        summarize(rt.block_on(resolve_text_retry_understudy(
                            &db,
                            &api_keys,
                            &user_id,
                            &chat,
                            settings.as_ref(),
                            &msg,
                        )))
                    }
                    "image" => {
                        let chat = chat_of(case["chatId"].as_str().unwrap());
                        let settings = settings_of(&user_id);
                        let exclude: Vec<Option<String>> = if case["exclude"] == json!("chat") {
                            vec![chat["imageProfileId"].as_str().map(str::to_string)]
                        } else {
                            case["exclude"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|v| v.as_str().map(str::to_string))
                                .collect()
                        };
                        let exclude_refs: Vec<Option<&str>> =
                            exclude.iter().map(|o| o.as_deref()).collect();
                        let (trail, content) = if case["answeredFromTool"].as_bool() == Some(true) {
                            let msg = message_of(
                                case["chatId"].as_str().unwrap(),
                                case["messageId"].as_str().unwrap(),
                            );
                            let content: Value =
                                serde_json::from_str(msg["content"].as_str().unwrap()).unwrap();
                            (msg.get("routeTrail").cloned(), Some(content))
                        } else {
                            (None, None)
                        };
                        let answered_by = content
                            .as_ref()
                            .map(|c| AnsweredBy {
                                provider: c["provider"].as_str(),
                                model_name: c["model"].as_str(),
                            })
                            .unwrap_or_default();
                        summarize(rt.block_on(resolve_image_retry_understudy(
                            &db,
                            &api_keys,
                            &user_id,
                            &chat,
                            settings.as_ref(),
                            &exclude_refs,
                            trail.as_ref(),
                            answered_by,
                        )))
                    }
                    "compose" => {
                        let msg = message_of(
                            case["chatId"].as_str().unwrap(),
                            case["messageId"].as_str().unwrap(),
                        );
                        let aid = case["answering"].as_str().unwrap().to_string();
                        let kind = if case["profileKind"] == "image" {
                            RetryProfileKind::Image
                        } else {
                            RetryProfileKind::Connection
                        };
                        let row = db
                            .read_main(|c| {
                                if kind == RetryProfileKind::Image {
                                    quilltap_core::db::image_profiles::find_by_id(c, &aid)
                                } else {
                                    quilltap_core::db::connection_profiles::find_by_id(c, &aid)
                                }
                            })
                            .unwrap()
                            .unwrap();
                        let s = |k: &str| row[k].as_str().unwrap_or("").to_string();
                        let (id, nm, pr, md) = (s("id"), s("name"), s("provider"), s("modelName"));
                        let trail = compose_retry_route_trail(
                            msg.get("routeTrail"),
                            AnsweringProfile {
                                id: &id,
                                name: &nm,
                                provider: &pr,
                                model_name: &md,
                            },
                            kind,
                        );
                        json!({ "trailJson": Value::Array(trail).to_string() })
                    }
                    "may" => {
                        let mut out = Map::new();
                        for id in case["chatIds"].as_array().unwrap() {
                            let id = id.as_str().unwrap();
                            out.insert(id.into(), json!(may_retry_uncensored(Some(&chat_of(id)))));
                        }
                        json!({ "may": out })
                    }
                    "desk" => {
                        let desks: Vec<Value> = case["settingsList"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|s| {
                                let carrier = (!s.is_null()).then(|| {
                                    let (parsed, issues) =
                                        quilltap_core::db::chat_settings::zod_parse_concierge_settings(s);
                                    assert!(issues.is_empty());
                                    json!({ "conciergeSettings": parsed })
                                });
                                serde_json::to_value(resolve_configured_concierge_desk(carrier.as_ref()))
                                    .unwrap()
                            })
                            .collect();
                        json!({ "desks": desks })
                    }
                    other => panic!("fn {other}"),
                },
                "chat" => {
                    let body = if case["malformed"].as_bool() == Some(true) {
                        // Over dispatch a malformed body cannot arrive (the
                        // dispatch JSON parse refuses it first); v4 reads it as
                        // `{}`, which is what the verb is handed here.
                        json!({})
                    } else {
                        case["body"].clone()
                    };
                    let r =
                        rt.block_on(quilltap_core::api::chat_media::chat_retry_image_uncensored(
                            &db,
                            &api_keys,
                            Some(&image_generation),
                            &user_id,
                            case["chatId"].as_str().unwrap(),
                            &body,
                        ));
                    response_json(&r)
                }
                "message" => {
                    let stream = case["stream"].as_bool() == Some(true);
                    let mid = case["messageId"].as_str().unwrap();
                    let r = rt.block_on(quilltap_core::api::salon::message_retry_uncensored(
                        &db,
                        &api_keys,
                        Some(&driver as &dyn SwipeGenerateDriver),
                        &user_id,
                        mid,
                        SwipeProgressEmitter::from_flag(stream, mid, events.clone()),
                    ));
                    let got = response_json(&r);
                    if stream && got["status"] == 201 {
                        json!({ "status": 200, "stream": true })
                    } else {
                        got
                    }
                }
                other => panic!("kind {other}"),
            }
        });

        let mut minted = Minted(Vec::new());
        let got_msgs = norm_rows(
            &read_new_rows(&db, "chat_messages", &msgs_before),
            &mut minted,
        );
        let got_jobs = norm_rows(
            &read_new_rows(&db, "background_jobs", &jobs_before),
            &mut minted,
        );
        let mut want_minted = Minted(Vec::new());
        let want_msgs = norm_rows(want["newMessages"].as_array().unwrap(), &mut want_minted);
        let want_jobs = norm_rows(want["newJobs"].as_array().unwrap(), &mut want_minted);
        for (col, prior) in reverts.into_iter().rev() {
            let uid = user_id.clone();
            rt.block_on(db.write(move |w| {
                w.main().connection().execute(
                    &format!("UPDATE chat_settings SET {col} = ?1 WHERE userId = ?2"),
                    rusqlite::params![prior, uid],
                )?;
                Ok(())
            }))
            .unwrap();
        }
        if case["plantJob"].as_bool() == Some(true) {
            rt.block_on(db.write(|w| {
                w.main().connection().execute(
                    "DELETE FROM background_jobs WHERE id = 'b9000001-0000-4000-8000-000000000001'",
                    [],
                )?;
                Ok(())
            }))
            .unwrap();
        }

        let norm_value = |v: &Value, m: &mut Minted| -> Value {
            let text = v.to_string();
            let text = m.norm(&text);
            serde_json::from_str(&text).unwrap()
        };
        let got_result = norm_value(&got_result, &mut minted);
        let want_result = norm_value(&want["result"], &mut want_minted);
        let mut got_calls: Vec<Value> = image_calls.lock().unwrap().clone();
        got_calls.extend(swipe_calls.lock().unwrap().iter().cloned());
        let got_lines: Vec<String> = lines
            .iter()
            .filter(|l| TARGETS.contains(&l.split(' ').nth(1).unwrap_or("")))
            .map(|l| minted.norm(l))
            .collect();
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| want_minted.norm(&render_v4(l)))
            .collect();
        announced += want_lines
            .iter()
            .filter(|l| l.contains("announced=true"))
            .count();

        // The v4 canned key rides the override on v4's side only; compare it
        // from the canned resolver by the understudy's id.
        let want_calls: Vec<Value> = want["calls"].as_array().unwrap().to_vec();
        let got_calls: Vec<Value> = got_calls
            .into_iter()
            .map(|mut c| {
                if let Some(pid) = c.get("overrideProfileId").and_then(Value::as_str) {
                    let pid = pid.to_string();
                    let row = db
                        .read_main(|cn| {
                            quilltap_core::db::connection_profiles::find_by_id(cn, &pid)
                        })
                        .unwrap()
                        .unwrap();
                    c["overrideApiKey"] = json!(keys[row["apiKeyId"].as_str().unwrap()].clone());
                }
                c
            })
            .collect();

        let mut diffs = Vec::new();
        // The body's trail rows are compared on BYTES: the 200 body carries
        // the chokepoint's in-memory key order for the understudy's own rows.
        let (got_bytes, want_bytes) = (got_result.to_string(), want_result.to_string());
        if got_bytes != want_bytes {
            diffs.push(format!(
                "result\n      rust   {got_result}\n      oracle {want_result}"
            ));
        }
        let got_call_bytes = Value::Array(got_calls.clone()).to_string();
        let want_call_bytes = Value::Array(want_calls.clone()).to_string();
        if got_call_bytes != want_call_bytes {
            diffs.push(format!(
                "calls\n      rust   {}\n      oracle {}",
                Value::Array(got_calls),
                Value::Array(want_calls)
            ));
        }
        if got_lines != want_lines {
            diffs.push(format!(
                "logs\n      rust   {got_lines:#?}\n      oracle {want_lines:#?}"
            ));
        }
        if got_msgs != want_msgs {
            diffs.push(format!(
                "newMessages\n      rust   {}\n      oracle {}",
                Value::Array(got_msgs),
                Value::Array(want_msgs)
            ));
        }
        if got_jobs != want_jobs {
            diffs.push(format!(
                "newJobs\n      rust   {}\n      oracle {}",
                Value::Array(got_jobs),
                Value::Array(want_jobs)
            ));
        }
        if !diffs.is_empty() {
            failed.push(format!("{name}:\n    {}", diffs.join("\n    ")));
        }
    }
    assert!(announced > 0, "the corpus announced no refusal-rerouted");
    assert!(
        failed.is_empty(),
        "{} of {} cases differ:\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
    eprintln!(
        "retry_uncensored_tier3: {} cases match v4 ({announced} announced)",
        cases.len()
    );
}
